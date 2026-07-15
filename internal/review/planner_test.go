package review

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestRunPlannerFixtureProducesExplainablePlan(t *testing.T) {
	t.Parallel()

	change := plannerFixtureChange()
	result, err := RunPlanner(context.Background(), change, NewFixtureModel(change), PlannerOptions{
		Retry:   RetryPolicy{MaxAttempts: 2, RepairAttempts: 1},
		Adapter: "fixture", Protocol: "fixture/v1", PromptTemplateVersion: "test-template",
		Model: "fixture-model", Now: func() time.Time { return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("RunPlanner() error = %v", err)
	}
	if result.Plan == nil || result.Run.Status != RunStatusComplete {
		t.Fatalf("result = %#v", result)
	}
	if len(result.Plan.Slices) != 1 || len(result.Plan.Slices[0].HunkIDs) != 1 {
		t.Fatalf("slices = %#v", result.Plan.Slices)
	}
	if result.Plan.Slices[0].HunkIDs[0] != "src/a.go#hunk" {
		t.Fatalf("slice hunk IDs = %#v", result.Plan.Slices[0].HunkIDs)
	}
	if len(result.Plan.Passes) != 12 || result.Plan.Passes[0].Order != 0 || result.Plan.Passes[11].Order != 11 {
		t.Fatalf("passes = %#v", result.Plan.Passes)
	}
	if result.Run.Provenance.Adapter != "fixture" || result.Run.Provenance.InputManifestDigest != "manifest-digest" {
		t.Fatalf("provenance = %#v", result.Run.Provenance)
	}
	if result.Plan.Digest == "" || result.Run.Provenance.InputDigest == "" {
		t.Fatalf("missing digests: plan=%q input=%q", result.Plan.Digest, result.Run.Provenance.InputDigest)
	}
}

func TestRunPlannerRepairsMalformedStructuredOutputWithinBound(t *testing.T) {
	t.Parallel()

	change := plannerFixtureChange()
	fixture := NewFixtureModel(change)
	fixture.Responses = []FixtureResponse{{Output: []byte(`{"schema_version":`)}}
	result, err := RunPlanner(context.Background(), change, fixture, PlannerOptions{
		Retry: RetryPolicy{MaxAttempts: 2, RepairAttempts: 1},
	})
	if err != nil {
		t.Fatalf("RunPlanner() error = %v", err)
	}
	if fixture.Calls != 2 || result.Run.Attempt != 2 || result.Plan == nil {
		t.Fatalf("calls=%d attempt=%d plan=%#v", fixture.Calls, result.Run.Attempt, result.Plan)
	}
}

func TestRunPlannerLeavesInvalidOutputAsVisibleFailure(t *testing.T) {
	t.Parallel()

	change := plannerFixtureChange()
	fixture := NewFixtureModel(change)
	fixture.Responses = []FixtureResponse{{Output: []byte("not-json")}, {Output: []byte("still-not-json")}}
	result, err := RunPlanner(context.Background(), change, fixture, PlannerOptions{
		Retry: RetryPolicy{MaxAttempts: 2, RepairAttempts: 1},
	})
	var plannerErr *PlannerError
	if !errors.As(err, &plannerErr) || plannerErr.Status != RunStatusFailed {
		t.Fatalf("error = %v, want failed PlannerError", err)
	}
	if result.Plan != nil || result.Run.Status != RunStatusFailed || result.Run.Provenance.TerminationCause != "invalid_structured_output" {
		t.Fatalf("result = %#v", result)
	}
	if !strings.Contains(result.Run.Error, "decode review plan") {
		t.Fatalf("run error = %q", result.Run.Error)
	}
}

func TestRunPlannerCancellationAndOutputBudgetAreDurableStatuses(t *testing.T) {
	t.Parallel()

	change := plannerFixtureChange()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	result, err := RunPlanner(ctx, change, NewFixtureModel(change), PlannerOptions{Retry: RetryPolicy{MaxAttempts: 2}})
	var plannerErr *PlannerError
	if !errors.As(err, &plannerErr) || plannerErr.Status != RunStatusCancelled || result.Run.Status != RunStatusCancelled {
		t.Fatalf("cancellation result=%#v error=%v", result, err)
	}

	fixture := NewFixtureModel(change)
	result, err = RunPlanner(context.Background(), change, fixture, PlannerOptions{
		Retry: RetryPolicy{MaxAttempts: 1, MaxOutputBytes: 1},
	})
	if !errors.As(err, &plannerErr) || plannerErr.Status != RunStatusBudgetExhausted || result.Run.Status != RunStatusBudgetExhausted {
		t.Fatalf("budget result=%#v error=%v", result, err)
	}

	fixture = NewFixtureModel(change)
	fixture.Delay = 50 * time.Millisecond
	result, err = RunPlanner(context.Background(), change, fixture, PlannerOptions{
		Retry: RetryPolicy{MaxAttempts: 1, Timeout: time.Millisecond},
	})
	if !errors.As(err, &plannerErr) || plannerErr.Status != RunStatusTimedOut || result.Run.Status != RunStatusTimedOut {
		t.Fatalf("timeout result=%#v error=%v", result, err)
	}
}

func plannerFixtureChange() ChangeModel {
	return ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: "session-1", SnapshotID: "snapshot-1",
		SnapshotDigest: "manifest-digest", Digest: "change-model-digest",
		Files:    []FileChange{{Status: "modified", TargetPath: "src/a.go", Hunks: []Hunk{{ID: "src/a.go#hunk", Available: true}}}},
		Surfaces: []AffectedSurface{{Kind: SurfaceContracts, Evidence: []SurfaceEvidence{{Kind: SurfaceContracts, Path: "src/a.go", HunkIDs: []string{"src/a.go#hunk"}}}}},
	}
}
