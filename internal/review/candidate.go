package review

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/snapshot"
)

// Anchor binds a candidate to an exact hunk in the frozen snapshot.
// A hunk ID, rather than a line number, is the identity-bearing field.
type Anchor struct {
	SnapshotID        string `json:"snapshot_id"`
	Side              string `json:"side"`
	Layer             string `json:"layer,omitempty"`
	Path              string `json:"path"`
	BlobDigest        string `json:"blob_digest,omitempty"`
	StartLine         int    `json:"start_line,omitempty"`
	EndLine           int    `json:"end_line,omitempty"`
	OriginalHunk      string `json:"original_hunk,omitempty"`
	HunkID            string `json:"hunk_id"`
	HunkDigest        string `json:"hunk_digest,omitempty"`
	ContextDigest     string `json:"context_digest,omitempty"`
	Symbol            string `json:"symbol,omitempty"`
	SyntaxFingerprint string `json:"syntax_fingerprint,omitempty"`
}

// Candidate is one schema-valid, plausible output emitted by a specialized
// reviewer. It is intentionally not a verified finding; later milestones add
// evidence and adversarial verification.
type Candidate struct {
	SourceID   string   `json:"source_id,omitempty"`
	Claim      string   `json:"claim"`
	Impact     string   `json:"impact"`
	Category   string   `json:"category"`
	Severity   string   `json:"severity"`
	Confidence float64  `json:"confidence,omitempty"`
	Anchors    []Anchor `json:"anchors"`
	Rationale  string   `json:"rationale,omitempty"`
}

func (c Candidate) normalize(change ChangeModel) (Candidate, error) {
	c.Claim = strings.TrimSpace(c.Claim)
	c.Impact = strings.TrimSpace(c.Impact)
	c.Category = strings.TrimSpace(c.Category)
	c.Severity = strings.ToLower(strings.TrimSpace(c.Severity))
	if c.Claim == "" || c.Impact == "" || c.Category == "" {
		return Candidate{}, errors.New("candidate claim, impact, and category are required")
	}
	switch c.Severity {
	case "low", "medium", "high", "critical":
	default:
		return Candidate{}, fmt.Errorf("candidate severity %q is unsupported", c.Severity)
	}
	if math.IsNaN(c.Confidence) || math.IsInf(c.Confidence, 0) || c.Confidence < 0 || c.Confidence > 1 {
		return Candidate{}, errors.New("candidate confidence must be between 0 and 1")
	}
	hunks := make(map[string]Hunk)
	paths := make(map[string]bool)
	for _, file := range change.Files {
		pathName := file.TargetPath
		if pathName == "" {
			pathName = file.BasePath
		}
		paths[pathName] = true
		for _, hunk := range file.Hunks {
			hunks[hunk.ID] = hunk
		}
	}
	if len(c.Anchors) == 0 {
		return Candidate{}, errors.New("candidate needs at least one snapshot hunk anchor")
	}
	for index := range c.Anchors {
		anchor := &c.Anchors[index]
		if anchor.SnapshotID == "" {
			anchor.SnapshotID = change.SnapshotID
		}
		if anchor.SnapshotID != change.SnapshotID {
			return Candidate{}, errors.New("candidate anchor belongs to another snapshot")
		}
		if anchor.Side == "" {
			anchor.Side = snapshot.TreeSideTarget
		}
		if anchor.Side != snapshot.TreeSideBase && anchor.Side != snapshot.TreeSideTarget && anchor.Side != snapshot.TreeSideHead && anchor.Side != snapshot.TreeSideIndex && anchor.Side != snapshot.TreeSideWorktree {
			return Candidate{}, fmt.Errorf("candidate anchor has unsupported side %q", anchor.Side)
		}
		hunk, ok := hunks[anchor.HunkID]
		if !ok {
			return Candidate{}, fmt.Errorf("candidate anchor references unknown hunk %q", anchor.HunkID)
		}
		if anchor.Path == "" {
			for _, file := range change.Files {
				for _, candidateHunk := range file.Hunks {
					if candidateHunk.ID == anchor.HunkID {
						anchor.Path = file.TargetPath
						if anchor.Path == "" {
							anchor.Path = file.BasePath
						}
					}
				}
			}
		}
		if !paths[anchor.Path] {
			return Candidate{}, fmt.Errorf("candidate anchor path %q is not changed", anchor.Path)
		}
		if anchor.HunkDigest == "" {
			anchor.HunkDigest = hunk.Digest
		}
		if hunk.Digest != "" && anchor.HunkDigest != hunk.Digest {
			return Candidate{}, fmt.Errorf("candidate anchor hunk %q digest does not match snapshot", anchor.HunkID)
		}
	}
	return c, nil
}

func (c Candidate) fingerprint() (string, error) {
	data, err := json.Marshal(c)
	if err != nil {
		return "", fmt.Errorf("encode candidate fingerprint: %w", err)
	}
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:]), nil
}

// CandidateEnvelope is the only structured reviewer payload accepted.
type CandidateEnvelope struct {
	SchemaVersion string      `json:"schema_version"`
	Candidates    []Candidate `json:"candidates"`
}

// CandidateRecord is an immutable retained emission. Each model emission gets
// its own ID, including duplicate or equivalent candidates.
type CandidateRecord struct {
	ID          string    `json:"id"`
	RunID       string    `json:"run_id"`
	PassName    string    `json:"pass_name"`
	Ordinal     int       `json:"ordinal"`
	Fingerprint string    `json:"fingerprint"`
	Candidate   Candidate `json:"candidate"`
	CreatedAt   time.Time `json:"created_at"`
}

func decodeCandidates(data []byte) (*CandidateEnvelope, error) {
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	var envelope CandidateEnvelope
	if err := decoder.Decode(&envelope); err != nil {
		return nil, fmt.Errorf("decode review candidates: %w", err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != nil {
		if !errors.Is(err, io.EOF) {
			return nil, errors.New("decode review candidates: trailing JSON")
		}
	} else {
		return nil, errors.New("decode review candidates: trailing JSON")
	}
	if envelope.SchemaVersion != ReviewCandidateSchemaVersion {
		return nil, fmt.Errorf("decode review candidates: unsupported schema %q", envelope.SchemaVersion)
	}
	if envelope.Candidates == nil {
		return nil, errors.New("decode review candidates: candidates field is required")
	}
	return &envelope, nil
}
