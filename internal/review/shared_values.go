package review

import (
	"errors"
	"fmt"
	"math"
	"strings"
)

// ReviewScope identifies the session, round, and frozen snapshot shared by a
// review record. All three values are required when this value is embedded.
type ReviewScope struct {
	SessionID  string `json:"session_id"`
	RoundID    string `json:"round_id"`
	SnapshotID string `json:"snapshot_id"`
}

// EvidenceLocation contains the snapshot-bound location and retained artifact
// metadata shared by verifier evidence and concrete verification steps.
type EvidenceLocation struct {
	Kind           string   `json:"kind,omitempty"`
	Summary        string   `json:"summary"`
	SnapshotID     string   `json:"snapshot_id"`
	Anchors        []Anchor `json:"anchors"`
	ArtifactDigest string   `json:"artifact_digest"`
	OutputPointer  string   `json:"output_pointer,omitempty"`
}

// CandidateContent contains the claim and supporting content proposed by a
// reviewer or contextual chat. It carries no source identity or lifecycle
// state, so a chat proposal remains a proposal until explicitly promoted.
type CandidateContent struct {
	Claim      string   `json:"claim"`
	Impact     string   `json:"impact"`
	Category   string   `json:"category"`
	Severity   string   `json:"severity"`
	Confidence float64  `json:"confidence,omitempty"`
	Anchors    []Anchor `json:"anchors"`
	Rationale  string   `json:"rationale,omitempty"`
}

func (content CandidateContent) validate(label string) error {
	if strings.TrimSpace(content.Claim) == "" || strings.TrimSpace(content.Impact) == "" ||
		strings.TrimSpace(content.Category) == "" {
		return fmt.Errorf("%s claim, impact, and category are required", label)
	}
	switch strings.ToLower(strings.TrimSpace(content.Severity)) {
	case "low", "medium", "high", "critical":
	default:
		return fmt.Errorf("%s severity %q is unsupported", label, content.Severity)
	}
	if math.IsNaN(content.Confidence) || math.IsInf(content.Confidence, 0) ||
		content.Confidence < 0 || content.Confidence > 1 {
		return errors.New(label + " confidence must be between 0 and 1")
	}
	if len(content.Anchors) == 0 {
		return errors.New(label + " needs at least one anchor")
	}
	return nil
}
