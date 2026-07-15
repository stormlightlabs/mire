package review

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"sort"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/snapshot"
)

const (
	// FindingSchemaVersion identifies the immutable ledger schema.
	FindingSchemaVersion = "mire/v1/finding-revision"

	// FindingPresentationSchemaVersion identifies versioned publishable wording.
	FindingPresentationSchemaVersion = "mire/v1/finding-presentation"
)

// FindingOrigin records the retained outputs that first produced a finding.
// It is provenance only; it does not grant authority to change the finding.
type FindingOrigin struct {
	CandidateID       string `json:"candidate_id,omitempty"`
	ReviewRunID       string `json:"review_run_id,omitempty"`
	PassName          string `json:"pass_name,omitempty"`
	VerificationRunID string `json:"verification_run_id,omitempty"`
	ChatMessageID     string `json:"chat_message_id,omitempty"`
	Source            string `json:"source,omitempty"`
}

// FindingRelationshipKind describes an explicit relationship between finding
// revisions. Relationships are references and never rewrite their target.
type FindingRelationshipKind string

const (
	// FindingRelationshipPredecessor links a revision to the prior revision
	// that supplied its stable finding identity.
	FindingRelationshipPredecessor FindingRelationshipKind = "predecessor"
	// FindingRelationshipPossibleSuccessor records an ambiguous continuity
	// candidate without claiming that it is the same finding.
	FindingRelationshipPossibleSuccessor FindingRelationshipKind = "possible_successor"
	// FindingRelationshipDuplicate records an ambiguous duplicate relationship.
	FindingRelationshipDuplicate FindingRelationshipKind = "duplicate"
)

// FindingRelationship links one immutable revision to another revision.
type FindingRelationship struct {
	Kind      FindingRelationshipKind `json:"kind"`
	FindingID string                  `json:"finding_id"`
	Revision  int                     `json:"revision"`
	Reason    string                  `json:"reason,omitempty"`
}

// FindingRevision is an immutable, revision-aware finding record. Verification
// and human disposition are intentionally separate axes; disposition events
// are stored independently of this record.
type FindingRevision struct {
	SchemaVersion      string                `json:"schema_version"`
	FindingID          string                `json:"finding_id"`
	Revision           int                   `json:"revision"`
	SessionID          string                `json:"session_id"`
	RoundID            string                `json:"round_id"`
	SnapshotID         string                `json:"snapshot_id"`
	Claim              string                `json:"claim"`
	Invariant          string                `json:"invariant,omitempty"`
	Impact             string                `json:"impact"`
	Category           string                `json:"category"`
	Severity           string                `json:"severity"`
	Confidence         float64               `json:"confidence"`
	Verification       VerificationState     `json:"verification"`
	VerificationRunID  string                `json:"verification_run_id,omitempty"`
	VerificationDigest string                `json:"verification_digest,omitempty"`
	Anchors            []Anchor              `json:"anchors"`
	Evidence           []Evidence            `json:"evidence,omitempty"`
	Origin             FindingOrigin         `json:"origin"`
	Relationships      []FindingRelationship `json:"relationships,omitempty"`
	CreatedAt          time.Time             `json:"created_at"`
	Digest             string                `json:"digest"`
}

// FindingRevisionOptions supplies optional context when constructing a
// finding from a retained candidate and verifier result.
type FindingRevisionOptions struct {
	RoundID         string
	Verification    *VerificationRecord
	VerificationRun *VerificationRunRecord
	FindingID       string
	Revision        int
	Now             func() time.Time
}

// NewFindingRevision converts a retained candidate and optional verifier
// result into a snapshot-bound finding revision. The variadic arguments accept
// FindingRevisionOptions, VerificationResult, VerificationRecord,
// VerificationRunRecord, a round ID string, and/or a clock time.Time so callers
// can add only the provenance they have at the boundary.
func NewFindingRevision(change ChangeModel, candidate CandidateRecord, inputs ...any) (FindingRevision, error) {
	options := FindingRevisionOptions{}
	for _, input := range inputs {
		switch value := input.(type) {
		case FindingRevisionOptions:
			options = mergeFindingRevisionOptions(options, value)
		case *FindingRevisionOptions:
			if value != nil {
				options = mergeFindingRevisionOptions(options, *value)
			}
		case VerificationResult:
			options.Verification = &value.Verification
			options.VerificationRun = &value.Run
		case *VerificationResult:
			if value != nil {
				options.Verification = &value.Verification
				options.VerificationRun = &value.Run
			}
		case VerificationRecord:
			copyValue := value
			options.Verification = &copyValue
		case *VerificationRecord:
			options.Verification = value
		case VerificationRunRecord:
			copyValue := value
			options.VerificationRun = &copyValue
		case *VerificationRunRecord:
			options.VerificationRun = value
		case string:
			if options.RoundID == "" {
				options.RoundID = value
			}
		case time.Time:
			clock := value
			options.Now = func() time.Time { return clock }
		case func() time.Time:
			options.Now = value
		default:
			return FindingRevision{}, fmt.Errorf("new finding revision: unsupported input %T", input)
		}
	}
	if options.Now == nil {
		options.Now = time.Now
	}
	if options.Verification != nil && options.RoundID == "" {
		options.RoundID = options.Verification.RoundID
	}
	if options.VerificationRun != nil && options.RoundID == "" {
		options.RoundID = options.VerificationRun.RoundID
	}

	normalizedCandidate, err := candidate.Candidate.normalize(change)
	if err != nil {
		return FindingRevision{}, fmt.Errorf("new finding revision: %w", err)
	}
	anchors := make([]Anchor, 0, len(normalizedCandidate.Anchors))
	for _, candidateAnchor := range normalizedCandidate.Anchors {
		anchor, err := NormalizeFindingAnchor(change, candidateAnchor)
		if err != nil {
			return FindingRevision{}, fmt.Errorf("new finding revision: %w", err)
		}
		anchors = append(anchors, anchor)
	}

	verificationState := VerificationNotRun
	verificationRunID := ""
	verificationDigest := ""
	evidence := make([]Evidence, 0)
	invariant := normalizedCandidate.Claim
	if options.Verification != nil {
		verificationState = options.Verification.State
		verificationRunID = options.Verification.RunID
		verificationDigest = options.Verification.Digest
		if verificationDigest != "" && verificationDigest != VerificationDigest(*options.Verification) {
			return FindingRevision{}, errors.New("new finding revision: verification digest does not match record")
		}
		if strings.TrimSpace(options.Verification.SuspectedInvariant) != "" {
			invariant = strings.TrimSpace(options.Verification.SuspectedInvariant)
		}
		for _, value := range options.Verification.Evidence {
			normalizedEvidence, err := normalizeFindingEvidence(change, value)
			if err != nil {
				return FindingRevision{}, fmt.Errorf("new finding revision: %w", err)
			}
			evidence = append(evidence, normalizedEvidence)
		}
	}
	if !verificationState.Valid() {
		return FindingRevision{}, fmt.Errorf("new finding revision: verification state %q is unsupported", verificationState)
	}
	if options.VerificationRun != nil && verificationRunID == "" {
		verificationRunID = options.VerificationRun.ID
	}
	if verificationState != VerificationNotRun && verificationRunID == "" {
		return FindingRevision{}, errors.New("new finding revision: verified finding needs verifier run provenance")
	}

	now := options.Now().UTC()
	if now.IsZero() {
		return FindingRevision{}, errors.New("new finding revision: clock returned zero time")
	}
	revision := FindingRevision{
		SchemaVersion: FindingSchemaVersion, FindingID: options.FindingID, Revision: options.Revision,
		SessionID: change.SessionID, RoundID: options.RoundID, SnapshotID: change.SnapshotID,
		Claim: normalizedCandidate.Claim, Invariant: invariant, Impact: normalizedCandidate.Impact,
		Category: normalizedCandidate.Category, Severity: normalizedCandidate.Severity,
		Confidence: normalizedCandidate.Confidence, Verification: verificationState,
		VerificationRunID: verificationRunID, VerificationDigest: verificationDigest,
		Anchors: anchors, Evidence: evidence,
		Origin: FindingOrigin{CandidateID: candidate.ID, ReviewRunID: candidate.RunID, PassName: candidate.PassName,
			VerificationRunID: verificationRunID, Source: "review_candidate"},
		CreatedAt: now,
	}
	if revision.Verification == VerificationNotRun && options.VerificationRun != nil && options.VerificationRun.ID != "" {
		revision.Origin.VerificationRunID = options.VerificationRun.ID
	}
	return normalizeFindingRevision(revision, false)
}

func mergeFindingRevisionOptions(base, overlay FindingRevisionOptions) FindingRevisionOptions {
	if overlay.RoundID != "" {
		base.RoundID = overlay.RoundID
	}
	if overlay.Verification != nil {
		base.Verification = overlay.Verification
	}
	if overlay.VerificationRun != nil {
		base.VerificationRun = overlay.VerificationRun
	}
	if overlay.FindingID != "" {
		base.FindingID = overlay.FindingID
	}
	if overlay.Revision != 0 {
		base.Revision = overlay.Revision
	}
	if overlay.Now != nil {
		base.Now = overlay.Now
	}
	return base
}

// NormalizeFindingAnchor upgrades a reviewer anchor with immutable snapshot
// location data. It deliberately ignores line numbers when producing identity
// material, so moving a finding within the same content can preserve identity.
func NormalizeFindingAnchor(change ChangeModel, candidate Anchor) (Anchor, error) {
	if candidate.SnapshotID == "" {
		candidate.SnapshotID = change.SnapshotID
	}
	if candidate.SnapshotID != change.SnapshotID {
		return Anchor{}, errors.New("finding anchor belongs to another snapshot")
	}
	if candidate.Side == "" {
		candidate.Side = snapshot.TreeSideTarget
	}
	if candidate.Layer == "" {
		candidate.Layer = candidate.Side
	}
	switch candidate.Side {
	case snapshot.TreeSideBase, snapshot.TreeSideTarget, snapshot.TreeSideHead, snapshot.TreeSideIndex, snapshot.TreeSideWorktree:
	default:
		return Anchor{}, fmt.Errorf("finding anchor side %q is unsupported", candidate.Side)
	}
	if candidate.HunkID == "" {
		return Anchor{}, errors.New("finding anchor hunk ID is required")
	}
	for _, file := range change.Files {
		path := file.TargetPath
		if path == "" {
			path = file.BasePath
		}
		for _, hunk := range file.Hunks {
			if hunk.ID != candidate.HunkID {
				continue
			}
			if candidate.Path == "" {
				candidate.Path = path
			}
			if candidate.Path != path {
				return Anchor{}, fmt.Errorf("finding anchor path %q does not match hunk %q", candidate.Path, candidate.HunkID)
			}
			if candidate.HunkDigest == "" {
				candidate.HunkDigest = hunk.Digest
			}
			if candidate.HunkDigest != "" && hunk.Digest != "" && candidate.HunkDigest != hunk.Digest {
				return Anchor{}, fmt.Errorf("finding anchor hunk %q digest does not match snapshot", candidate.HunkID)
			}
			if candidate.OriginalHunk == "" && len(hunk.Lines) > 0 {
				candidate.OriginalHunk = strings.Join(hunk.Lines, "")
			}
			if candidate.ContextDigest == "" {
				candidate.ContextDigest = digestText(contextLines(hunk.Lines))
				if candidate.ContextDigest == "" {
					candidate.ContextDigest = candidate.HunkDigest
				}
			}
			if candidate.BlobDigest == "" {
				if candidate.Side == snapshot.TreeSideBase || candidate.Side == snapshot.TreeSideHead {
					candidate.BlobDigest = file.BaseDigest
				} else {
					candidate.BlobDigest = file.TargetDigest
				}
				if candidate.BlobDigest == "" {
					candidate.BlobDigest = candidate.HunkDigest
				}
			}
			if err := validateFindingAnchor(candidate); err != nil {
				return Anchor{}, err
			}
			return candidate, nil
		}
	}
	return Anchor{}, fmt.Errorf("finding anchor references unknown hunk %q", candidate.HunkID)
}

func contextLines(lines []string) string {
	var builder strings.Builder
	for _, line := range lines {
		if strings.HasPrefix(line, " ") {
			builder.WriteString(line)
		}
	}
	return builder.String()
}

func validateFindingAnchor(anchor Anchor) error {
	if anchor.SnapshotID == "" || anchor.Side == "" || anchor.Layer == "" {
		return errors.New("finding anchor snapshot, side, and layer are required")
	}
	if err := snapshot.ValidateRepositoryPath(anchor.Path); err != nil {
		return fmt.Errorf("finding anchor: %w", err)
	}
	if anchor.HunkID == "" {
		return errors.New("finding anchor hunk ID is required")
	}
	if anchor.BlobDigest == "" && anchor.HunkDigest == "" && anchor.ContextDigest == "" && anchor.OriginalHunk == "" {
		return errors.New("finding anchor needs content identity beyond line numbers")
	}
	return nil
}

func normalizeFindingEvidence(change ChangeModel, evidence Evidence) (Evidence, error) {
	if !evidence.Relation.Valid() {
		return Evidence{}, fmt.Errorf("finding evidence relation %q is unsupported", evidence.Relation)
	}
	if strings.TrimSpace(evidence.Summary) == "" {
		return Evidence{}, errors.New("finding evidence summary is required")
	}
	if evidence.SnapshotID == "" {
		evidence.SnapshotID = change.SnapshotID
	}
	if evidence.SnapshotID != change.SnapshotID {
		return Evidence{}, errors.New("finding evidence belongs to another snapshot")
	}
	if len(evidence.Anchors) == 0 {
		return Evidence{}, errors.New("finding evidence needs snapshot anchors")
	}
	for index := range evidence.Anchors {
		anchor, err := NormalizeFindingAnchor(change, evidence.Anchors[index])
		if err != nil {
			return Evidence{}, err
		}
		evidence.Anchors[index] = anchor
	}
	return evidence, nil
}

func normalizeFindingRevision(record FindingRevision, requireIdentity bool) (FindingRevision, error) {
	if record.SchemaVersion == "" {
		record.SchemaVersion = FindingSchemaVersion
	}
	if record.SchemaVersion != FindingSchemaVersion {
		return FindingRevision{}, fmt.Errorf("finding revision schema %q is unsupported", record.SchemaVersion)
	}
	if requireIdentity && (strings.TrimSpace(record.FindingID) == "" || record.Revision < 1) {
		return FindingRevision{}, errors.New("finding revision identity is required")
	}
	if strings.TrimSpace(record.SessionID) == "" || strings.TrimSpace(record.RoundID) == "" || strings.TrimSpace(record.SnapshotID) == "" {
		return FindingRevision{}, errors.New("finding revision session, round, and snapshot are required")
	}
	record.Claim = strings.TrimSpace(record.Claim)
	record.Invariant = strings.TrimSpace(record.Invariant)
	record.Impact = strings.TrimSpace(record.Impact)
	record.Category = strings.TrimSpace(record.Category)
	record.Severity = strings.ToLower(strings.TrimSpace(record.Severity))
	if record.Claim == "" || record.Impact == "" || record.Category == "" {
		return FindingRevision{}, errors.New("finding revision claim, impact, and category are required")
	}
	switch record.Severity {
	case "low", "medium", "high", "critical":
	default:
		return FindingRevision{}, fmt.Errorf("finding revision severity %q is unsupported", record.Severity)
	}
	if math.IsNaN(record.Confidence) || math.IsInf(record.Confidence, 0) || record.Confidence < 0 || record.Confidence > 1 {
		return FindingRevision{}, errors.New("finding revision confidence must be between 0 and 1")
	}
	if !record.Verification.Valid() {
		return FindingRevision{}, fmt.Errorf("finding revision verification state %q is unsupported", record.Verification)
	}
	if len(record.Anchors) == 0 {
		return FindingRevision{}, errors.New("finding revision needs at least one anchor")
	}
	for index := range record.Anchors {
		if err := validateFindingAnchor(record.Anchors[index]); err != nil {
			return FindingRevision{}, err
		}
	}
	for index := range record.Evidence {
		if record.Evidence[index].SnapshotID != record.SnapshotID {
			return FindingRevision{}, errors.New("finding evidence snapshot does not match revision")
		}
		if !record.Evidence[index].Relation.Valid() || strings.TrimSpace(record.Evidence[index].Summary) == "" {
			return FindingRevision{}, errors.New("finding evidence is incomplete")
		}
	}
	if record.CreatedAt.IsZero() {
		return FindingRevision{}, errors.New("finding revision created time is required")
	}
	if record.Digest == "" {
		record.Digest = FindingRevisionDigest(record)
	} else if record.Digest != FindingRevisionDigest(record) {
		return FindingRevision{}, errors.New("finding revision digest does not match record")
	}
	return record, nil
}

// ValidateFindingRevision verifies the immutable finding contract, including
// the rule that line numbers cannot be the only anchor identity material.
func ValidateFindingRevision(record FindingRevision) error {
	_, err := normalizeFindingRevision(record, true)
	return err
}

// FindingRevisionDigest returns the canonical digest of a finding revision.
func FindingRevisionDigest(record FindingRevision) string {
	record.Digest = ""
	data, err := json.Marshal(record)
	if err != nil {
		return ""
	}
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

// CanonicalFindingRevisionJSON returns the stable JSON representation of a
// finding revision.
func CanonicalFindingRevisionJSON(record FindingRevision) ([]byte, error) {
	return json.Marshal(record)
}

// FindingDisposition is the human decision axis, independent of machine
// verification and presentation lane.
type FindingDisposition string

const (
	FindingDispositionOpen         FindingDisposition = "open"
	FindingDispositionAccepted     FindingDisposition = "accepted"
	FindingDispositionIntentional  FindingDisposition = "intentional"
	FindingDispositionDismissed    FindingDisposition = "dismissed"
	FindingDispositionDeferred     FindingDisposition = "deferred"
	FindingDispositionResolved     FindingDisposition = "resolved"
	FindingDispositionAcceptedRisk FindingDisposition = "accepted_risk"
)

// Disposition constants mirror the explicit human decision vocabulary.
const (
	DispositionOpen         = FindingDispositionOpen
	DispositionAccepted     = FindingDispositionAccepted
	DispositionIntentional  = FindingDispositionIntentional
	DispositionDismissed    = FindingDispositionDismissed
	DispositionDeferred     = FindingDispositionDeferred
	DispositionResolved     = FindingDispositionResolved
	DispositionAcceptedRisk = FindingDispositionAcceptedRisk
)

// Valid reports whether a disposition is supported by V1.
func (disposition FindingDisposition) Valid() bool {
	switch disposition {
	case FindingDispositionOpen, FindingDispositionAccepted, FindingDispositionIntentional,
		FindingDispositionDismissed, FindingDispositionDeferred, FindingDispositionResolved,
		FindingDispositionAcceptedRisk:
		return true
	default:
		return false
	}
}

// RequiresRationale reports whether a human disposition must explain its
// decision in the durable ledger.
func (disposition FindingDisposition) RequiresRationale() bool {
	switch disposition {
	case FindingDispositionIntentional, FindingDispositionDismissed, FindingDispositionDeferred,
		FindingDispositionResolved, FindingDispositionAcceptedRisk:
		return true
	default:
		return false
	}
}

// DispositionRecord is one append-only human decision event.
type DispositionRecord struct {
	ID          string             `json:"id"`
	FindingID   string             `json:"finding_id"`
	Revision    int                `json:"revision"`
	SessionID   string             `json:"session_id"`
	RoundID     string             `json:"round_id"`
	Disposition FindingDisposition `json:"disposition"`
	Rationale   string             `json:"rationale,omitempty"`
	CreatedAt   time.Time          `json:"created_at"`
	Digest      string             `json:"digest"`
}

// Validate checks a disposition event before it crosses a persistence
// boundary.
func ValidateDisposition(record DispositionRecord) error {
	if strings.TrimSpace(record.FindingID) == "" || record.Revision < 1 {
		return errors.New("disposition finding identity is required")
	}
	if !record.Disposition.Valid() {
		return fmt.Errorf("disposition %q is unsupported", record.Disposition)
	}
	if record.Disposition.RequiresRationale() && strings.TrimSpace(record.Rationale) == "" {
		return fmt.Errorf("disposition %q requires a rationale", record.Disposition)
	}
	if record.CreatedAt.IsZero() {
		return errors.New("disposition created time is required")
	}
	if record.Digest != "" && record.Digest != DispositionDigest(record) {
		return errors.New("disposition digest does not match record")
	}
	return nil
}

// DispositionDigest returns the canonical digest of an append-only decision.
func DispositionDigest(record DispositionRecord) string {
	record.Digest = ""
	data, err := json.Marshal(record)
	if err != nil {
		return ""
	}
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

// PresentationRecord is immutable publishable wording for one finding
// revision. Editing wording creates another version instead of changing the
// finding revision, evidence, or verification history.
type PresentationRecord struct {
	SchemaVersion   string    `json:"schema_version"`
	ID              string    `json:"id"`
	FindingID       string    `json:"finding_id"`
	FindingRevision int       `json:"finding_revision"`
	Version         int       `json:"version"`
	Body            string    `json:"body"`
	Comment         string    `json:"comment,omitempty"`
	Wording         string    `json:"wording,omitempty"`
	CreatedAt       time.Time `json:"created_at"`
	Digest          string    `json:"digest"`
}

// CommentRevision is the domain name used by comment-edit APIs.
type CommentRevision = PresentationRecord

// FindingPresentation is an alternate readable name for a presentation
// record.
type FindingPresentation = PresentationRecord

// Validate checks one immutable presentation version.
func ValidatePresentation(record PresentationRecord) error {
	if record.SchemaVersion == "" {
		record.SchemaVersion = FindingPresentationSchemaVersion
	}
	if record.SchemaVersion != FindingPresentationSchemaVersion {
		return fmt.Errorf("presentation schema %q is unsupported", record.SchemaVersion)
	}
	if strings.TrimSpace(record.FindingID) == "" || record.FindingRevision < 1 || record.Version < 1 {
		return errors.New("presentation finding identity and version are required")
	}
	if strings.TrimSpace(record.Body) == "" {
		record.Body = strings.TrimSpace(record.Comment)
	}
	if strings.TrimSpace(record.Body) == "" {
		record.Body = strings.TrimSpace(record.Wording)
	}
	if strings.TrimSpace(record.Body) == "" {
		return errors.New("presentation body is required")
	}
	if record.CreatedAt.IsZero() {
		return errors.New("presentation created time is required")
	}
	if record.Digest != "" && record.Digest != PresentationDigest(record) {
		return errors.New("presentation digest does not match record")
	}
	return nil
}

// PresentationDigest returns the canonical digest of a presentation record.
func PresentationDigest(record PresentationRecord) string {
	if record.SchemaVersion == "" {
		record.SchemaVersion = FindingPresentationSchemaVersion
	}
	if strings.TrimSpace(record.Body) == "" {
		record.Body = strings.TrimSpace(record.Comment)
	}
	if strings.TrimSpace(record.Body) == "" {
		record.Body = strings.TrimSpace(record.Wording)
	}
	record.Digest = ""
	data, err := json.Marshal(record)
	if err != nil {
		return ""
	}
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

// FindingStore persists immutable findings and their human presentation axes.
type FindingStore interface {
	SaveFindingRevision(context.Context, FindingRevision) error
	GetFindingRevision(context.Context, string, int) (FindingRevision, error)
	ListFindingRevisions(context.Context, string) ([]FindingRevision, error)
	ListFindingRevisionsForFinding(context.Context, string) ([]FindingRevision, error)
	GetLatestFindingRevision(context.Context, string) (FindingRevision, error)
	SaveDisposition(context.Context, DispositionRecord) error
	ListDispositions(context.Context, string) ([]DispositionRecord, error)
	GetCurrentDisposition(context.Context, string) (DispositionRecord, error)
	SavePresentation(context.Context, PresentationRecord) error
	ListPresentations(context.Context, string) ([]PresentationRecord, error)
	GetLatestPresentation(context.Context, string) (PresentationRecord, error)
}

// FindingMatchKind describes the outcome of identity correlation.
type FindingMatchKind string

const (
	// FindingMatchNew means no prior revision had a strong identity match.
	FindingMatchNew FindingMatchKind = "new"
	// FindingMatchStable means one unambiguous prior finding retained identity.
	FindingMatchStable FindingMatchKind = "stable"
	// FindingMatchAmbiguous means continuity was possible but unsafe to choose.
	FindingMatchAmbiguous FindingMatchKind = "ambiguous"
)

// FindingMatch reports correlation without mutating either input revision.
type FindingMatch struct {
	Kind       FindingMatchKind
	FindingID  string
	Revision   int
	Candidates []FindingRelationship
}

// CorrelateFinding correlates one new revision against prior revisions. It
// preserves an ID only for an exact claim/invariant and strong anchor match.
// Line movement is ignored; ambiguous candidates receive explicit links.
func CorrelateFinding(previous []FindingRevision, next FindingRevision) (FindingRevision, error) {
	normalized, err := normalizeFindingRevision(next, false)
	if err != nil {
		return FindingRevision{}, err
	}
	matches := rankedMatches(previous, normalized)
	if len(matches) == 0 {
		return finalizeCorrelatedRevision(normalized, FindingMatchNew, nil)
	}
	topScore := matches[0].score
	top := matches[:0]
	for _, match := range matches {
		if match.score == topScore {
			top = append(top, match)
		}
	}
	if len(top) == 1 {
		prior := top[0].revision
		normalized.FindingID = prior.FindingID
		normalized.Revision = prior.Revision + 1
		normalized.Relationships = append(normalized.Relationships, FindingRelationship{
			Kind: FindingRelationshipPredecessor, FindingID: prior.FindingID, Revision: prior.Revision,
			Reason: "claim and snapshot anchor fingerprints matched strongly.",
		})
		return finalizeCorrelatedRevision(normalized, FindingMatchStable, nil)
	}
	relationships := relationshipsForMatches(top, FindingRelationshipPossibleSuccessor, "multiple prior findings matched with equal identity strength.")
	return finalizeCorrelatedRevision(normalized, FindingMatchAmbiguous, relationships)
}

// CorrelateFindings correlates a complete round at once. It additionally
// prevents two new outputs from reusing one prior ID, linking those outputs as
// duplicates when the prior match is ambiguous.
func CorrelateFindings(previous, next []FindingRevision) ([]FindingRevision, error) {
	result := make([]FindingRevision, len(next))
	matches := make([][]rankedFindingMatch, len(next))
	for index, value := range next {
		normalized, err := normalizeFindingRevision(value, false)
		if err != nil {
			return nil, fmt.Errorf("correlate finding %d: %w", index, err)
		}
		result[index] = normalized
		matches[index] = rankedMatches(previous, normalized)
	}

	priorUsers := make(map[int][]int)
	for currentIndex, candidates := range matches {
		if len(candidates) == 0 {
			continue
		}
		topScore := candidates[0].score
		for _, candidate := range candidates {
			if candidate.score != topScore {
				break
			}
			priorUsers[candidate.index] = append(priorUsers[candidate.index], currentIndex)
		}
	}
	for currentIndex, candidates := range matches {
		kind := FindingMatchNew
		var top []rankedFindingMatch
		if len(candidates) > 0 {
			topScore := candidates[0].score
			for _, candidate := range candidates {
				if candidate.score != topScore {
					break
				}
				top = append(top, candidate)
			}
		}
		if len(top) == 1 && len(priorUsers[top[0].index]) == 1 {
			prior := top[0].revision
			result[currentIndex].FindingID = prior.FindingID
			result[currentIndex].Revision = prior.Revision + 1
			result[currentIndex].Relationships = append(result[currentIndex].Relationships, FindingRelationship{
				Kind: FindingRelationshipPredecessor, FindingID: prior.FindingID, Revision: prior.Revision,
				Reason: "claim and snapshot anchor fingerprints matched strongly.",
			})
			kind = FindingMatchStable
		} else if len(top) > 0 {
			result[currentIndex].Relationships = append(result[currentIndex].Relationships,
				relationshipsForMatches(top, FindingRelationshipPossibleSuccessor, "continuity was ambiguous across review revisions.")...)
			kind = FindingMatchAmbiguous
		}
		finalized, err := finalizeCorrelatedRevision(result[currentIndex], kind, nil)
		if err != nil {
			return nil, fmt.Errorf("correlate finding %d: %w", currentIndex, err)
		}
		result[currentIndex] = finalized
	}
	for priorIndex, users := range priorUsers {
		if len(users) < 2 {
			continue
		}
		for _, currentIndex := range users {
			for _, otherIndex := range users {
				if currentIndex == otherIndex {
					continue
				}
				prior := previous[priorIndex]
				result[currentIndex].Relationships = append(result[currentIndex].Relationships, FindingRelationship{
					Kind: FindingRelationshipDuplicate, FindingID: result[otherIndex].FindingID,
					Revision: result[otherIndex].Revision, Reason: fmt.Sprintf("both outputs ambiguously matched prior finding %q.", prior.FindingID),
				})
			}
		}
	}
	for index := range result {
		sortFindingRelationships(result[index].Relationships)
		result[index].Digest = FindingRevisionDigest(result[index])
	}
	return result, nil
}

type rankedFindingMatch struct {
	index    int
	score    int
	revision FindingRevision
}

func rankedMatches(previous []FindingRevision, next FindingRevision) []rankedFindingMatch {
	matches := make([]rankedFindingMatch, 0)
	for index, prior := range previous {
		score, ok := findingMatchScore(prior, next)
		if ok {
			matches = append(matches, rankedFindingMatch{index: index, score: score, revision: prior})
		}
	}
	sort.SliceStable(matches, func(i, j int) bool {
		if matches[i].score != matches[j].score {
			return matches[i].score > matches[j].score
		}
		if matches[i].revision.FindingID != matches[j].revision.FindingID {
			return matches[i].revision.FindingID < matches[j].revision.FindingID
		}
		return matches[i].revision.Revision < matches[j].revision.Revision
	})
	return matches
}

func findingMatchScore(previous, next FindingRevision) (int, bool) {
	if canonicalClaim(previous.Claim) != canonicalClaim(next.Claim) {
		return 0, false
	}
	if effectiveInvariant(previous) != effectiveInvariant(next) {
		return 0, false
	}
	anchorScore := 0
	for _, nextAnchor := range next.Anchors {
		for _, previousAnchor := range previous.Anchors {
			anchorScore = max(anchorScore, anchorMatchScore(previousAnchor, nextAnchor))
		}
	}
	if anchorScore < 10 {
		return 0, false
	}
	return 100 + anchorScore, true
}

func canonicalClaim(value string) string {
	return strings.Join(strings.Fields(strings.ToLower(strings.TrimSpace(value))), " ")
}

func effectiveInvariant(record FindingRevision) string {
	if strings.TrimSpace(record.Invariant) == "" {
		return canonicalClaim(record.Claim)
	}
	return canonicalClaim(record.Invariant)
}

func anchorMatchScore(previous, next Anchor) int {
	score := 0
	if previous.HunkDigest != "" && previous.HunkDigest == next.HunkDigest {
		score += 10
	}
	if previous.OriginalHunk != "" && previous.OriginalHunk == next.OriginalHunk {
		score += 8
	}
	if previous.ContextDigest != "" && previous.ContextDigest == next.ContextDigest {
		score += 8
	}
	if previous.BlobDigest != "" && previous.BlobDigest == next.BlobDigest {
		score += 6
	}
	if previous.Symbol != "" && previous.Symbol == next.Symbol {
		score += 3
	}
	if previous.SyntaxFingerprint != "" && previous.SyntaxFingerprint == next.SyntaxFingerprint {
		score += 3
	}
	if previous.Side != "" && previous.Side == next.Side {
		score += 2
	}
	if previous.Layer != "" && previous.Layer == next.Layer {
		score++
	}
	if previous.Path != "" && previous.Path == next.Path {
		score++
	}
	return score
}

func relationshipsForMatches(matches []rankedFindingMatch, kind FindingRelationshipKind, reason string) []FindingRelationship {
	relationships := make([]FindingRelationship, 0, len(matches))
	for _, match := range matches {
		relationships = append(relationships, FindingRelationship{Kind: kind, FindingID: match.revision.FindingID, Revision: match.revision.Revision, Reason: reason})
	}
	return relationships
}

func finalizeCorrelatedRevision(record FindingRevision, kind FindingMatchKind, relationships []FindingRelationship) (FindingRevision, error) {
	if len(relationships) > 0 {
		record.Relationships = append(record.Relationships, relationships...)
	}
	if record.FindingID == "" {
		id, err := newFindingID()
		if err != nil {
			return FindingRevision{}, fmt.Errorf("create finding ID: %w", err)
		}
		record.FindingID = id
	}
	if record.Revision < 1 {
		record.Revision = 1
	}
	sortFindingRelationships(record.Relationships)
	record.Digest = FindingRevisionDigest(record)
	if err := ValidateFindingRevision(record); err != nil {
		return FindingRevision{}, fmt.Errorf("finalize %s finding: %w", kind, err)
	}
	return record, nil
}

func sortFindingRelationships(relationships []FindingRelationship) {
	sort.Slice(relationships, func(i, j int) bool {
		if relationships[i].Kind != relationships[j].Kind {
			return relationships[i].Kind < relationships[j].Kind
		}
		if relationships[i].FindingID != relationships[j].FindingID {
			return relationships[i].FindingID < relationships[j].FindingID
		}
		return relationships[i].Revision < relationships[j].Revision
	})
}

func newFindingID() (string, error) {
	var bytes [16]byte
	if _, err := rand.Read(bytes[:]); err != nil {
		return "", err
	}
	return "finding-" + hex.EncodeToString(bytes[:]), nil
}

func digestText(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	digest := sha256.Sum256([]byte(value))
	return hex.EncodeToString(digest[:])
}

func max(left, right int) int {
	if left > right {
		return left
	}
	return right
}
