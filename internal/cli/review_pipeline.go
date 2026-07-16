package cli

import (
	"context"
	"fmt"
	"io"
	"sort"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/snapshot"
	"github.com/stormlightlabs/mire/internal/terminal"
)

type reviewExecution struct {
	Change           review.ChangeModel
	Coverage         review.ReviewCoverage
	Passes           []review.PassCoverage
	Diagnostics      []review.ReviewDiagnostic
	Findings         []review.FindingRevision
	IncompleteReason string
}

func executeReview(
	ctx context.Context,
	store *db.RepositoryStore,
	session db.Session,
	round db.Round,
	persisted db.Snapshot,
	capture snapshot.Capture,
	objectStore *snapshot.ObjectStore,
	configured review.Model,
) (reviewExecution, error) {
	change, err := assembleChangeModel(ctx, session.ID, persisted.ID, capture, objectStore)
	if err != nil {
		return reviewExecution{}, err
	}
	model := configured
	if model == nil {
		model = review.NewFixtureModel(change)
	}
	execution := reviewExecution{Change: change}
	planner, plannerErr := review.RunPlanner(ctx, change, model, review.PlannerOptions{
		ModelRunOptions: review.ModelRunOptions{Retry: review.DefaultRetryPolicy}, RoundID: round.ID, Store: store,
	})
	if plannerErr != nil || planner.Plan == nil {
		execution.IncompleteReason = "The review plan could not be completed."
		execution.Diagnostics = append(execution.Diagnostics, review.ReviewDiagnostic{
			ID: "planner-failure", PassName: "planner", Code: "planner_failure",
			Message: errorMessage(plannerErr, "The planner returned no usable plan."), CreatedAt: time.Now().UTC(),
		})
		return execution, nil
	}

	reviewer, reviewerErr := review.RunReviewPasses(ctx, change, model, review.ReviewerOpts{
		ModelRunOptions: review.ModelRunOptions{
			Retry: review.DefaultRetryPolicy,
		},
		RoundID:   round.ID,
		Passes:    planner.Plan.Passes,
		Retriever: snapshotRetriever{capture: capture, objectStore: objectStore},
		Store:     store,
	})
	execution.Coverage = reviewer.Coverage
	execution.Passes = append([]review.PassCoverage(nil), reviewer.Coverage.Passes...)
	execution.Diagnostics = append(execution.Diagnostics, reviewer.Diagnostics...)
	if reviewerErr != nil {
		execution.IncompleteReason = "One or more review passes did not complete."
	}

	if len(reviewer.Candidates) > 0 {
		verification, verificationErr := review.RunCandidateVerifications(
			ctx,
			change,
			reviewer.Candidates,
			model,
			review.VerifierOptions{
				ModelRunOptions: review.ModelRunOptions{Retry: review.DefaultRetryPolicy}, RoundID: round.ID,
				Retriever: snapshotRetriever{capture: capture, objectStore: objectStore}, Store: store,
			},
		)
		if verificationErr != nil && execution.IncompleteReason == "" {
			execution.IncompleteReason = "One or more candidate verifications did not complete."
		}
		for _, result := range verification.Results {
			finding, findingErr := review.NewFindingRevision(change, result.Candidate, result, round.ID)
			if findingErr != nil {
				execution.Diagnostics = append(execution.Diagnostics, review.ReviewDiagnostic{
					ID: result.Candidate.ID + ":finding", PassName: result.Candidate.PassName,
					Code: "finding_persistence", Message: findingErr.Error(), CreatedAt: time.Now().UTC(),
				})
				execution.IncompleteReason = "A retained candidate could not be recorded as a finding revision."
				continue
			}
			if err := store.SaveFindingRevision(ctx, finding); err != nil {
				return execution, fmt.Errorf("persist finding %q: %w", finding.FindingID, err)
			}
			execution.Findings = append(execution.Findings, finding)
		}
	}
	return execution, nil
}

func assembleChangeModel(
	ctx context.Context,
	sessionID, snapshotID string,
	capture snapshot.Capture,
	objectStore *snapshot.ObjectStore,
) (review.ChangeModel, error) {
	return review.Assemble(ctx, review.Input{
		SessionID: sessionID, SnapshotID: snapshotID, Snapshot: capture,
		Content: func(ctx context.Context, digest string) ([]byte, error) {
			if objectStore == nil {
				return nil, fmt.Errorf("private snapshot object store is unavailable")
			}
			file, err := objectStore.Open(digest)
			if err != nil {
				return nil, err
			}
			data, readErr := io.ReadAll(file)
			closeErr := file.Close()
			if readErr != nil {
				return nil, readErr
			}
			return data, closeErr
		},
	})
}

func buildTerminalReport(
	ctx context.Context,
	store *db.RepositoryStore,
	session db.Session,
	round db.Round,
	persisted db.Snapshot,
	capture snapshot.Capture,
	objectStore *snapshot.ObjectStore,
) (terminal.Report, error) {
	report := terminal.Report{
		SessionID: session.ID, RoundID: round.ID, SnapshotID: persisted.ID,
		SnapshotKind: persisted.Kind, RequestedComparison: persisted.RequestedComparison,
		Status: string(round.Status),
	}
	change, changeErr := assembleChangeModel(ctx, session.ID, persisted.ID, capture, objectStore)
	if changeErr != nil {
		report.IncompleteReason = "The immutable snapshot could not be assembled for display: " + changeErr.Error()
		return report, nil
	}
	report.Change = change
	coverage, coverageErr := store.GetReviewCoverage(ctx, round.ID)
	if coverageErr == nil {
		report.Coverage = coverage
	} else {
		report.IncompleteReason = "No persisted review coverage is available."
	}
	if operations, operationsErr := store.ListOperations(ctx, session.ID); operationsErr == nil {
		for index := len(operations) - 1; index >= 0; index-- {
			operation := operations[index]
			if operation.RoundID != round.ID || operation.Status != db.OperationStatusFailed {
				continue
			}
			if strings.TrimSpace(operation.Failure) != "" {
				report.IncompleteReason = operation.Failure
			}
			break
		}
	}
	report.Passes, _ = store.ListReviewPasses(ctx, round.ID)
	report.Diagnostics, _ = store.ListReviewDiagnostics(ctx, round.ID)
	candidates, candidateErr := store.ListReviewCandidates(ctx, round.ID)
	if candidateErr != nil {
		return report, candidateErr
	}
	findings, findingErr := store.ListFindingRevisions(ctx, round.ID)
	if findingErr != nil {
		return report, findingErr
	}
	findingByCandidate := make(map[string]review.FindingRevision, len(findings))
	for _, finding := range findings {
		if finding.Origin.CandidateID != "" {
			findingByCandidate[finding.Origin.CandidateID] = finding
		}
	}
	for _, candidate := range candidates {
		verification, verificationErr := store.GetLatestVerification(ctx, candidate.ID)
		if verificationErr != nil {
			view := terminal.CandidateView{Candidate: candidate, Reason: "not verified"}
			report.Candidates = append(report.Candidates, view)
			continue
		}
		run, runErr := store.GetVerificationRun(ctx, verification.RunID)
		if runErr != nil {
			view := terminal.CandidateView{Candidate: candidate, Reason: "verification provenance unavailable"}
			report.Candidates = append(report.Candidates, view)
			continue
		}
		lane, laneErr := review.DeriveLane(change, candidate, verification, run)
		finding, hasFinding := findingByCandidate[candidate.ID]
		if laneErr != nil {
			lane = review.FindingLaneCandidate
		}
		if hasFinding && lane == review.FindingLaneVerified {
			report.Findings = append(
				report.Findings,
				terminal.FindingView{Revision: finding, Lane: lane, Candidate: &candidate, Verification: &verification},
			)
		} else if lane == review.FindingLaneRefuted {
			report.Refuted = append(
				report.Refuted,
				terminal.CandidateView{Candidate: candidate, Reason: string(verification.State)},
			)
		} else {
			report.Candidates = append(
				report.Candidates,
				terminal.CandidateView{Candidate: candidate, Reason: string(verification.State)},
			)
		}
	}
	for _, finding := range findings {
		if finding.Origin.CandidateID != "" {
			continue
		}
		// Older or externally-created ledger revisions remain visible, but the
		// renderer conservatively keeps them out of the verified lane unless the
		// immutable revision itself records supporting verification evidence.
		if finding.Verification == review.VerificationSupported && len(finding.Evidence) > 0 {
			report.Findings = append(
				report.Findings,
				terminal.FindingView{Revision: finding, Lane: review.FindingLaneVerified},
			)
		} else if finding.Verification == review.VerificationRefuted {
			report.Refuted = append(
				report.Refuted,
				terminal.CandidateView{
					Reason: string(finding.Verification),
					Candidate: review.CandidateRecord{
						ID: finding.FindingID,
						Candidate: review.Candidate{
							CandidateContent: review.CandidateContent{
								Claim:    finding.Claim,
								Impact:   finding.Impact,
								Category: finding.Category,
								Severity: finding.Severity,
							},
						},
					},
				},
			)
		} else {
			report.Candidates = append(
				report.Candidates,
				terminal.CandidateView{
					Reason: string(finding.Verification),
					Candidate: review.CandidateRecord{
						ID: finding.FindingID,
						Candidate: review.Candidate{
							CandidateContent: review.CandidateContent{
								Claim:    finding.Claim,
								Impact:   finding.Impact,
								Category: finding.Category,
								Severity: finding.Severity,
							},
						},
					},
				},
			)
		}
	}
	if round.Status == db.RoundStatusIncomplete && report.IncompleteReason == "" {
		report.IncompleteReason = "The review operation completed with incomplete analysis."
	}
	return report, nil
}

func captureFromStore(ctx context.Context, store *db.RepositoryStore, persisted db.Snapshot) (snapshot.Capture, error) {
	readEntries := func(side string) ([]snapshot.Entry, error) {
		entries, err := store.ListSnapshotEntries(ctx, persisted.ID, side)
		if err != nil {
			return nil, err
		}
		result := make([]snapshot.Entry, 0, len(entries))
		for _, entry := range entries {
			result = append(
				result,
				snapshot.Entry{
					Path:          entry.Path,
					Kind:          entry.Kind,
					Mode:          entry.Mode,
					Size:          entry.Size,
					ContentDigest: entry.ContentDigest,
					GitOID:        entry.GitOID,
					SymlinkTarget: entry.SymlinkTarget,
				},
			)
		}
		return result, nil
	}
	readChanges := func() ([]snapshot.Change, error) {
		changes, err := store.ListSnapshotChanges(ctx, persisted.ID)
		if err != nil {
			return nil, err
		}
		result := make([]snapshot.Change, 0, len(changes))
		for _, change := range changes {
			result = append(
				result,
				snapshot.Change{
					Status:       change.Status,
					BasePath:     change.BasePath,
					TargetPath:   change.TargetPath,
					BaseDigest:   change.BaseDigest,
					TargetDigest: change.TargetDigest,
				},
			)
		}
		return result, nil
	}
	baseSide := snapshot.TreeSideBase
	targetSide := snapshot.TreeSideTarget
	if persisted.Kind == snapshot.ComparisonWorktree {
		baseSide = snapshot.TreeSideHead
		targetSide = snapshot.TreeSideWorktree
	}
	base, err := readEntries(baseSide)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read snapshot base entries: %w", err)
	}
	target, err := readEntries(targetSide)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read snapshot target entries: %w", err)
	}
	changes, err := readChanges()
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read snapshot changes: %w", err)
	}
	capture := snapshot.Capture{
		ComparisonKind:      persisted.Kind,
		RequestedComparison: persisted.RequestedComparison,
		BaseOID:             persisted.BaseOID,
		EffectiveBaseOID:    persisted.EffectiveBaseOID,
		MergeBaseOID:        persisted.MergeBaseOID,
		ObjectFormat:        persisted.ObjectFormat,
		ContextPolicyHash:   persisted.ContextPolicyHash,
		IgnorePolicy:        persisted.IgnorePolicy,
		CapturedAt:          persisted.CreatedAt,
		Base: snapshot.TreeState{
			OID: persisted.EffectiveBaseOID, Entries: base, ManifestDigest: persisted.BaseManifestDigest,
		},
		Target: snapshot.TreeState{
			OID: persisted.TargetOID, Entries: target, ManifestDigest: persisted.TargetManifestDigest,
		},
		Changes:        changes,
		ManifestDigest: persisted.ManifestDigest,
	}
	if persisted.Kind == snapshot.ComparisonWorktree {
		capture.Base.OID = persisted.EffectiveBaseOID
		capture.Head.OID = persisted.BaseOID
		capture.Head.Entries = append([]snapshot.Entry(nil), base...)
		capture.Worktree.Entries = append([]snapshot.Entry(nil), target...)
		capture.Worktree.OID = persisted.TargetOID
		capture.Index.OID = persisted.IndexOID
		capture.Index.Entries, err = readEntries(snapshot.TreeSideIndex)
		if err != nil {
			return snapshot.Capture{}, fmt.Errorf("read snapshot index entries: %w", err)
		}
		capture.Head.ManifestDigest = persisted.BaseManifestDigest
		capture.Worktree.ManifestDigest = persisted.TargetManifestDigest
		for _, layer := range persisted.Layers {
			if layer.Layer == snapshot.TreeSideIndex {
				capture.Index.ManifestDigest = layer.ManifestDigest
			}
		}
	}
	if err := capture.Validate(); err != nil {
		return snapshot.Capture{}, fmt.Errorf("validate stored snapshot: %w", err)
	}
	return capture, nil
}

type snapshotRetriever struct {
	capture     snapshot.Capture
	objectStore *snapshot.ObjectStore
}

func (retriever snapshotRetriever) Retrieve(
	ctx context.Context,
	request review.RetrievalRequest,
) ([]review.RetrievedArtifact, error) {
	pathName := strings.TrimSpace(request.Path)
	if pathName == "" || retriever.objectStore == nil {
		return nil, nil
	}
	entries := append([]snapshot.Entry(nil), retriever.capture.Target.Entries...)
	for _, entry := range entries {
		if entry.Path != pathName || entry.ContentDigest == "" {
			continue
		}
		file, err := retriever.objectStore.Open(entry.ContentDigest)
		if err != nil {
			return nil, err
		}
		data, readErr := io.ReadAll(file)
		closeErr := file.Close()
		if readErr != nil {
			return nil, readErr
		}
		if closeErr != nil {
			return nil, closeErr
		}
		return []review.RetrievedArtifact{
			{
				Kind:     request.Kind,
				Path:     pathName,
				Relation: request.Relation,
				HunkIDs:  append([]string(nil), request.HunkIDs...),
				Digest:   entry.ContentDigest,
				Size:     int64(len(data)),
				Content:  string(data),
			},
		}, nil
	}
	return nil, nil
}

func sortReportViews(report *terminal.Report) {
	sort.SliceStable(report.Findings, func(i, j int) bool {
		return report.Findings[i].Revision.FindingID < report.Findings[j].Revision.FindingID
	})
	sort.SliceStable(
		report.Candidates,
		func(i, j int) bool { return report.Candidates[i].Candidate.ID < report.Candidates[j].Candidate.ID },
	)
	sort.SliceStable(
		report.Refuted,
		func(i, j int) bool { return report.Refuted[i].Candidate.ID < report.Refuted[j].Candidate.ID },
	)
}

func errorMessage(err error, fallback string) string {
	if err == nil {
		return fallback
	}
	return err.Error()
}
