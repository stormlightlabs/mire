package export

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestBuildAndRenderAreDeterministicAndDoNotEmbedSnapshotContent(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	stateDir := t.TempDir()
	database, err := db.OpenState(ctx, stateDir)
	if err != nil {
		t.Fatal(err)
	}
	store := db.NewRepositoryStore(database)
	t.Cleanup(func() { _ = store.Close() })
	objects, err := snapshot.OpenObjectStore(stateDir)
	if err != nil {
		t.Fatal(err)
	}
	baseObject, err := objects.Put(ctx, bytes.NewBufferString("before\n"))
	if err != nil {
		t.Fatal(err)
	}
	targetObject, err := objects.Put(ctx, bytes.NewBufferString("after\n"))
	if err != nil {
		t.Fatal(err)
	}
	baseEntries := []snapshot.Entry{{Path: "a.txt", Kind: snapshot.EntryKindFile, Mode: 0o644, Size: 7, ContentDigest: baseObject.Digest}}
	targetEntries := []snapshot.Entry{{Path: "a.txt", Kind: snapshot.EntryKindFile, Mode: 0o644, Size: 6, ContentDigest: targetObject.Digest}}
	changes := snapshot.BuildChanges(baseEntries, targetEntries)
	baseDigest, _ := snapshot.ManifestDigest(baseEntries)
	targetDigest, _ := snapshot.ManifestDigest(targetEntries)
	capture := snapshot.Capture{ComparisonKind: snapshot.ComparisonTwoDot, RequestedComparison: "base..target", BaseOID: "base", EffectiveBaseOID: "base", TargetOID: "target", ObjectFormat: "sha256", ContextPolicyHash: snapshot.DefaultContextPolicyHash(), CapturedAt: time.Date(2026, 7, 15, 12, 0, 0, 0, time.UTC), BaseEntries: baseEntries, TargetEntries: targetEntries, Changes: changes, BaseManifestDigest: baseDigest, TargetManifestDigest: targetDigest, Layers: []snapshot.Layer{{Name: snapshot.TreeSideBase, Identity: "base", ManifestDigest: baseDigest}, {Name: snapshot.TreeSideTarget, Identity: "target", ManifestDigest: targetDigest}}}
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	session, round, _, err := store.CreateCapturedSession(ctx, db.RepositoryIdentity{CanonicalIdentity: "/tmp/export-fixture", DisplayName: "fixture", DiscoveredGitDir: "/tmp/export-fixture/.git"}, "fixture", capture)
	if err != nil {
		t.Fatal(err)
	}
	persisted, _ := store.GetSnapshot(ctx, round.SnapshotID)
	reconstructed, reconstructErr := captureFromStore(ctx, store, persisted)
	if reconstructErr != nil {
		t.Fatal(reconstructErr)
	}
	if digest, digestErr := snapshot.OverallManifestDigest(reconstructed); digestErr != nil || digest != persisted.ManifestDigest {
		t.Fatalf("reconstructed=%#v digest=%s err=%v persisted=%#v manifest=%s layers=%#v", reconstructed, digest, digestErr, persisted, persisted.ManifestDigest, reconstructed.Layers)
	}
	value, err := Build(ctx, store, session, round, objects)
	if err != nil {
		t.Fatal(err)
	}
	if value.Change.Digest == "" || !strings.Contains(value.DiffPatch, "before") || !strings.Contains(value.DiffPatch, "after") {
		t.Fatalf("change projection = %#v, diff = %q, omissions = %#v", value.Change, value.DiffPatch, value.Omissions)
	}
	value.Coverage.RetrievedArtifacts = []review.RetrievedArtifact{{ID: "artifact", Content: "before\n", Digest: "digest", Size: 7}}
	canonical, err := CanonicalJSON(value)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(canonical), "before\n") || strings.Contains(string(canonical), "after\n") {
		t.Fatalf("canonical review contains snapshot object content: %s", canonical)
	}
	repeated, err := CanonicalJSON(value)
	if err != nil {
		t.Fatal(err)
	}
	if string(canonical) != string(repeated) {
		t.Fatal("canonical JSON changed between identical exports")
	}
	rendered, err := Render(value)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(rendered.SARIF, []byte(`"version": "2.1.0"`)) {
		t.Fatalf("SARIF = %s", rendered.SARIF)
	}

	destination := filepath.Join(t.TempDir(), "review.json")
	if err := Write(value, FormatJSON, destination); err != nil {
		t.Fatal(err)
	}
	if err := Write(value, FormatJSON, destination); err == nil {
		t.Fatal("Write() overwrote an existing destination")
	}
	var decoded map[string]any
	data, err := os.ReadFile(destination)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatal(err)
	}
}

func TestBundleContainsOnlyNamedFiles(t *testing.T) {
	t.Parallel()
	value := Review{SchemaVersion: SchemaVersion, ExportKind: "portable_review_projection", Session: SessionDescriptor{ID: "session", RepositoryIdentity: "repo"}, Round: RoundDescriptor{ID: "round", Status: string(db.RoundStatusComplete)}, SnapshotManifest: SnapshotManifest{ID: "snapshot", ManifestDigest: "manifest"}, Coverage: emptyCoverage(), Ledger: Ledger{Passes: []review.PassCoverage{}, Diagnostics: []review.ReviewDiagnostic{}, Candidates: []CandidateProjection{}, Findings: []FindingProjection{}, Verifications: []review.VerificationRecord{}, Dispositions: []review.DispositionRecord{}, Presentations: []review.PresentationRecord{}}, Chat: []review.ChatMessage{}, Omissions: []Omission{}}
	destination := filepath.Join(t.TempDir(), "bundle")
	if err := Write(value, FormatBundle, destination); err != nil {
		t.Fatal(err)
	}
	want := []string{"REVIEW.md", "review.json", "manifest.json", "diff.patch", "findings.json", "evidence.jsonl", "chat.jsonl", "activity.jsonl", "findings.sarif"}
	for _, name := range want {
		if _, err := os.Stat(filepath.Join(destination, name)); err != nil {
			t.Errorf("bundle member %s: %v", name, err)
		}
	}
	entries, err := os.ReadDir(destination)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != len(want) {
		t.Fatalf("bundle entries = %d, want %d", len(entries), len(want))
	}
}
