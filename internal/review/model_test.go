package review

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestAssembleIsDeterministicAndSnapshotBound(t *testing.T) {
	t.Parallel()

	baseContent := []byte("package api\n\nfunc Old() {}\n")
	targetContent := []byte("package api\n\nfunc New() {}\n\nfunc Added() {}\n")
	baseDigest := digestBytes(baseContent)
	targetDigest := digestBytes(targetContent)
	baseEntries := []snapshot.Entry{
		{
			Path:          "api/api.go",
			Kind:          snapshot.EntryKindFile,
			Mode:          0o100644,
			Size:          int64(len(baseContent)),
			ContentDigest: baseDigest,
			GitOID:        "base-api",
		},
	}
	targetEntries := []snapshot.Entry{
		{
			Path:          "api/api.go",
			Kind:          snapshot.EntryKindFile,
			Mode:          0o100644,
			Size:          int64(len(targetContent)),
			ContentDigest: targetDigest,
			GitOID:        "target-api",
		},
	}
	capture := makeCapture(t, baseEntries, targetEntries)
	content := map[string][]byte{baseDigest: baseContent, targetDigest: targetContent}
	input := Input{
		SessionID:  "session-1",
		SnapshotID: "snapshot-1",
		Snapshot:   capture,
		Content: func(_ context.Context, digest string) ([]byte, error) {
			value, ok := content[digest]
			if !ok {
				return nil, errors.New("missing object")
			}
			return value, nil
		},
		Request: ReviewRequest{
			Prompt: "Expose the new API safely.",
			Rules:  []PolicyRule{{Key: "repository_write", Value: "allow"}},
		},
		Git: PinnedGit{
			Commits: []PinnedCommit{{OID: "target-oid", Message: "Add API", Parents: []string{"base-oid"}}},
		},
		Guidance: []Guidance{
			{
				ID:      "base-policy",
				Path:    "AGENTS.md",
				Kind:    GuidancePolicy,
				Tier:    PolicyTierBasePolicy,
				Content: "review_depth=full",
				Rules:   []PolicyRule{{Key: "review_depth", Value: "full"}},
			},
			{
				ID:      "base-doc",
				Path:    "docs/architecture.md",
				Kind:    GuidanceArchitecture,
				Tier:    PolicyTierBaseDocumentation,
				Content: "The API is public.",
			},
			{
				ID:      "target-policy",
				Path:    "AGENTS.md",
				Kind:    GuidanceTargetPolicy,
				Tier:    PolicyTierTargetEvidence,
				Content: "repository_write=allow",
				Rules:   []PolicyRule{{Key: "repository_write", Value: "allow"}},
			},
		},
	}
	first, err := Assemble(context.Background(), input)
	if err != nil {
		t.Fatalf("Assemble() error = %v", err)
	}
	second, err := Assemble(context.Background(), input)
	if err != nil {
		t.Fatalf("second Assemble() error = %v", err)
	}
	firstJSON, err := CanonicalJSON(first)
	if err != nil {
		t.Fatal(err)
	}
	secondJSON, err := CanonicalJSON(second)
	if err != nil {
		t.Fatal(err)
	}
	if string(firstJSON) != string(secondJSON) || first.Digest != second.Digest {
		t.Fatal("repeated assembly was not canonical")
	}
	if first.SnapshotDigest != capture.ManifestDigest || first.Git.Digest == "" || first.Intent.Digest == "" {
		t.Fatalf("missing provenance digests: %#v", first)
	}
	if len(first.Files) != 1 || len(first.Files[0].Hunks) == 0 || first.Files[0].Patch == "" {
		t.Fatalf("file diff inventory = %#v", first.Files)
	}
	if len(first.Files[0].Symbols) != 2 || !hasSurface(first.Files[0].Surfaces, SurfacePublicAPI) {
		t.Fatalf("symbols/surfaces = %#v/%#v", first.Files[0].Symbols, first.Files[0].Surfaces)
	}
	if !hasPolicyDecision(first.Policies, "repository_write", "deny") {
		t.Fatalf("built-in safety did not win: %#v", first.Policies.Decisions)
	}
	if len(first.Policies.TargetEvidence) != 1 || first.Policies.TargetEvidence[0].Digest == "" {
		t.Fatalf("target policy evidence = %#v", first.Policies.TargetEvidence)
	}
	if countContextKind(first.Context, string(GuidanceTargetPolicy)) != 1 {
		t.Fatalf("target evidence was duplicated in context: %#v", first.Context)
	}

	content[targetDigest] = []byte("tampered\n")
	if _, err := Assemble(
		context.Background(),
		input,
	); err == nil ||
		!strings.Contains(err.Error(), "digest mismatch") {
		t.Fatalf("tampered content error = %v", err)
	}
}

func TestPolicyPrecedencePathScopeAndConflicts(t *testing.T) {
	t.Parallel()
	base := []snapshot.Entry{
		{
			Path:          "src/a.go",
			Kind:          snapshot.EntryKindFile,
			Mode:          0o100644,
			Size:          1,
			ContentDigest: digestBytes([]byte("a")),
			GitOID:        "a",
		},
	}
	target := []snapshot.Entry{
		{
			Path:          "src/a.go",
			Kind:          snapshot.EntryKindFile,
			Mode:          0o100644,
			Size:          1,
			ContentDigest: digestBytes([]byte("b")),
			GitOID:        "b",
		},
	}
	capture := makeCapture(t, base, target)
	model, err := Assemble(context.Background(), Input{
		Snapshot: capture,
		Content: func(_ context.Context, digest string) ([]byte, error) {
			if digest == base[0].ContentDigest {
				return []byte("a"), nil
			}
			return []byte("b"), nil
		},
		Request: ReviewRequest{Rules: []PolicyRule{{Key: "review_depth", Value: "private", Scope: "src/*"}}},
		Guidance: []Guidance{
			{
				ID:   "general",
				Path: "AGENTS.md",
				Kind: GuidancePolicy,
				Tier: PolicyTierBasePolicy,
				Rules: []PolicyRule{
					{Key: "review_depth", Value: "base"},
					{Key: "review_order", Value: "alphabetical", Scope: "src/*"},
				},
			},
			{
				ID:   "specific",
				Path: "AGENTS.md",
				Kind: GuidancePolicy,
				Tier: PolicyTierBasePolicy,
				Rules: []PolicyRule{
					{Key: "review_depth", Value: "specific", Scope: "src/*"},
					{Key: "review_order", Value: "risk", Scope: "src/*"},
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("Assemble() error = %v", err)
	}
	if selected := selectedValue(model.Policies, "review_depth"); selected != "private" {
		t.Fatalf("private tier selection = %q, want private", selected)
	}
	if selected := selectedValue(model.Policies, "review_order"); selected != "alphabetical" {
		t.Fatalf("same-tier safe selection = %q, want alphabetical", selected)
	}
	if len(model.Policies.Conflicts) != 1 || model.Policies.Conflicts[0].Key != "review_order" {
		t.Fatalf("conflicts = %#v", model.Policies.Conflicts)
	}

	noBase, err := Assemble(context.Background(), Input{
		Snapshot: capture,
		Guidance: []Guidance{
			{
				ID:    "target",
				Path:  "AGENTS.md",
				Kind:  GuidanceTargetPolicy,
				Tier:  PolicyTierTargetEvidence,
				Rules: []PolicyRule{{Key: "initial_scope", Value: "target"}},
			},
		},
		NoBaseRevision: true,
	})
	if err != nil {
		t.Fatalf("no-base Assemble() error = %v", err)
	}
	if !noBase.Policies.NoBaseRevisionException || selectedValue(noBase.Policies, "initial_scope") != "target" {
		t.Fatalf("no-base policy exception = %#v", noBase.Policies)
	}
}

func TestAssembleRejectsUntrustedOrMismatchedInputs(t *testing.T) {
	t.Parallel()
	capture := makeCapture(t, nil, nil)
	cases := []struct {
		name  string
		input Input
		want  string
	}{
		{
			name:  "pinned git mismatch",
			input: Input{Snapshot: capture, Git: PinnedGit{TargetOID: "other"}},
			want:  "pinned Git metadata does not match",
		},
		{
			name: "earlier round session mismatch",
			input: Input{
				SessionID:    "current",
				Snapshot:     capture,
				EarlierRound: &EarlierRound{SessionID: "other", RoundID: "round"},
			},
			want: "another session",
		},
		{
			name: "guidance digest mismatch",
			input: Input{
				Snapshot: capture,
				Guidance: []Guidance{
					{Path: "AGENTS.md", Kind: GuidancePolicy, Tier: PolicyTierBasePolicy, Content: "x", Digest: "bad"},
				},
			},
			want: "guidance",
		},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			_, err := Assemble(context.Background(), test.input)
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("error = %v, want %q", err, test.want)
			}
		})
	}
}

func makeCapture(t *testing.T, base, target []snapshot.Entry) snapshot.Capture {
	t.Helper()
	capture := snapshot.Capture{
		ComparisonKind:      snapshot.ComparisonTwoDot,
		RequestedComparison: "base..target",
		BaseOID:             "base-oid",
		EffectiveBaseOID:    "base-oid",
		ObjectFormat:        "sha1",
		ContextPolicyHash:   snapshot.DefaultContextPolicyHash(),
		CapturedAt:          time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC),
		Base:                snapshot.TreeState{OID: "base-oid", Entries: base},
		Target:              snapshot.TreeState{OID: "target-oid", Entries: target},
	}
	var err error
	capture.Base.ManifestDigest, err = snapshot.ManifestDigest(base)
	if err != nil {
		t.Fatal(err)
	}
	capture.Target.ManifestDigest, err = snapshot.ManifestDigest(target)
	if err != nil {
		t.Fatal(err)
	}
	capture.Changes = snapshot.BuildChanges(base, target)
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	return capture
}

func digestBytes(content []byte) string {
	digest := sha256.Sum256(content)
	return hex.EncodeToString(digest[:])
}

func hasSurface(surfaces []SurfaceKind, wanted SurfaceKind) bool {
	for _, surface := range surfaces {
		if surface == wanted {
			return true
		}
	}
	return false
}

func hasPolicyDecision(policies PolicyResolution, key, value string) bool {
	for _, decision := range policies.Decisions {
		if decision.Key == key && decision.Selected.Value == value {
			return true
		}
	}
	return false
}

func selectedValue(policies PolicyResolution, key string) string {
	for _, decision := range policies.Decisions {
		if decision.Key == key {
			return decision.Selected.Value
		}
	}
	return ""
}

func countContextKind(artifacts []ContextArtifact, kind string) int {
	count := 0
	for _, artifact := range artifacts {
		if artifact.Kind == kind {
			count++
		}
	}
	return count
}

func TestModelRunOptionsNormalizeCentralizesDefaultsAndCopiesInputs(t *testing.T) {
	t.Parallel()

	parameters := map[string]any{"temperature": 0.2}
	redactions := []string{"credential", "request", "credential"}
	model := modelRunOptionsMetadataModel{value: ModelMetadata{
		Adapter:    " provider-adapter ",
		Protocol:   " provider-protocol/v1 ",
		Model:      " provider-model ",
		Redactions: []string{"provider-secret", "credential"},
	}}
	normalized := (ModelRunOptions{
		Retry:   RetryPolicy{RepairAttempts: -1},
		Adapter: " ", Protocol: " ", PromptTemplateVersion: " ", Model: " ",
		Parameters: parameters, Redactions: redactions,
	}).normalize(model, "mire/v1/test-prompt")

	wantRetry := RetryPolicy{
		MaxAttempts:    DefaultRetryPolicy.MaxAttempts,
		Timeout:        DefaultRetryPolicy.Timeout,
		MaxOutputBytes: DefaultRetryPolicy.MaxOutputBytes,
	}
	if normalized.Retry != wantRetry {
		t.Fatalf("normalized retry = %#v, want %#v", normalized.Retry, wantRetry)
	}
	if normalized.Adapter != "provider-adapter" || normalized.Protocol != "provider-protocol/v1" ||
		normalized.Model != "provider-model" ||
		normalized.PromptTemplateVersion != "mire/v1/test-prompt" {
		t.Fatalf("normalized metadata = %#v", normalized)
	}
	if normalized.Now == nil || normalized.Now().IsZero() {
		t.Fatal("normalized clock is missing or returned zero time")
	}
	wantRedactions := []string{"credential", "request", "provider-secret"}
	if strings.Join(normalized.Redactions, ",") != strings.Join(wantRedactions, ",") {
		t.Fatalf("normalized redactions = %#v, want %#v", normalized.Redactions, wantRedactions)
	}

	normalized.Parameters["top_p"] = 0.9
	if _, ok := parameters["top_p"]; ok {
		t.Fatal("normalization reused the caller's parameter map")
	}
	if len(redactions) != 3 {
		t.Fatalf("normalization mutated caller redactions = %#v", redactions)
	}
}

func TestNewRunRecordCentralizesCommonProvenance(t *testing.T) {
	t.Parallel()

	parameters := map[string]any{"temperature": 0.2}
	redactions := []string{"token"}
	now := time.Date(2026, time.July, 15, 14, 0, 0, 0, time.UTC)
	options := ModelRunOptions{
		Retry:                 RetryPolicy{MaxAttempts: 3},
		Adapter:               "adapter",
		Protocol:              "protocol/v1",
		PromptTemplateVersion: "prompt/v1",
		Model:                 "model",
		Parameters:            parameters,
		Redactions:            redactions,
	}
	run := newRunRecord(
		"run-1", "session-1", "round-1", "snapshot-1", ModelRoleReviewer, "correctness",
		"manifest-digest", "input-digest", options, now,
	)
	if run.Status != RunStatusQueued || run.MaxAttempts != 3 || run.PassName != "correctness" ||
		run.CreatedAt != now || run.UpdatedAt != now {
		t.Fatalf("run identity = %#v", run)
	}
	if run.Provenance.Adapter != "adapter" || run.Provenance.Protocol != "protocol/v1" ||
		run.Provenance.PromptTemplateVersion != "prompt/v1" || run.Provenance.Model != "model" ||
		run.Provenance.InputManifestDigest != "manifest-digest" || run.Provenance.InputDigest != "input-digest" {
		t.Fatalf("run provenance = %#v", run.Provenance)
	}
	run.Provenance.Parameters["top_p"] = 0.9
	run.Provenance.Redactions[0] = "changed"
	if _, ok := parameters["top_p"]; ok || redactions[0] != "token" {
		t.Fatal("newRunRecord reused caller-owned provenance collections")
	}
}

type modelRunOptionsMetadataModel struct {
	value ModelMetadata
}

func (model modelRunOptionsMetadataModel) Complete(context.Context, ModelRequest) (ModelResponse, error) {
	return ModelResponse{}, nil
}

func (model modelRunOptionsMetadataModel) Metadata() ModelMetadata {
	return model.value
}
