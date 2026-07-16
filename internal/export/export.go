// Package export projects the durable review ledger into the explicitly
// requested V1 handoff formats. Export is a one-way, inspectable projection;
// it never includes the private snapshot object store and cannot restore a
// session.
package export

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

// SchemaVersion is independent of the SQLite migration version.
const SchemaVersion = "mire/v1/review-export"

// ManifestSchemaVersion identifies the bundle manifest projection.
const ManifestSchemaVersion = "mire/v1/export-manifest"

// MIREVersion is the application version label carried by V1 exports.
const MIREVersion = "v1"

// Format is one of the explicit V1 export views.
type Format string

const (
	// FormatMarkdown emits the human-readable REVIEW.md projection.
	FormatMarkdown Format = "markdown"
	// FormatJSON emits the canonical review.json projection.
	FormatJSON Format = "json"
	// FormatSARIF emits the findings-only SARIF 2.1.0 projection.
	FormatSARIF Format = "sarif"
	// FormatBundle emits the inspectable multi-file handoff directory.
	FormatBundle Format = "bundle"
)

// Valid reports whether format is supported by V1.
func (format Format) Valid() bool {
	switch format {
	case FormatMarkdown, FormatJSON, FormatSARIF, FormatBundle:
		return true
	default:
		return false
	}
}

// Omission records information intentionally absent from a projection.
type Omission struct {
	Kind   string `json:"kind"`
	Reason string `json:"reason"`
}

// SnapshotEntryDescriptor is a manifest entry without snapshot bytes.
type SnapshotEntryDescriptor struct {
	TreeSide      string `json:"tree_side"`
	Path          string `json:"path"`
	Kind          string `json:"kind"`
	Mode          uint32 `json:"mode"`
	Size          int64  `json:"size"`
	ContentDigest string `json:"content_digest,omitempty"`
	GitOID        string `json:"git_oid,omitempty"`
	SymlinkTarget string `json:"symlink_target,omitempty"`
}

// SnapshotChangeDescriptor describes a path comparison without file content.
type SnapshotChangeDescriptor struct {
	Status       string `json:"status"`
	BasePath     string `json:"base_path,omitempty"`
	TargetPath   string `json:"target_path,omitempty"`
	BaseDigest   string `json:"base_digest,omitempty"`
	TargetDigest string `json:"target_digest,omitempty"`
}

// RepositoryDescriptor is the export boundary for persisted repository
// identity. It intentionally contains no live repository handle.
type RepositoryDescriptor struct {
	ID                string    `json:"id"`
	CanonicalIdentity string    `json:"canonical_identity"`
	DisplayName       string    `json:"display_name"`
	DiscoveredGitDir  string    `json:"discovered_git_dir"`
	CreatedAt         time.Time `json:"created_at"`
}

// SessionDescriptor is the export boundary for one review session.
type SessionDescriptor struct {
	ID                 string    `json:"id"`
	RepositoryID       string    `json:"repository_id"`
	RepositoryName     string    `json:"repository_name"`
	RepositoryIdentity string    `json:"repository_identity"`
	Title              string    `json:"title"`
	CreatedAt          time.Time `json:"created_at"`
	CurrentRoundID     string    `json:"current_round_id,omitempty"`
}

// RoundDescriptor is the export boundary for one review round.
type RoundDescriptor struct {
	ID                 string    `json:"id"`
	SessionID          string    `json:"session_id"`
	RepositoryID       string    `json:"repository_id"`
	SnapshotID         string    `json:"snapshot_id,omitempty"`
	PredecessorRoundID string    `json:"predecessor_round_id,omitempty"`
	Number             int       `json:"number"`
	Status             string    `json:"status"`
	CreatedAt          time.Time `json:"created_at"`
	UpdatedAt          time.Time `json:"updated_at"`
}

// LayerDescriptor identifies one immutable snapshot layer.
type LayerDescriptor struct {
	Layer          string `json:"layer"`
	Identity       string `json:"identity"`
	ManifestDigest string `json:"manifest_digest"`
}

// SnapshotManifest is the portable, content-addressed snapshot manifest.
// Entries are descriptors only; object-store bytes are deliberately absent.
type SnapshotManifest struct {
	ID                   string                     `json:"id"`
	RepositoryID         string                     `json:"repository_id"`
	Kind                 string                     `json:"kind"`
	RequestedComparison  string                     `json:"requested_comparison"`
	BaseOID              string                     `json:"base_oid,omitempty"`
	EffectiveBaseOID     string                     `json:"effective_base_oid,omitempty"`
	TargetOID            string                     `json:"target_oid,omitempty"`
	MergeBaseOID         string                     `json:"merge_base_oid,omitempty"`
	IndexOID             string                     `json:"index_oid,omitempty"`
	ObjectFormat         string                     `json:"object_format"`
	ContextPolicyHash    string                     `json:"context_policy_hash"`
	IgnorePolicy         string                     `json:"ignore_policy,omitempty"`
	BaseManifestDigest   string                     `json:"base_manifest_digest"`
	TargetManifestDigest string                     `json:"target_manifest_digest"`
	ManifestDigest       string                     `json:"manifest_digest"`
	Complete             bool                       `json:"complete"`
	CreatedAt            time.Time                  `json:"created_at"`
	Layers               []LayerDescriptor          `json:"layers"`
	Entries              []SnapshotEntryDescriptor  `json:"entries"`
	Changes              []SnapshotChangeDescriptor `json:"changes"`
}

// HunkDescriptor retains anchor identity while omitting line content.
type HunkDescriptor struct {
	ID        string `json:"id"`
	Kind      string `json:"kind"`
	OldStart  int    `json:"old_start"`
	OldLines  int    `json:"old_lines"`
	NewStart  int    `json:"new_start"`
	NewLines  int    `json:"new_lines"`
	Binary    bool   `json:"binary,omitempty"`
	Available bool   `json:"available"`
	Digest    string `json:"digest"`
}

// ChangeFileDescriptor is the normalized changed-file inventory.
type ChangeFileDescriptor struct {
	Status       string               `json:"status"`
	BasePath     string               `json:"base_path,omitempty"`
	TargetPath   string               `json:"target_path,omitempty"`
	BaseDigest   string               `json:"base_digest,omitempty"`
	TargetDigest string               `json:"target_digest,omitempty"`
	Hunks        []HunkDescriptor     `json:"hunks"`
	Symbols      []review.Symbol      `json:"symbols,omitempty"`
	Surfaces     []review.SurfaceKind `json:"surfaces,omitempty"`
}

// ChangeDescriptor is the export-safe portion of the assembled change model.
type ChangeDescriptor struct {
	SchemaVersion  string                   `json:"schema_version"`
	SnapshotDigest string                   `json:"snapshot_digest"`
	ComparisonKind string                   `json:"comparison_kind"`
	Requested      string                   `json:"requested_comparison"`
	Digest         string                   `json:"digest"`
	Files          []ChangeFileDescriptor   `json:"files"`
	Surfaces       []review.AffectedSurface `json:"surfaces"`
}

// ArtifactDescriptor describes retrieved context without embedding its bytes.
// The alias keeps the export boundary restricted to review's safe metadata
// projection without duplicating its JSON contract.
type ArtifactDescriptor = review.RetrievedArtifactMetadata

// FindingProjection is the derived lane plus immutable finding history.
type FindingProjection struct {
	Finding       review.FindingRevision      `json:"finding"`
	Lane          review.FindingLane          `json:"lane"`
	CandidateID   string                      `json:"candidate_id,omitempty"`
	Verification  *review.VerificationRecord  `json:"verification,omitempty"`
	Dispositions  []review.DispositionRecord  `json:"dispositions"`
	Presentations []review.PresentationRecord `json:"presentations"`
}

// CandidateProjection retains every emitted candidate, including those that
// did not become a finding revision.
type CandidateProjection struct {
	Candidate    review.CandidateRecord     `json:"candidate"`
	Lane         review.FindingLane         `json:"lane"`
	Reason       string                     `json:"reason,omitempty"`
	Verification *review.VerificationRecord `json:"verification,omitempty"`
}

// Ledger contains the normalized durable review records.
type Ledger struct {
	Passes        []review.PassCoverage       `json:"passes"`
	Diagnostics   []review.ReviewDiagnostic   `json:"diagnostics"`
	Candidates    []CandidateProjection       `json:"candidates"`
	Findings      []FindingProjection         `json:"findings"`
	Verifications []review.VerificationRecord `json:"verifications"`
	Dispositions  []review.DispositionRecord  `json:"dispositions"`
	Presentations []review.PresentationRecord `json:"presentations"`
}

// OperationDescriptor is the export boundary for durable operation state.
type OperationDescriptor struct {
	ID           string    `json:"id"`
	SessionID    string    `json:"session_id"`
	RepositoryID string    `json:"repository_id"`
	RoundID      string    `json:"round_id,omitempty"`
	Kind         string    `json:"kind"`
	Status       string    `json:"status"`
	Failure      string    `json:"failure,omitempty"`
	CreatedAt    time.Time `json:"created_at"`
	UpdatedAt    time.Time `json:"updated_at"`
	StartedAt    time.Time `json:"started_at,omitempty"`
	FinishedAt   time.Time `json:"finished_at,omitempty"`
}

// ActivityDescriptor is the export boundary for append-only activity.
type ActivityDescriptor struct {
	ID           int64     `json:"id"`
	SessionID    string    `json:"session_id"`
	RepositoryID string    `json:"repository_id"`
	RoundID      string    `json:"round_id,omitempty"`
	OperationID  string    `json:"operation_id,omitempty"`
	Kind         string    `json:"kind"`
	Status       string    `json:"status"`
	Message      string    `json:"message"`
	CreatedAt    time.Time `json:"created_at"`
}

// ChatRunProjection keeps chat provenance while excluding retrieved snapshot
// content from review.json.
type ChatRunProjection struct {
	Run            review.RunRecord     `json:"run"`
	UserMessageID  string               `json:"user_message_id"`
	Binding        review.ChatBinding   `json:"binding"`
	InputDigest    string               `json:"input_digest"`
	ArtifactIDs    []string             `json:"artifact_ids"`
	Response       *review.ChatResponse `json:"response,omitempty"`
	RetainedOutput string               `json:"retained_output,omitempty"`
}

// Provenance captures the model, operation, and activity history needed to
// audit a review without retaining credentials or snapshot object contents.
type Provenance struct {
	PlannerRuns      []review.RunRecord             `json:"planner_runs"`
	ReviewRuns       []review.RunRecord             `json:"review_runs"`
	VerificationRuns []review.VerificationRunRecord `json:"verification_runs"`
	ChatRuns         []ChatRunProjection            `json:"chat_runs"`
	Operations       []OperationDescriptor          `json:"operations"`
	Activity         []ActivityDescriptor           `json:"activity"`
}

// Review is the canonical versioned portable projection.
type Review struct {
	SchemaVersion    string                `json:"schema_version"`
	ExportKind       string                `json:"export_kind"`
	MIREVersion      string                `json:"mire_version"`
	Repository       RepositoryDescriptor  `json:"repository"`
	Session          SessionDescriptor     `json:"session"`
	Round            RoundDescriptor       `json:"round"`
	SnapshotManifest SnapshotManifest      `json:"snapshot_manifest"`
	Change           ChangeDescriptor      `json:"change"`
	Ledger           Ledger                `json:"ledger"`
	Artifacts        []ArtifactDescriptor  `json:"artifact_descriptors"`
	Provenance       Provenance            `json:"provenance"`
	Coverage         review.ReviewCoverage `json:"coverage"`
	Chat             []review.ChatMessage  `json:"chat"`
	Omissions        []Omission            `json:"omissions"`
	DiffPatch        string                `json:"-"`
	ChangeModel      review.ChangeModel    `json:"-"`
	artifactContents []EvidenceArtifact    `json:"-"`
}

// Document is an alias retained for callers that prefer the schema term.
type Document = Review

// EvidenceArtifact is a named, explicitly included bundle excerpt. Its
// content is never part of canonical review.json.
type EvidenceArtifact struct {
	Path    string
	Digest  string
	Content string
}

// Build loads one session round and assembles its canonical export projection.
// objectStore is optional: without it the manifest and ledger still export,
// while the diff is recorded as an omission.
func Build(
	ctx context.Context,
	store *db.RepositoryStore,
	session db.Session,
	round db.Round,
	objectStore *snapshot.ObjectStore,
) (Review, error) {
	if store == nil {
		return Review{}, errors.New("build export: store is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if session.ID == "" {
		return Review{}, errors.New("build export: session ID is required")
	}
	if round.ID == "" || round.SessionID != session.ID || round.RepositoryID != session.RepositoryID ||
		round.SnapshotID == "" {
		return Review{}, errors.New("build export: round provenance is incomplete")
	}
	persisted, err := store.GetSnapshot(ctx, round.SnapshotID)
	if err != nil {
		return Review{}, fmt.Errorf("build export: load snapshot: %w", err)
	}
	repository, repositoryErr := store.GetRepositoryForSession(ctx, session.ID)
	if repositoryErr != nil {
		return Review{}, fmt.Errorf("build export: load repository: %w", repositoryErr)
	}
	result := Review{
		SchemaVersion: SchemaVersion,
		ExportKind:    "portable_review_projection",
		MIREVersion:   MIREVersion,
		Repository:    repositoryDescriptor(repository),
		Session:       sessionDescriptor(session),
		Round:         roundDescriptor(round),
		Coverage:      emptyCoverage(),
		Chat:          []review.ChatMessage{},
		Omissions:     []Omission{},
	}
	result.SnapshotManifest = snapshotManifest(persisted)
	entries, changes, entryErr := loadManifest(ctx, store, persisted)
	if entryErr != nil {
		return Review{}, entryErr
	}
	result.SnapshotManifest.Entries = entries
	result.SnapshotManifest.Changes = changes

	var changeModel review.ChangeModel
	if objectStore == nil {
		result.Omissions = append(
			result.Omissions,
			Omission{Kind: "diff", Reason: "private snapshot object store was not supplied"},
		)
	} else {
		capture, captureErr := captureFromStore(ctx, store, persisted)
		if captureErr != nil {
			result.Omissions = append(result.Omissions, Omission{Kind: "diff", Reason: captureErr.Error()})
		} else {
			changeModel, err = review.Assemble(
				ctx,
				review.Input{
					SessionID:  session.ID,
					SnapshotID: persisted.ID,
					Snapshot:   capture,
					Content:    objectContent(objectStore),
				},
			)
			if err != nil {
				result.Omissions = append(result.Omissions, Omission{Kind: "diff", Reason: err.Error()})
			} else {
				result.ChangeModel = changeModel
				result.Change = changeDescriptor(changeModel)
				result.DiffPatch = diffPatch(changeModel)
			}
		}
	}
	result.Change = result.Change.withSnapshot(persisted)

	result.Ledger.Passes, _ = store.ListReviewPasses(ctx, round.ID)
	result.Ledger.Diagnostics, _ = store.ListReviewDiagnostics(ctx, round.ID)
	candidates, candidateErr := store.ListReviewCandidates(ctx, round.ID)
	if candidateErr != nil {
		return Review{}, fmt.Errorf("build export: list candidates: %w", candidateErr)
	}
	findings, findingErr := store.ListFindingRevisions(ctx, round.ID)
	if findingErr != nil {
		return Review{}, fmt.Errorf("build export: list findings: %w", findingErr)
	}
	verifications, verificationErr := store.ListVerifications(ctx, round.ID)
	if verificationErr != nil {
		return Review{}, fmt.Errorf("build export: list verifications: %w", verificationErr)
	}
	result.Ledger.Verifications = append([]review.VerificationRecord(nil), verifications...)
	result.Ledger.Candidates = candidateProjections(changeModel, candidates, store, ctx)
	result.Ledger.Findings = findingProjections(changeModel, findings, candidates, verifications, store, ctx)
	for _, finding := range result.Ledger.Findings {
		if finding.Lane == review.FindingLaneRefuted || findingHasSARIFLocation(finding.Finding) {
			continue
		}
		result.Omissions = append(
			result.Omissions,
			Omission{Kind: "sarif", Reason: finding.Finding.FindingID + " has no representable path location."},
		)
	}
	for _, finding := range findings {
		if dispositions, listErr := store.ListDispositions(ctx, finding.FindingID); listErr == nil {
			result.Ledger.Dispositions = append(result.Ledger.Dispositions, dispositions...)
		}
		if presentations, listErr := store.ListPresentations(ctx, finding.FindingID); listErr == nil {
			result.Ledger.Presentations = append(result.Ledger.Presentations, presentations...)
		}
	}
	sort.SliceStable(result.Ledger.Dispositions, func(i, j int) bool {
		left, right := result.Ledger.Dispositions[i], result.Ledger.Dispositions[j]
		if left.FindingID != right.FindingID {
			return left.FindingID < right.FindingID
		}
		if left.Revision != right.Revision {
			return left.Revision < right.Revision
		}
		if !left.CreatedAt.Equal(right.CreatedAt) {
			return left.CreatedAt.Before(right.CreatedAt)
		}
		return left.ID < right.ID
	})
	sort.SliceStable(result.Ledger.Presentations, func(i, j int) bool {
		left, right := result.Ledger.Presentations[i], result.Ledger.Presentations[j]
		if left.FindingID != right.FindingID {
			return left.FindingID < right.FindingID
		}
		if left.Version != right.Version {
			return left.Version < right.Version
		}
		return left.ID < right.ID
	})

	artifacts, artifactErr := store.ListReviewArtifacts(ctx, round.ID)
	if artifactErr == nil {
		for _, artifact := range artifacts {
			result.Artifacts = append(result.Artifacts, artifactDescriptor(artifact))
			if strings.TrimSpace(artifact.Content()) != "" && !artifact.Excluded {
				result.artifactContents = append(
					result.artifactContents,
					EvidenceArtifact{
						Path:    evidencePath(artifact.ID),
						Digest:  artifact.Digest,
						Content: artifact.Content(),
					},
				)
			}
		}
	} else {
		result.Omissions = append(result.Omissions, Omission{Kind: "artifacts", Reason: artifactErr.Error()})
	}
	result.Provenance.PlannerRuns, _ = store.ListPlanRuns(ctx, round.ID)
	result.Provenance.ReviewRuns, _ = store.ListReviewRuns(ctx, round.ID)
	result.Provenance.VerificationRuns, _ = store.ListVerificationRuns(ctx, round.ID)
	operations, _ := store.ListOperations(ctx, session.ID)
	result.Provenance.Operations = operationDescriptors(operations)
	for _, operation := range operations {
		if operation.RoundID != round.ID {
			continue
		}
		if operation.Status == db.OperationStatusFailed || operation.Status == db.OperationStatusAbandoned ||
			operation.Status == db.OperationStatusCancelled {
			reason := operation.Failure
			if reason == "" {
				reason = "Operation ended with status " + string(operation.Status) + "."
			}
			result.Omissions = append(result.Omissions, Omission{Kind: "operation", Reason: reason})
		}
	}
	if round.Status != db.RoundStatusComplete {
		result.Omissions = append(
			result.Omissions,
			Omission{Kind: "round", Reason: "Round status is " + string(round.Status) + "."},
		)
	}
	activities, _ := store.ListActivity(ctx, session.ID, 0)
	result.Provenance.Activity = activityDescriptors(activities)
	timeline, timelineErr := store.GetChatTimeline(ctx, session.ID)
	if timelineErr == nil {
		result.Chat = append(result.Chat, timeline.Messages...)
		result.Provenance.ChatRuns = projectChatRuns(timeline.Runs)
	} else {
		result.Omissions = append(result.Omissions, Omission{Kind: "chat", Reason: timelineErr.Error()})
	}
	if coverage, coverageErr := store.GetReviewCoverage(ctx, round.ID); coverageErr == nil {
		result.Coverage = coverage
	} else {
		result.Omissions = append(result.Omissions, Omission{Kind: "coverage", Reason: coverageErr.Error()})
	}
	seenArtifacts := make(map[string]bool, len(result.Artifacts))
	for _, artifact := range result.Artifacts {
		seenArtifacts[artifact.ID] = true
	}
	for _, artifact := range result.Coverage.RetrievedArtifacts {
		if artifact.ID == "" || seenArtifacts[artifact.ID] {
			continue
		}
		result.Artifacts = append(result.Artifacts, artifactDescriptor(artifact))
		seenArtifacts[artifact.ID] = true
	}
	result.Omissions = uniqueOmissions(result.Omissions)
	normalizeReview(&result)
	return result, nil
}

func emptyCoverage() review.ReviewCoverage {
	return review.ReviewCoverage{
		ExaminedFiles: []string{},
		ExaminedHunks: []string{},
		Passes:        []review.PassCoverage{},
		Analyzers:     []review.AnalyzerAvailability{},
		Exclusions:    []review.CoverageExclusion{},
		Failures:      []review.CoverageFailure{},
		Gaps:          []string{},
	}
}

func snapshotManifest(value db.Snapshot) SnapshotManifest {
	layers := make([]LayerDescriptor, 0, len(value.Layers))
	for _, layer := range value.Layers {
		layers = append(
			layers,
			LayerDescriptor{Layer: layer.Layer, Identity: layer.Identity, ManifestDigest: layer.ManifestDigest},
		)
	}
	return SnapshotManifest{
		ID:                   value.ID,
		RepositoryID:         value.RepositoryID,
		Kind:                 value.Kind,
		RequestedComparison:  value.RequestedComparison,
		BaseOID:              value.BaseOID,
		EffectiveBaseOID:     value.EffectiveBaseOID,
		TargetOID:            value.TargetOID,
		MergeBaseOID:         value.MergeBaseOID,
		IndexOID:             value.IndexOID,
		ObjectFormat:         value.ObjectFormat,
		ContextPolicyHash:    value.ContextPolicyHash,
		IgnorePolicy:         value.IgnorePolicy,
		BaseManifestDigest:   value.BaseManifestDigest,
		TargetManifestDigest: value.TargetManifestDigest,
		ManifestDigest:       value.ManifestDigest,
		Complete:             value.Complete,
		CreatedAt:            value.CreatedAt,
		Layers:               layers,
		Entries:              []SnapshotEntryDescriptor{},
		Changes:              []SnapshotChangeDescriptor{},
	}
}

func repositoryDescriptor(value db.Repository) RepositoryDescriptor {
	return RepositoryDescriptor{
		ID:                value.ID,
		CanonicalIdentity: value.CanonicalIdentity,
		DisplayName:       value.DisplayName,
		DiscoveredGitDir:  value.DiscoveredGitDir,
		CreatedAt:         value.CreatedAt,
	}
}

func sessionDescriptor(value db.Session) SessionDescriptor {
	return SessionDescriptor{
		ID:                 value.ID,
		RepositoryID:       value.RepositoryID,
		RepositoryName:     value.RepositoryName,
		RepositoryIdentity: value.RepositoryIdentity,
		Title:              value.Title,
		CreatedAt:          value.CreatedAt,
		CurrentRoundID:     value.CurrentRoundID,
	}
}

func roundDescriptor(value db.Round) RoundDescriptor {
	return RoundDescriptor{
		ID:                 value.ID,
		SessionID:          value.SessionID,
		RepositoryID:       value.RepositoryID,
		SnapshotID:         value.SnapshotID,
		PredecessorRoundID: value.PredecessorRoundID,
		Number:             value.Number,
		Status:             string(value.Status),
		CreatedAt:          value.CreatedAt,
		UpdatedAt:          value.UpdatedAt,
	}
}

func operationDescriptors(values []db.Operation) []OperationDescriptor {
	result := make([]OperationDescriptor, 0, len(values))
	for _, value := range values {
		result = append(result, OperationDescriptor{
			ID:           value.ID,
			SessionID:    value.SessionID,
			RepositoryID: value.RepositoryID,
			RoundID:      value.RoundID,
			Kind:         string(value.Kind),
			Status:       string(value.Status),
			Failure:      value.Failure,
			CreatedAt:    value.CreatedAt,
			UpdatedAt:    value.UpdatedAt,
			StartedAt:    value.StartedAt,
			FinishedAt:   value.FinishedAt,
		})
	}
	return result
}

func activityDescriptors(values []db.Activity) []ActivityDescriptor {
	result := make([]ActivityDescriptor, 0, len(values))
	for _, value := range values {
		result = append(
			result,
			ActivityDescriptor{
				ID:           value.ID,
				SessionID:    value.SessionID,
				RepositoryID: value.RepositoryID,
				RoundID:      value.RoundID,
				OperationID:  value.OperationID,
				Kind:         value.Kind,
				Status:       value.Status,
				Message:      value.Message,
				CreatedAt:    value.CreatedAt,
			},
		)
	}
	return result
}

func loadManifest(
	ctx context.Context,
	store *db.RepositoryStore,
	persisted db.Snapshot,
) ([]SnapshotEntryDescriptor, []SnapshotChangeDescriptor, error) {
	sides := []string{snapshot.TreeSideBase, snapshot.TreeSideTarget}
	if persisted.Kind == snapshot.ComparisonWorktree {
		sides = []string{snapshot.TreeSideHead, snapshot.TreeSideIndex, snapshot.TreeSideWorktree}
	}
	entries := make([]SnapshotEntryDescriptor, 0)
	for _, side := range sides {
		values, err := store.ListSnapshotEntries(ctx, persisted.ID, side)
		if err != nil {
			return nil, nil, fmt.Errorf("build export: list %s entries: %w", side, err)
		}
		for _, value := range values {
			entries = append(
				entries,
				SnapshotEntryDescriptor{
					TreeSide:      value.TreeSide,
					Path:          value.Path,
					Kind:          value.Kind,
					Mode:          value.Mode,
					Size:          value.Size,
					ContentDigest: value.ContentDigest,
					GitOID:        value.GitOID,
					SymlinkTarget: value.SymlinkTarget,
				},
			)
		}
	}
	sort.SliceStable(entries, func(i, j int) bool {
		if entries[i].TreeSide != entries[j].TreeSide {
			return entries[i].TreeSide < entries[j].TreeSide
		}
		return entries[i].Path < entries[j].Path
	})
	changes, err := store.ListSnapshotChanges(ctx, persisted.ID)
	if err != nil {
		return nil, nil, fmt.Errorf("build export: list snapshot changes: %w", err)
	}
	result := make([]SnapshotChangeDescriptor, 0, len(changes))
	for _, value := range changes {
		result = append(
			result,
			SnapshotChangeDescriptor{
				Status:       value.Status,
				BasePath:     value.BasePath,
				TargetPath:   value.TargetPath,
				BaseDigest:   value.BaseDigest,
				TargetDigest: value.TargetDigest,
			},
		)
	}
	sort.SliceStable(result, func(i, j int) bool {
		if result[i].BasePath != result[j].BasePath {
			return result[i].BasePath < result[j].BasePath
		}
		return result[i].TargetPath < result[j].TargetPath
	})
	return entries, result, nil
}

func captureFromStore(ctx context.Context, store *db.RepositoryStore, persisted db.Snapshot) (snapshot.Capture, error) {
	read := func(side string) ([]snapshot.Entry, error) {
		values, err := store.ListSnapshotEntries(ctx, persisted.ID, side)
		if err != nil {
			return nil, err
		}
		result := make([]snapshot.Entry, 0, len(values))
		for _, value := range values {
			result = append(
				result,
				snapshot.Entry{
					Path:          value.Path,
					Kind:          value.Kind,
					Mode:          value.Mode,
					Size:          value.Size,
					ContentDigest: value.ContentDigest,
					GitOID:        value.GitOID,
					SymlinkTarget: value.SymlinkTarget,
				},
			)
		}
		return result, nil
	}
	changeValues, err := store.ListSnapshotChanges(ctx, persisted.ID)
	if err != nil {
		return snapshot.Capture{}, err
	}
	changes := make([]snapshot.Change, 0, len(changeValues))
	for _, value := range changeValues {
		changes = append(
			changes,
			snapshot.Change{
				Status:       value.Status,
				BasePath:     value.BasePath,
				TargetPath:   value.TargetPath,
				BaseDigest:   value.BaseDigest,
				TargetDigest: value.TargetDigest,
			},
		)
	}
	baseSide, targetSide := snapshot.TreeSideBase, snapshot.TreeSideTarget
	if persisted.Kind == snapshot.ComparisonWorktree {
		baseSide, targetSide = snapshot.TreeSideHead, snapshot.TreeSideWorktree
	}
	base, err := read(baseSide)
	if err != nil {
		return snapshot.Capture{}, err
	}
	target, err := read(targetSide)
	if err != nil {
		return snapshot.Capture{}, err
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
		capture.Index.OID = persisted.IndexOID
		capture.Worktree.Entries = append([]snapshot.Entry(nil), target...)
		capture.Worktree.OID = persisted.TargetOID
		capture.Index.Entries, err = read(snapshot.TreeSideIndex)
		if err != nil {
			return snapshot.Capture{}, err
		}
		capture.Head.ManifestDigest, capture.Worktree.ManifestDigest = persisted.BaseManifestDigest, persisted.TargetManifestDigest
		for _, layer := range persisted.Layers {
			if layer.Layer == snapshot.TreeSideIndex {
				capture.Index.ManifestDigest = layer.ManifestDigest
			}
		}
	}
	if err := capture.Validate(); err != nil {
		return snapshot.Capture{}, err
	}
	return capture, nil
}

func objectContent(store *snapshot.ObjectStore) review.ContentReader {
	return func(_ context.Context, digest string) ([]byte, error) {
		file, err := store.Open(digest)
		if err != nil {
			return nil, err
		}
		data, readErr := io.ReadAll(file)
		closeErr := file.Close()
		if readErr != nil {
			return nil, readErr
		}
		return data, closeErr
	}
}

func changeDescriptor(model review.ChangeModel) ChangeDescriptor {
	result := ChangeDescriptor{
		SchemaVersion:  model.SchemaVersion,
		SnapshotDigest: model.SnapshotDigest,
		ComparisonKind: model.ComparisonKind,
		Requested:      model.Requested,
		Digest:         model.Digest,
		Files:          []ChangeFileDescriptor{},
		Surfaces:       append([]review.AffectedSurface{}, model.Surfaces...),
	}
	for _, file := range model.Files {
		descriptor := ChangeFileDescriptor{
			Status:       file.Status,
			BasePath:     file.BasePath,
			TargetPath:   file.TargetPath,
			BaseDigest:   file.BaseDigest,
			TargetDigest: file.TargetDigest,
			Symbols:      append([]review.Symbol{}, file.Symbols...),
			Surfaces:     append([]review.SurfaceKind{}, file.Surfaces...),
			Hunks:        []HunkDescriptor{},
		}
		for _, hunk := range file.Hunks {
			descriptor.Hunks = append(
				descriptor.Hunks,
				HunkDescriptor{
					ID:        hunk.ID,
					Kind:      hunk.Kind,
					OldStart:  hunk.OldStart,
					OldLines:  hunk.OldLines,
					NewStart:  hunk.NewStart,
					NewLines:  hunk.NewLines,
					Binary:    hunk.Binary,
					Available: hunk.Available,
					Digest:    hunk.Digest,
				},
			)
		}
		result.Files = append(result.Files, descriptor)
	}
	return result
}

func (change ChangeDescriptor) withSnapshot(snapshotValue db.Snapshot) ChangeDescriptor {
	if change.SchemaVersion == "" {
		change.SchemaVersion = "mire/v1/change-model"
	}
	if change.SnapshotDigest == "" {
		change.SnapshotDigest = snapshotValue.ManifestDigest
	}
	if change.ComparisonKind == "" {
		change.ComparisonKind = snapshotValue.Kind
	}
	if change.Requested == "" {
		change.Requested = snapshotValue.RequestedComparison
	}
	if change.Files == nil {
		change.Files = []ChangeFileDescriptor{}
	}
	if change.Surfaces == nil {
		change.Surfaces = []review.AffectedSurface{}
	}
	return change
}

func diffPatch(model review.ChangeModel) string {
	var builder strings.Builder
	for _, file := range model.Files {
		if file.Patch == "" {
			continue
		}
		if builder.Len() > 0 && !strings.HasSuffix(builder.String(), "\n") {
			builder.WriteByte('\n')
		}
		builder.WriteString(file.Patch)
		if !strings.HasSuffix(file.Patch, "\n") {
			builder.WriteByte('\n')
		}
	}
	return builder.String()
}

func artifactDescriptor(value review.RetrievedArtifact) ArtifactDescriptor {
	return value.Metadata()
}

func candidateProjections(
	change review.ChangeModel,
	candidates []review.CandidateRecord,
	store *db.RepositoryStore,
	ctx context.Context,
) []CandidateProjection {
	result := make([]CandidateProjection, 0, len(candidates))
	for _, candidate := range candidates {
		projection := CandidateProjection{Candidate: candidate, Lane: review.FindingLaneCandidate}
		verification, err := store.GetLatestVerification(ctx, candidate.ID)
		if err == nil {
			projection.Verification = &verification
			run, runErr := store.GetVerificationRun(ctx, verification.RunID)
			if runErr == nil && change.SnapshotID != "" {
				lane, laneErr := review.DeriveLane(change, candidate, verification, run)
				if laneErr == nil {
					projection.Lane = lane
				} else {
					projection.Reason = laneErr.Error()
				}
			} else if verification.State == review.VerificationRefuted {
				projection.Lane = review.FindingLaneRefuted
			}
		} else {
			projection.Reason = "verification not recorded"
		}
		result = append(result, projection)
	}
	sort.SliceStable(result, func(i, j int) bool { return result[i].Candidate.ID < result[j].Candidate.ID })
	return result
}

func findingProjections(
	change review.ChangeModel,
	findings []review.FindingRevision,
	candidates []review.CandidateRecord,
	verifications []review.VerificationRecord,
	store *db.RepositoryStore,
	ctx context.Context,
) []FindingProjection {
	candidateByID := make(map[string]review.CandidateRecord, len(candidates))
	for _, candidate := range candidates {
		candidateByID[candidate.ID] = candidate
	}
	verificationByCandidate := make(map[string]review.VerificationRecord, len(verifications))
	for _, value := range verifications {
		verificationByCandidate[value.CandidateID] = value
	}
	result := make([]FindingProjection, 0, len(findings))
	for _, finding := range findings {
		projection := FindingProjection{
			Finding:       finding,
			Lane:          review.FindingLaneCandidate,
			CandidateID:   finding.Origin.CandidateID,
			Dispositions:  []review.DispositionRecord{},
			Presentations: []review.PresentationRecord{},
		}
		if candidate, ok := candidateByID[finding.Origin.CandidateID]; ok {
			if verification, verified := verificationByCandidate[candidate.ID]; verified {
				projection.Verification = &verification
				if run, runErr := store.GetVerificationRun(
					ctx,
					verification.RunID,
				); runErr == nil &&
					change.SnapshotID != "" {
					if lane, laneErr := review.DeriveLane(change, candidate, verification, run); laneErr == nil {
						projection.Lane = lane
					}
				} else if verification.State == review.VerificationRefuted {
					projection.Lane = review.FindingLaneRefuted
				}
			}
		} else {
			switch finding.Verification {
			case review.VerificationSupported:
				projection.Lane = review.FindingLaneVerified
			case review.VerificationRefuted:
				projection.Lane = review.FindingLaneRefuted
			}
		}
		if values, err := store.ListDispositions(ctx, finding.FindingID); err == nil {
			projection.Dispositions = values
		}
		if values, err := store.ListPresentations(ctx, finding.FindingID); err == nil {
			projection.Presentations = values
		}
		result = append(result, projection)
	}
	sort.SliceStable(result, func(i, j int) bool {
		if result[i].Finding.FindingID != result[j].Finding.FindingID {
			return result[i].Finding.FindingID < result[j].Finding.FindingID
		}
		return result[i].Finding.Revision < result[j].Finding.Revision
	})
	return result
}

func findingHasSARIFLocation(finding review.FindingRevision) bool {
	_, ok := firstSARIFAnchor(finding)
	return ok
}

func firstSARIFAnchor(finding review.FindingRevision) (review.Anchor, bool) {
	for _, anchor := range finding.Anchors {
		if strings.TrimSpace(anchor.Path) != "" {
			return anchor, true
		}
	}
	return review.Anchor{}, false
}

func projectChatRuns(runs []review.ChatRunRecord) []ChatRunProjection {
	result := make([]ChatRunProjection, 0, len(runs))
	for _, run := range runs {
		artifactIDs := make([]string, 0, len(run.Input.Artifacts))
		for _, artifact := range run.Input.Artifacts {
			artifactIDs = append(artifactIDs, artifact.ID)
		}
		sort.Strings(artifactIDs)
		result = append(
			result,
			ChatRunProjection{
				Run:            sanitizeRun(run.Run),
				UserMessageID:  run.UserMessageID,
				Binding:        run.Binding,
				InputDigest:    run.Input.Digest,
				ArtifactIDs:    artifactIDs,
				Response:       run.Response,
				RetainedOutput: run.RetainedOutput,
			},
		)
	}
	return result
}

func sanitizeRun(value review.RunRecord) review.RunRecord {
	value.Provenance.Parameters = sanitizeParameters(value.Provenance.Parameters)
	value.Provenance.Redactions = append([]string{}, value.Provenance.Redactions...)
	return value
}

var sensitiveParameter = regexp.MustCompile(
	`(?i)(password|secret|token|credential|authorization|api[_-]?key|private[_-]?key)`,
)

func sanitizeParameters(values map[string]any) map[string]any {
	if values == nil {
		return map[string]any{}
	}
	result := make(map[string]any, len(values))
	for key, value := range values {
		if sensitiveParameter.MatchString(key) {
			result[key] = "[REDACTED]"
			continue
		}
		result[key] = sanitizeParameterValue(value)
	}
	return result
}

func sanitizeParameterValue(value any) any {
	switch typed := value.(type) {
	case map[string]any:
		return sanitizeParameters(typed)
	case []any:
		result := make([]any, len(typed))
		for index, item := range typed {
			result[index] = sanitizeParameterValue(item)
		}
		return result
	default:
		return value
	}
}

func normalizeReview(value *Review) {
	if value.SchemaVersion == "" {
		value.SchemaVersion = SchemaVersion
	}
	if value.MIREVersion == "" {
		value.MIREVersion = MIREVersion
	}
	value.SnapshotManifest.Layers = append([]LayerDescriptor{}, value.SnapshotManifest.Layers...)
	sort.SliceStable(value.SnapshotManifest.Layers, func(i, j int) bool {
		return value.SnapshotManifest.Layers[i].Layer < value.SnapshotManifest.Layers[j].Layer
	})
	value.Ledger.Passes = append([]review.PassCoverage{}, value.Ledger.Passes...)
	value.Ledger.Diagnostics = append([]review.ReviewDiagnostic{}, value.Ledger.Diagnostics...)
	value.Ledger.Verifications = append([]review.VerificationRecord{}, value.Ledger.Verifications...)
	for index := range value.Ledger.Candidates {
		scrubCandidate(&value.Ledger.Candidates[index].Candidate)
		if value.Ledger.Candidates[index].Verification != nil {
			scrubVerification(value.Ledger.Candidates[index].Verification)
		}
	}
	for index := range value.Ledger.Findings {
		scrubFinding(&value.Ledger.Findings[index].Finding)
		if value.Ledger.Findings[index].Verification != nil {
			scrubVerification(value.Ledger.Findings[index].Verification)
		}
	}
	for index := range value.Ledger.Verifications {
		scrubVerification(&value.Ledger.Verifications[index])
	}
	value.Artifacts = append([]ArtifactDescriptor{}, value.Artifacts...)
	sort.SliceStable(value.Artifacts, func(i, j int) bool { return value.Artifacts[i].ID < value.Artifacts[j].ID })
	value.Provenance.PlannerRuns = sanitizeRuns(value.Provenance.PlannerRuns)
	value.Provenance.ReviewRuns = sanitizeRuns(value.Provenance.ReviewRuns)
	for i := range value.Provenance.VerificationRuns {
		value.Provenance.VerificationRuns[i].RunRecord = sanitizeRun(value.Provenance.VerificationRuns[i].RunRecord)
	}
	for index := range value.Provenance.ChatRuns {
		scrubChatBinding(&value.Provenance.ChatRuns[index].Binding)
		scrubChatResponse(value.Provenance.ChatRuns[index].Response)
	}
	sort.SliceStable(
		value.Provenance.Operations,
		func(i, j int) bool { return value.Provenance.Operations[i].ID < value.Provenance.Operations[j].ID },
	)
	sort.SliceStable(
		value.Provenance.Activity,
		func(i, j int) bool { return value.Provenance.Activity[i].ID < value.Provenance.Activity[j].ID },
	)
	sort.SliceStable(value.Chat, func(i, j int) bool {
		return value.Chat[i].CreatedAt.Before(value.Chat[j].CreatedAt) ||
			(value.Chat[i].CreatedAt.Equal(value.Chat[j].CreatedAt) && value.Chat[i].ID < value.Chat[j].ID)
	})
	for index := range value.Chat {
		scrubChatMessage(&value.Chat[index])
	}
	value.Coverage = normalizeCoverage(value.Coverage)
	value.Omissions = uniqueOmissions(value.Omissions)
}

func scrubAnchor(anchor *review.Anchor) {
	if anchor != nil {
		anchor.OriginalHunk = ""
	}
}

func scrubCandidate(candidate *review.CandidateRecord) {
	if candidate == nil {
		return
	}
	for index := range candidate.Candidate.Anchors {
		scrubAnchor(&candidate.Candidate.Anchors[index])
	}
}

func scrubEvidence(evidence []review.Evidence) {
	for index := range evidence {
		for anchorIndex := range evidence[index].Anchors {
			scrubAnchor(&evidence[index].Anchors[anchorIndex])
		}
	}
}

func scrubVerification(verification *review.VerificationRecord) {
	if verification == nil {
		return
	}
	for index := range verification.ConcretePath {
		for anchorIndex := range verification.ConcretePath[index].Anchors {
			scrubAnchor(&verification.ConcretePath[index].Anchors[anchorIndex])
		}
	}
	scrubEvidence(verification.GuardEvidence)
	scrubEvidence(verification.TestEvidence)
	scrubEvidence(verification.Evidence)
}

func scrubFinding(finding *review.FindingRevision) {
	if finding == nil {
		return
	}
	for index := range finding.Anchors {
		scrubAnchor(&finding.Anchors[index])
	}
	scrubEvidence(finding.Evidence)
}

func scrubChatBinding(binding *review.ChatBinding) {
	if binding == nil {
		return
	}
	for index := range binding.Context.References {
		scrubChatReference(&binding.Context.References[index])
	}
	scrubChatReference(&binding.Context.Primary)
}

func scrubChatReference(reference *review.ChatReference) {
	if reference != nil && reference.DiffAnchor != nil {
		scrubAnchor(reference.DiffAnchor)
	}
}

func scrubChatResponse(response *review.ChatResponse) {
	if response == nil || response.CandidateProposal == nil {
		return
	}
	for index := range response.CandidateProposal.Anchors {
		scrubAnchor(&response.CandidateProposal.Anchors[index])
	}
}

func scrubChatMessage(message *review.ChatMessage) {
	if message == nil {
		return
	}
	for index := range message.Context.References {
		scrubChatReference(&message.Context.References[index])
	}
	scrubChatReference(&message.Context.Primary)
	scrubChatResponse(message.Response)
}

func sanitizeRuns(values []review.RunRecord) []review.RunRecord {
	result := make([]review.RunRecord, 0, len(values))
	for _, value := range values {
		result = append(result, sanitizeRun(value))
	}
	return result
}

func normalizeCoverage(value review.ReviewCoverage) review.ReviewCoverage {
	value.ExaminedFiles = sortedUnique(value.ExaminedFiles)
	value.ExaminedHunks = sortedUnique(value.ExaminedHunks)
	if value.ExaminedFiles == nil {
		value.ExaminedFiles = []string{}
	}
	if value.ExaminedHunks == nil {
		value.ExaminedHunks = []string{}
	}
	if value.Passes == nil {
		value.Passes = []review.PassCoverage{}
	}
	if value.RetrievedArtifacts == nil {
		value.RetrievedArtifacts = []review.RetrievedArtifact{}
	}
	sort.SliceStable(
		value.RetrievedArtifacts,
		func(i, j int) bool { return value.RetrievedArtifacts[i].ID < value.RetrievedArtifacts[j].ID },
	)
	sort.SliceStable(value.Passes, func(i, j int) bool {
		if value.Passes[i].Order != value.Passes[j].Order {
			return value.Passes[i].Order < value.Passes[j].Order
		}
		return value.Passes[i].Name < value.Passes[j].Name
	})
	sort.SliceStable(value.Analyzers, func(i, j int) bool { return value.Analyzers[i].Name < value.Analyzers[j].Name })
	value.Gaps = sortedUnique(value.Gaps)
	if value.Analyzers == nil {
		value.Analyzers = []review.AnalyzerAvailability{}
	}
	if value.Exclusions == nil {
		value.Exclusions = []review.CoverageExclusion{}
	}
	if value.Failures == nil {
		value.Failures = []review.CoverageFailure{}
	}
	if value.Gaps == nil {
		value.Gaps = []string{}
	}
	value.Digest = ""
	if data, err := json.Marshal(value); err == nil {
		value.Digest = shared.Digest(data)
	}
	return value
}

func sortedUnique(values []string) []string {
	seen := make(map[string]struct{}, len(values))
	result := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		result = append(result, value)
	}
	sort.Strings(result)
	return result
}

func uniqueOmissions(values []Omission) []Omission {
	seen := map[string]bool{}
	result := make([]Omission, 0, len(values))
	for _, value := range values {
		value.Kind, value.Reason = strings.TrimSpace(value.Kind), strings.TrimSpace(value.Reason)
		if value.Kind == "" || value.Reason == "" {
			continue
		}
		key := value.Kind + "\x00" + value.Reason
		if !seen[key] {
			seen[key] = true
			result = append(result, value)
		}
	}
	sort.SliceStable(result, func(i, j int) bool {
		if result[i].Kind != result[j].Kind {
			return result[i].Kind < result[j].Kind
		}
		return result[i].Reason < result[j].Reason
	})
	return result
}

// CanonicalJSON returns the deterministic review.json bytes.
func CanonicalJSON(value Review) ([]byte, error) {
	normalizeReview(&value)
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encode review export: %w", err)
	}
	return append(data, '\n'), nil
}

// Markdown returns the deterministic human front door for a review.
func Markdown(value Review) []byte {
	normalizeReview(&value)
	var b strings.Builder
	fmt.Fprintf(
		&b,
		"# MIRE Review\n\nSession: %s\nRound: %s\nSnapshot: %s\nStatus: %s\n\n",
		value.Session.ID,
		value.Round.ID,
		value.SnapshotManifest.ID,
		value.Round.Status,
	)
	b.WriteString("## Diff\n\n")
	if value.DiffPatch == "" {
		b.WriteString("Diff content is unavailable from the exported manifest.\n\n")
	} else {
		b.WriteString("    ")
		b.WriteString(strings.ReplaceAll(value.DiffPatch, "\n", "\n    "))
		b.WriteString("\n\n")
	}
	writeFindingSection(&b, "Verified findings", value.Ledger.Findings, review.FindingLaneVerified)
	b.WriteString("## Candidates\n\n")
	for _, candidate := range value.Ledger.Candidates {
		if candidate.Lane != review.FindingLaneCandidate {
			continue
		}
		fmt.Fprintf(
			&b,
			"- `%s` (%s, %s): %s — %s\n",
			candidate.Candidate.ID,
			candidate.Candidate.Candidate.Severity,
			candidate.Candidate.Candidate.Category,
			candidate.Candidate.Candidate.Claim,
			candidate.Reason,
		)
	}
	if !strings.HasSuffix(b.String(), "\n\n") {
		b.WriteString("\n")
	}
	b.WriteString("## Refuted / audit\n\n")
	for _, candidate := range value.Ledger.Candidates {
		if candidate.Lane == review.FindingLaneRefuted {
			fmt.Fprintf(&b, "- `%s`: %s\n", candidate.Candidate.ID, candidate.Reason)
		}
	}
	for _, finding := range value.Ledger.Findings {
		if finding.Lane == review.FindingLaneRefuted {
			fmt.Fprintf(
				&b,
				"- `%s/%d`: %s\n",
				finding.Finding.FindingID,
				finding.Finding.Revision,
				finding.Finding.Claim,
			)
		}
	}
	b.WriteString("\n## Chat\n\n")
	if len(value.Chat) == 0 {
		b.WriteString("No chat turns recorded.\n\n")
	}
	for _, message := range value.Chat {
		fmt.Fprintf(&b, "- %s `%s`: %s\n", message.Role, message.ID, strings.ReplaceAll(message.Body, "\n", " "))
	}
	b.WriteString("\n## Coverage\n\n")
	fmt.Fprintf(
		&b,
		"Examined files: %d\nExamined hunks: %d\n",
		len(value.Coverage.ExaminedFiles),
		len(value.Coverage.ExaminedHunks),
	)
	for _, pass := range value.Ledger.Passes {
		fmt.Fprintf(&b, "- %s: %s (%s)\n", pass.Name, pass.Status, pass.Reason)
	}
	for _, gap := range value.Coverage.Gaps {
		fmt.Fprintf(&b, "- gap: %s\n", gap)
	}
	for _, failure := range value.Coverage.Failures {
		fmt.Fprintf(&b, "- failure: %s: %s\n", failure.Code, failure.Message)
	}
	b.WriteString("\n## Incomplete analysis\n\n")
	if len(value.Omissions) == 0 && len(value.Coverage.Failures) == 0 &&
		value.Round.Status != string(db.RoundStatusIncomplete) {
		b.WriteString("No incomplete-analysis diagnostics recorded.\n")
	} else {
		for _, omission := range value.Omissions {
			fmt.Fprintf(&b, "- %s: %s\n", omission.Kind, omission.Reason)
		}
		for _, failure := range value.Coverage.Failures {
			fmt.Fprintf(&b, "- %s: %s\n", failure.Code, failure.Message)
		}
	}
	return []byte(b.String())
}

func writeFindingSection(b *strings.Builder, title string, findings []FindingProjection, lane review.FindingLane) {
	fmt.Fprintf(b, "## %s\n\n", title)
	count := 0
	for _, finding := range findings {
		if finding.Lane != lane {
			continue
		}
		count++
		anchor := ""
		if len(finding.Finding.Anchors) > 0 {
			anchor = finding.Finding.Anchors[0].Path + "#" + finding.Finding.Anchors[0].HunkID
		}
		fmt.Fprintf(
			b,
			"### %s/%d\n\n%s\n\n- Severity: %s\n- Category: %s\n- Anchor: `%s`\n",
			finding.Finding.FindingID,
			finding.Finding.Revision,
			finding.Finding.Claim,
			finding.Finding.Severity,
			finding.Finding.Category,
			anchor,
		)
		for _, evidence := range finding.Finding.Evidence {
			fmt.Fprintf(b, "- Evidence (%s): %s\n", evidence.Relation, evidence.Summary)
		}
		b.WriteString("\n")
	}
	if count == 0 {
		b.WriteString("No findings in this lane.\n\n")
	}
}

// FindingsJSON returns a focused, lane-aware findings projection.
func FindingsJSON(value Review) ([]byte, error) {
	type output struct {
		SchemaVersion string                `json:"schema_version"`
		Verified      []FindingProjection   `json:"verified"`
		Candidates    []CandidateProjection `json:"candidates"`
		Refuted       []FindingProjection   `json:"refuted"`
	}
	result := output{
		SchemaVersion: SchemaVersion,
		Verified:      []FindingProjection{},
		Candidates:    []CandidateProjection{},
		Refuted:       []FindingProjection{},
	}
	for _, finding := range value.Ledger.Findings {
		if finding.Lane == review.FindingLaneVerified {
			result.Verified = append(result.Verified, finding)
		}
		if finding.Lane == review.FindingLaneRefuted {
			result.Refuted = append(result.Refuted, finding)
		}
	}
	for _, candidate := range value.Ledger.Candidates {
		if candidate.Lane == review.FindingLaneCandidate {
			result.Candidates = append(result.Candidates, candidate)
		}
	}
	data, err := json.MarshalIndent(result, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(data, '\n'), nil
}

// SARIF returns a findings-only SARIF 2.1.0 projection. Findings without a
// usable path are omitted and recorded in the canonical review omissions by
// callers that need that distinction.
func SARIF(value Review) ([]byte, error) {
	type artifactLocation struct {
		URI string `json:"uri"`
	}
	type region struct {
		StartLine int `json:"startLine,omitempty"`
		EndLine   int `json:"endLine,omitempty"`
	}
	type location struct {
		Physical struct {
			ArtifactLocation artifactLocation `json:"artifactLocation"`
			Region           *region          `json:"region,omitempty"`
		} `json:"physicalLocation"`
	}
	type result struct {
		RuleID    string `json:"ruleId"`
		RuleIndex int    `json:"ruleIndex,omitempty"`
		Level     string `json:"level"`
		Message   struct {
			Text string `json:"text"`
		} `json:"message"`
		Locations           []location        `json:"locations"`
		Fingerprints        map[string]string `json:"fingerprints,omitempty"`
		PartialFingerprints map[string]string `json:"partialFingerprints"`
		Properties          map[string]any    `json:"properties"`
	}
	type rule struct {
		ID               string `json:"id"`
		Name             string `json:"name"`
		ShortDescription struct {
			Text string `json:"text"`
		} `json:"shortDescription"`
	}
	type sarif struct {
		Version string `json:"version"`
		Schema  string `json:"$schema"`
		Runs    []struct {
			Tool struct {
				Driver struct {
					Name           string `json:"name"`
					Version        string `json:"version"`
					InformationURI string `json:"informationUri"`
					Rules          []rule `json:"rules"`
				} `json:"driver"`
			} `json:"tool"`
			Results []result `json:"results"`
		} `json:"runs"`
	}
	rulesByID := map[string]rule{}
	results := make([]result, 0)
	for _, finding := range value.Ledger.Findings {
		if finding.Lane == review.FindingLaneRefuted || len(finding.Finding.Anchors) == 0 {
			continue
		}
		anchor, ok := firstSARIFAnchor(finding.Finding)
		if !ok {
			continue
		}
		ruleID := "mire." + sarifID(finding.Finding.Category)
		if _, ok := rulesByID[ruleID]; !ok {
			description := rule{ID: ruleID, Name: finding.Finding.Category}
			description.ShortDescription.Text = finding.Finding.Category
			rulesByID[ruleID] = description
		}
		identity := finding.Finding.FindingID + "/" + fmt.Sprint(finding.Finding.Revision)
		item := result{
			RuleID: ruleID,
			Level:  sarifLevel(finding.Finding.Severity), Locations: []location{},
			Fingerprints:        map[string]string{"mireResult": identity},
			PartialFingerprints: map[string]string{"mireFinding": finding.Finding.Digest},
			Properties: map[string]any{
				"lane": string(
					finding.Lane,
				),
				"severity":         finding.Finding.Severity,
				"finding_id":       finding.Finding.FindingID,
				"finding_revision": finding.Finding.Revision,
			},
		}
		item.Message.Text = strings.TrimSpace(finding.Finding.Claim + " — " + finding.Finding.Impact)
		var loc location
		loc.Physical.ArtifactLocation.URI = anchor.Path
		if anchor.StartLine > 0 {
			loc.Physical.Region = &region{
				StartLine: anchor.StartLine,
				EndLine:   shared.MaxInt(anchor.EndLine, anchor.StartLine),
			}
		}
		item.Locations = append(item.Locations, loc)
		results = append(results, item)
	}
	rules := make([]rule, 0, len(rulesByID))
	for _, value := range rulesByID {
		rules = append(rules, value)
	}
	sort.SliceStable(rules, func(i, j int) bool { return rules[i].ID < rules[j].ID })
	sort.SliceStable(results, func(i, j int) bool { return results[i].RuleID < results[j].RuleID })
	ruleIndexes := make(map[string]int, len(rules))
	for index, value := range rules {
		ruleIndexes[value.ID] = index
	}
	for index := range results {
		results[index].RuleIndex = ruleIndexes[results[index].RuleID]
	}
	var run struct {
		Tool struct {
			Driver struct {
				Name           string `json:"name"`
				Version        string `json:"version"`
				InformationURI string `json:"informationUri"`
				Rules          []rule `json:"rules"`
			} `json:"driver"`
		} `json:"tool"`
		Results []result `json:"results"`
	}
	run.Tool.Driver.Name, run.Tool.Driver.Version, run.Tool.Driver.InformationURI, run.Tool.Driver.Rules, run.Results = "mire", "v1", "https://github.com/stormlightlabs/mire", rules, results
	output := sarif{Version: "2.1.0", Schema: "https://json.schemastore.org/sarif-2.1.0.json", Runs: []struct {
		Tool struct {
			Driver struct {
				Name           string `json:"name"`
				Version        string `json:"version"`
				InformationURI string `json:"informationUri"`
				Rules          []rule `json:"rules"`
			} `json:"driver"`
		} `json:"tool"`
		Results []result `json:"results"`
	}{run}}
	data, err := json.MarshalIndent(output, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(data, '\n'), nil
}

func sarifID(value string) string {
	value = strings.ToLower(strings.TrimSpace(value))
	value = regexp.MustCompile(`[^a-z0-9._-]+`).ReplaceAllString(value, "-")
	if value == "" {
		return "finding"
	}
	return value
}

func sarifLevel(value string) string {
	switch strings.ToLower(value) {
	case "critical", "high":
		return "error"
	case "medium":
		return "warning"
	default:
		return "note"
	}
}

// ChatJSONL returns one canonical JSON object per chat message.
func ChatJSONL(value Review) ([]byte, error) { return shared.Jsonl(value.Chat) }

// ActivityJSONL returns one canonical JSON object per durable activity event.
func ActivityJSONL(value Review) ([]byte, error) { return shared.Jsonl(value.Provenance.Activity) }

// EvidenceJSONL returns one canonical JSON object per finding evidence record.
func EvidenceJSONL(value Review) ([]byte, error) {
	type entry struct {
		FindingID string          `json:"finding_id"`
		Revision  int             `json:"revision"`
		Evidence  review.Evidence `json:"evidence"`
	}
	values := make([]entry, 0)
	for _, finding := range value.Ledger.Findings {
		for _, evidence := range finding.Finding.Evidence {
			values = append(
				values,
				entry{FindingID: finding.Finding.FindingID, Revision: finding.Finding.Revision, Evidence: evidence},
			)
		}
	}
	sort.SliceStable(values, func(i, j int) bool {
		if values[i].FindingID != values[j].FindingID {
			return values[i].FindingID < values[j].FindingID
		}
		if values[i].Revision != values[j].Revision {
			return values[i].Revision < values[j].Revision
		}
		return values[i].Evidence.ID < values[j].Evidence.ID
	})
	return shared.Jsonl(values)
}

// BundleManifest describes every generated file and explicitly states that
// the bundle is not an import or replay format.
type BundleManifest struct {
	SchemaVersion           string            `json:"schema_version"`
	ExportKind              string            `json:"export_kind"`
	ReviewSchemaVersion     string            `json:"review_schema_version"`
	MIREVersion             string            `json:"mire_version"`
	RepositoryIdentity      string            `json:"repository_identity"`
	SessionID               string            `json:"session_id"`
	RoundID                 string            `json:"round_id"`
	SnapshotDigest          string            `json:"snapshot_digest"`
	PolicyHash              string            `json:"policy_hash"`
	ConfigurationDigest     string            `json:"configuration_digest"`
	CoverageDigest          string            `json:"coverage_digest"`
	Files                   []string          `json:"files"`
	FileDigests             map[string]string `json:"file_digests"`
	EvidenceArtifacts       []string          `json:"evidence_artifacts"`
	ExcludedSnapshotObjects []string          `json:"excluded_snapshot_objects"`
	Omissions               []Omission        `json:"omissions"`
	Importable              bool              `json:"importable"`
	Warning                 string            `json:"warning"`
}

// Rendered contains all bytes required by a requested format.
type Rendered struct {
	ReviewJSON        []byte
	Markdown          []byte
	FindingsJSON      []byte
	EvidenceJSONL     []byte
	ChatJSONL         []byte
	ActivityJSONL     []byte
	SARIF             []byte
	DiffPatch         []byte
	ManifestJSON      []byte
	EvidenceArtifacts []EvidenceArtifact
}

// Render creates all projections once so every requested view shares the same
// domain projection.
func Render(value Review) (Rendered, error) {
	normalizeReview(&value)
	reviewJSON, err := CanonicalJSON(value)
	if err != nil {
		return Rendered{}, err
	}
	findingsJSON, err := FindingsJSON(value)
	if err != nil {
		return Rendered{}, err
	}
	evidenceJSONL, err := EvidenceJSONL(value)
	if err != nil {
		return Rendered{}, err
	}
	chatJSONL, err := ChatJSONL(value)
	if err != nil {
		return Rendered{}, err
	}
	activityJSONL, err := ActivityJSONL(value)
	if err != nil {
		return Rendered{}, err
	}
	sarifJSON, err := SARIF(value)
	if err != nil {
		return Rendered{}, err
	}
	named := append([]EvidenceArtifact{}, value.artifactContents...)
	names := make([]string, 0, len(named))
	for _, artifact := range named {
		names = append(names, artifact.Path)
	}
	sort.Strings(names)
	files := []string{
		"REVIEW.md",
		"review.json",
		"manifest.json",
		"diff.patch",
		"findings.json",
		"evidence.jsonl",
		"chat.jsonl",
		"activity.jsonl",
		"findings.sarif",
	}
	files = append(files, names...)
	sort.Strings(files)
	digests := map[string]string{
		"REVIEW.md":      shared.Digest(Markdown(value)),
		"review.json":    shared.Digest(reviewJSON),
		"diff.patch":     shared.Digest([]byte(value.DiffPatch)),
		"findings.json":  shared.Digest(findingsJSON),
		"evidence.jsonl": shared.Digest(evidenceJSONL),
		"chat.jsonl":     shared.Digest(chatJSONL),
		"activity.jsonl": shared.Digest(activityJSONL),
		"findings.sarif": shared.Digest(sarifJSON),
	}
	configurationData, _ := json.Marshal(value.Provenance)
	excludedObjects := make([]string, 0)
	for _, entry := range value.SnapshotManifest.Entries {
		if entry.ContentDigest != "" {
			excludedObjects = append(excludedObjects, entry.ContentDigest)
		}
	}

	excludedObjects = sortedUnique(excludedObjects)
	manifest := BundleManifest{
		SchemaVersion:           ManifestSchemaVersion,
		ExportKind:              "portable_review_bundle",
		ReviewSchemaVersion:     SchemaVersion,
		MIREVersion:             MIREVersion,
		RepositoryIdentity:      value.Session.RepositoryIdentity,
		SessionID:               value.Session.ID,
		RoundID:                 value.Round.ID,
		SnapshotDigest:          value.SnapshotManifest.ManifestDigest,
		PolicyHash:              value.SnapshotManifest.ContextPolicyHash,
		ConfigurationDigest:     shared.Digest(configurationData),
		CoverageDigest:          value.Coverage.Digest,
		Files:                   files,
		FileDigests:             digests,
		EvidenceArtifacts:       names,
		ExcludedSnapshotObjects: excludedObjects,
		Omissions:               value.Omissions,
		Importable:              false,
		Warning:                 "This bundle is an inspectable export, not a V1 import or replay format; code and conversation may be sensitive.",
	}
	manifestJSON, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return Rendered{}, err
	}
	manifestJSON = append(manifestJSON, '\n')
	return Rendered{
		ReviewJSON:        reviewJSON,
		Markdown:          Markdown(value),
		FindingsJSON:      findingsJSON,
		EvidenceJSONL:     evidenceJSONL,
		ChatJSONL:         chatJSONL,
		ActivityJSONL:     activityJSONL,
		SARIF:             sarifJSON,
		DiffPatch:         []byte(value.DiffPatch),
		ManifestJSON:      manifestJSON,
		EvidenceArtifacts: named,
	}, nil
}

func evidencePath(id string) string {
	id = strings.TrimSpace(id)
	id = regexp.MustCompile(`[^A-Za-z0-9._-]+`).ReplaceAllString(id, "-")
	if id == "" {
		id = "artifact"
	}
	return "evidence/" + id + ".txt"
}

// Write writes one requested format and refuses to overwrite any destination.
func Write(value Review, format Format, destination string) error {
	if !format.Valid() {
		return fmt.Errorf("write export: unsupported format %q", format)
	}
	destination = strings.TrimSpace(destination)
	if destination == "" {
		return errors.New("write export: destination is required")
	}
	rendered, err := Render(value)
	if err != nil {
		return err
	}
	if format == FormatBundle {
		return writeBundle(destination, rendered)
	}
	if info, statErr := os.Lstat(destination); statErr == nil {
		if info.IsDir() {
			return fmt.Errorf("write export: destination %q is a directory", destination)
		}
		return fmt.Errorf("write export: destination %q already exists", destination)
	} else if !errors.Is(statErr, os.ErrNotExist) {
		return fmt.Errorf("write export: inspect destination: %w", statErr)
	}
	var data []byte
	switch format {
	case FormatMarkdown:
		data = rendered.Markdown
	case FormatJSON:
		data = rendered.ReviewJSON
	case FormatSARIF:
		data = rendered.SARIF
	}
	if err := os.MkdirAll(filepath.Dir(destination), 0o700); err != nil {
		return fmt.Errorf("write export: create destination directory: %w", err)
	}
	temporary, err := os.CreateTemp(filepath.Dir(destination), ".mire-export-")
	if err != nil {
		return fmt.Errorf("write export: create temporary file: %w", err)
	}
	temporaryName := temporary.Name()
	defer func() { _ = os.Remove(temporaryName) }()
	if err := temporary.Chmod(0o600); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("write export: restrict temporary file: %w", err)
	}
	if _, err := temporary.Write(data); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("write export: write temporary file: %w", err)
	}
	if err := temporary.Sync(); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("write export: sync temporary file: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("write export: close temporary file: %w", err)
	}
	if err := os.Link(temporaryName, destination); err != nil {
		if errors.Is(err, os.ErrExist) {
			return fmt.Errorf("write export: destination %q already exists", destination)
		}
		return fmt.Errorf("write export: publish destination: %w", err)
	}
	return nil
}

func writeBundle(destination string, rendered Rendered) error {
	if _, err := os.Lstat(destination); err == nil {
		return fmt.Errorf("write export: bundle destination %q already exists", destination)
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("write export: inspect bundle destination: %w", err)
	}
	if err := os.MkdirAll(destination, 0o700); err != nil {
		return fmt.Errorf("write export: create bundle: %w", err)
	}
	complete := false
	defer func() {
		if !complete {
			_ = os.RemoveAll(destination)
		}
	}()
	files := map[string][]byte{
		"REVIEW.md":      rendered.Markdown,
		"review.json":    rendered.ReviewJSON,
		"manifest.json":  rendered.ManifestJSON,
		"diff.patch":     rendered.DiffPatch,
		"findings.json":  rendered.FindingsJSON,
		"evidence.jsonl": rendered.EvidenceJSONL,
		"chat.jsonl":     rendered.ChatJSONL,
		"activity.jsonl": rendered.ActivityJSONL,
		"findings.sarif": rendered.SARIF,
	}
	for _, artifact := range rendered.EvidenceArtifacts {
		files[artifact.Path] = []byte(artifact.Content)
	}
	names := make([]string, 0, len(files))
	for name := range files {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		target := filepath.Join(destination, filepath.FromSlash(name))
		if err := snapshot.ValidateRepositoryPath(name); err != nil && !strings.HasPrefix(name, "evidence/") {
			return fmt.Errorf("write export: invalid bundle member %q", name)
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			return err
		}
		if err := os.WriteFile(target, files[name], 0o600); err != nil {
			return fmt.Errorf("write export: write %s: %w", name, err)
		}
	}
	complete = true
	return nil
}
