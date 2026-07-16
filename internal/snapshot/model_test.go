package snapshot

import (
	"reflect"
	"testing"
	"time"
)

func TestCaptureManifestLayersAreDerivedFromTreeState(t *testing.T) {
	t.Parallel()

	base := validTestEntry("base.txt", "base")
	target := validTestEntry("target.txt", "target")
	baseDigest, err := ManifestDigest([]Entry{base})
	if err != nil {
		t.Fatal(err)
	}
	targetDigest, err := ManifestDigest([]Entry{target})
	if err != nil {
		t.Fatal(err)
	}
	capture := Capture{
		ComparisonKind:      ComparisonTwoDot,
		RequestedComparison: "base..target",
		BaseOID:             "requested-base",
		EffectiveBaseOID:    "effective-base",
		ObjectFormat:        "sha256",
		ContextPolicyHash:   DefaultContextPolicyHash(),
		CapturedAt:          time.Date(2026, time.July, 15, 12, 0, 0, 0, time.UTC),
		Base:                TreeState{OID: "effective-base", Entries: []Entry{base}, ManifestDigest: baseDigest},
		Target:              TreeState{OID: "target", Entries: []Entry{target}, ManifestDigest: targetDigest},
	}
	capture.Changes = BuildChanges(capture.Base.Entries, capture.Target.Entries)
	capture.ManifestDigest, err = OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}

	wantLayers := []Layer{
		{Name: TreeSideBase, Identity: "effective-base", ManifestDigest: baseDigest},
		{Name: TreeSideTarget, Identity: "target", ManifestDigest: targetDigest},
	}
	if layers := capture.ManifestLayers(); !reflect.DeepEqual(layers, wantLayers) {
		t.Fatalf("ManifestLayers() = %#v, want %#v", layers, wantLayers)
	}
	if err := capture.Validate(); err != nil {
		t.Fatalf("Capture.Validate() error = %v", err)
	}

	firstDigest := capture.ManifestDigest
	capture.Target.OID = "different-target"
	secondDigest, err := OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	if firstDigest == secondDigest {
		t.Fatal("overall manifest digest ignored the authoritative target tree identity")
	}
}

func TestWorktreeManifestLayersUseNamedTreeStates(t *testing.T) {
	t.Parallel()

	head := validTestEntry("head.txt", "head")
	index := validTestEntry("index.txt", "index")
	worktree := validTestEntry("worktree.txt", "worktree")
	headsDigest := mustManifestDigest(t, []Entry{head})
	indexDigest := mustManifestDigest(t, []Entry{index})
	worktreeDigest := mustManifestDigest(t, []Entry{worktree})
	capture := Capture{
		ComparisonKind:      ComparisonWorktree,
		RequestedComparison: WorktreeComparison,
		BaseOID:             "head-commit",
		EffectiveBaseOID:    "head-commit",
		ObjectFormat:        "sha1",
		ContextPolicyHash:   DefaultContextPolicyHash(),
		CapturedAt:          time.Date(2026, time.July, 15, 13, 0, 0, 0, time.UTC),
		Base:                TreeState{OID: "head-commit", Entries: []Entry{head}, ManifestDigest: headsDigest},
		Target:              TreeState{OID: worktreeDigest, Entries: []Entry{worktree}, ManifestDigest: worktreeDigest},
		Head:                TreeState{OID: "head-commit", Entries: []Entry{head}, ManifestDigest: headsDigest},
		Index:               TreeState{OID: "index-file", Entries: []Entry{index}, ManifestDigest: indexDigest},
		Worktree:            TreeState{OID: worktreeDigest, Entries: []Entry{worktree}, ManifestDigest: worktreeDigest},
	}
	capture.Changes = BuildChanges(capture.Base.Entries, capture.Target.Entries)
	capture.ManifestDigest = mustOverallManifestDigest(t, capture)

	wantLayers := []Layer{
		{Name: TreeSideHead, Identity: "head-commit", ManifestDigest: headsDigest},
		{Name: TreeSideIndex, Identity: "index-file", ManifestDigest: indexDigest},
		{Name: TreeSideWorktree, Identity: worktreeDigest, ManifestDigest: worktreeDigest},
	}
	if layers := capture.ManifestLayers(); !reflect.DeepEqual(layers, wantLayers) {
		t.Fatalf("ManifestLayers() = %#v, want %#v", layers, wantLayers)
	}
	if err := capture.Validate(); err != nil {
		t.Fatalf("Capture.Validate() error = %v", err)
	}
	capture.Head.OID = "different-head"
	if err := capture.Validate(); err == nil {
		t.Fatal("Capture.Validate() accepted disagreement between base and head tree state")
	}
}

func validTestEntry(path, oid string) Entry {
	return Entry{Path: path, Kind: EntryKindFile, Mode: 0o100644, Size: 1, ContentDigest: oid, GitOID: oid}
}

func mustManifestDigest(t *testing.T, entries []Entry) string {
	t.Helper()
	digest, err := ManifestDigest(entries)
	if err != nil {
		t.Fatal(err)
	}
	return digest
}

func mustOverallManifestDigest(t *testing.T, capture Capture) string {
	t.Helper()
	digest, err := OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	return digest
}
