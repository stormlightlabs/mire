// Package review assembles deterministic, snapshot-bound review inputs.
package review

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/stormlightlabs/mire/internal/snapshot"
)

// ContentReader reads one already-captured object by its digest. It must not
// consult the reviewed repository or a live Git object database.
type ContentReader func(context.Context, string) ([]byte, error)

// PolicyTier is the precedence tier of a review rule. Lower values have
// higher authority.
type PolicyTier int

const (
	PolicyTierBuiltIn PolicyTier = iota
	PolicyTierPrivate
	PolicyTierBasePolicy
	PolicyTierBaseDocumentation
	PolicyTierTargetEvidence
)

// String returns the stable serialized name of a policy tier.
func (tier PolicyTier) String() string {
	switch tier {
	case PolicyTierBuiltIn:
		return "built_in"
	case PolicyTierPrivate:
		return "private"
	case PolicyTierBasePolicy:
		return "base_policy"
	case PolicyTierBaseDocumentation:
		return "base_documentation"
	case PolicyTierTargetEvidence:
		return "target_evidence"
	default:
		return "unknown"
	}
}

// GuidanceKind identifies how a captured text artifact participates in review.
type GuidanceKind string

const (
	GuidancePolicy         GuidanceKind = "policy"
	GuidanceAgent          GuidanceKind = "agents"
	GuidanceContribution   GuidanceKind = "contribution"
	GuidanceArchitecture   GuidanceKind = "architecture"
	GuidanceDocumentation  GuidanceKind = "documentation"
	GuidanceTargetPolicy   GuidanceKind = "target_policy"
	GuidanceTargetDocument GuidanceKind = "target_documentation"
)

// PolicyRule is a normalized rule from private configuration or captured
// guidance. Scope is empty for a general rule, or a repository-relative path
// or path.Match pattern for a path-specific rule.
type PolicyRule struct {
	Key    string     `json:"key"`
	Value  string     `json:"value"`
	Scope  string     `json:"scope,omitempty"`
	Tier   PolicyTier `json:"tier"`
	Source string     `json:"source"`
}

// Guidance is immutable text supplied as review context. Digest is verified
// when present and otherwise derived from all provenance fields and content.
type Guidance struct {
	ID      string       `json:"id"`
	Path    string       `json:"path"`
	Kind    GuidanceKind `json:"kind"`
	Tier    PolicyTier   `json:"tier"`
	Scope   string       `json:"scope,omitempty"`
	Content string       `json:"content"`
	Rules   []PolicyRule `json:"rules,omitempty"`
	Digest  string       `json:"digest"`
}

// ReviewRequest contains user-owned review intent and private configuration.
type ReviewRequest struct {
	Prompt        string       `json:"prompt,omitempty"`
	Configuration string       `json:"configuration,omitempty"`
	Rules         []PolicyRule `json:"rules,omitempty"`
}

// PinnedCommit is commit metadata captured before model work. It is not a
// request to resolve an object ID later.
type PinnedCommit struct {
	OID       string   `json:"oid"`
	Parents   []string `json:"parents,omitempty"`
	Message   string   `json:"message"`
	Author    string   `json:"author,omitempty"`
	Committer string   `json:"committer,omitempty"`
	Digest    string   `json:"digest"`
}

// PinnedGitQuery records an exact, already-completed metadata query.
type PinnedGitQuery struct {
	Query  string `json:"query"`
	Output string `json:"output"`
	Digest string `json:"digest"`
}

// PinnedGit is the immutable Git provenance available to a review model.
type PinnedGit struct {
	ObjectFormat     string           `json:"object_format"`
	BaseOID          string           `json:"base_oid"`
	EffectiveBaseOID string           `json:"effective_base_oid"`
	TargetOID        string           `json:"target_oid"`
	MergeBaseOID     string           `json:"merge_base_oid,omitempty"`
	Commits          []PinnedCommit   `json:"commits,omitempty"`
	Queries          []PinnedGitQuery `json:"queries,omitempty"`
	Digest           string           `json:"digest"`
}

// EarlierRound contains only the prior round's immutable intent. Its session
// identity is checked before it can influence a new model.
type EarlierRound struct {
	SessionID      string `json:"session_id"`
	RoundID        string `json:"round_id"`
	SnapshotDigest string `json:"snapshot_digest"`
	Intent         string `json:"intent"`
	Digest         string `json:"digest"`
}

// Input is the complete immutable boundary for model assembly.
type Input struct {
	SessionID      string
	SnapshotID     string
	Snapshot       snapshot.Capture
	Content        ContentReader
	Request        ReviewRequest
	Git            PinnedGit
	Guidance       []Guidance
	EarlierRound   *EarlierRound
	NoBaseRevision bool
}

// ContextArtifact is a digest-recorded piece of model context.
type ContextArtifact struct {
	ID      string     `json:"id"`
	Kind    string     `json:"kind"`
	Source  string     `json:"source"`
	Path    string     `json:"path,omitempty"`
	Tier    PolicyTier `json:"tier"`
	Content string     `json:"content"`
	Digest  string     `json:"digest"`
}

// Intent is the deterministic intent portion of a change model.
type Intent struct {
	Prompt         string            `json:"prompt,omitempty"`
	CommitMessages []PinnedCommit    `json:"commit_messages,omitempty"`
	BaseGuidance   []ContextArtifact `json:"base_guidance,omitempty"`
	EarlierRound   *EarlierRound     `json:"earlier_round,omitempty"`
	Digest         string            `json:"digest"`
}

// SurfaceKind identifies a review-relevant affected surface.
type SurfaceKind string

const (
	SurfaceTests         SurfaceKind = "tests"
	SurfaceContracts     SurfaceKind = "contracts"
	SurfaceConfiguration SurfaceKind = "configuration"
	SurfaceDependencies  SurfaceKind = "dependencies"
	SurfaceMigrations    SurfaceKind = "migrations"
	SurfacePublicAPI     SurfaceKind = "public_api"
)

// Symbol is a lexical symbol found in changed snapshot content.
type Symbol struct {
	Path string `json:"path"`
	Kind string `json:"kind"`
	Name string `json:"name"`
}

// Hunk is a deterministic, snapshot-bound diff hunk.
type Hunk struct {
	ID        string   `json:"id"`
	Kind      string   `json:"kind"`
	OldStart  int      `json:"old_start"`
	OldLines  int      `json:"old_lines"`
	NewStart  int      `json:"new_start"`
	NewLines  int      `json:"new_lines"`
	Lines     []string `json:"lines,omitempty"`
	Binary    bool     `json:"binary,omitempty"`
	Available bool     `json:"available"`
	Digest    string   `json:"digest"`
}

// FileChange is one changed path pair and its inventory of hunks and symbols.
type FileChange struct {
	Status       string        `json:"status"`
	BasePath     string        `json:"base_path,omitempty"`
	TargetPath   string        `json:"target_path,omitempty"`
	BaseDigest   string        `json:"base_digest,omitempty"`
	TargetDigest string        `json:"target_digest,omitempty"`
	Hunks        []Hunk        `json:"hunks,omitempty"`
	Symbols      []Symbol      `json:"symbols,omitempty"`
	Surfaces     []SurfaceKind `json:"surfaces,omitempty"`
	Patch        string        `json:"patch,omitempty"`
}

// SurfaceEvidence explains why an affected surface is present.
type SurfaceEvidence struct {
	Kind    SurfaceKind `json:"kind"`
	Path    string      `json:"path"`
	HunkIDs []string    `json:"hunk_ids,omitempty"`
	Reason  string      `json:"reason"`
}

// AffectedSurface groups evidence for one review-relevant surface.
type AffectedSurface struct {
	Kind     SurfaceKind       `json:"kind"`
	Evidence []SurfaceEvidence `json:"evidence"`
}

// PolicyDecision records every applicable candidate and the selected safe
// interpretation for one key and path.
type PolicyDecision struct {
	Path       string       `json:"path,omitempty"`
	Key        string       `json:"key"`
	Candidates []PolicyRule `json:"candidates"`
	Selected   PolicyRule   `json:"selected"`
}

// PolicyConflict records an unresolved same-tier disagreement. Selected is
// the deterministic safer interpretation and must remain visible to callers.
type PolicyConflict struct {
	Path     string       `json:"path,omitempty"`
	Key      string       `json:"key"`
	Tier     PolicyTier   `json:"tier"`
	Rules    []PolicyRule `json:"rules"`
	Selected string       `json:"selected"`
}

// PolicyResolution is the complete precedence audit for a model.
type PolicyResolution struct {
	Decisions               []PolicyDecision  `json:"decisions"`
	Conflicts               []PolicyConflict  `json:"conflicts,omitempty"`
	TargetEvidence          []ContextArtifact `json:"target_evidence,omitempty"`
	NoBaseRevisionException bool              `json:"no_base_revision_exception,omitempty"`
	Digest                  string            `json:"digest"`
}

// ChangeModel is the canonical provider-neutral review input for one frozen
// snapshot.
type ChangeModel struct {
	SchemaVersion  string            `json:"schema_version"`
	SessionID      string            `json:"session_id,omitempty"`
	SnapshotID     string            `json:"snapshot_id,omitempty"`
	SnapshotDigest string            `json:"snapshot_digest"`
	ComparisonKind string            `json:"comparison_kind"`
	Requested      string            `json:"requested_comparison"`
	Git            PinnedGit         `json:"git"`
	Intent         Intent            `json:"intent"`
	Files          []FileChange      `json:"files"`
	Surfaces       []AffectedSurface `json:"surfaces"`
	Policies       PolicyResolution  `json:"policies"`
	Context        []ContextArtifact `json:"context"`
	Digest         string            `json:"digest"`
}

var symbolPatterns = []*regexp.Regexp{
	regexp.MustCompile(`(?m)\bfunc\s+(?:\([^)]*\)\s*)?([A-Z][A-Za-z0-9_]*)\s*\(`),
	regexp.MustCompile(`(?m)\btype\s+([A-Z][A-Za-z0-9_]*)\b`),
	regexp.MustCompile(`(?m)\b(?:const|var)\s+([A-Z][A-Za-z0-9_]*)\b`),
	regexp.MustCompile(`(?m)\bexport\s+(?:async\s+)?(?:function|class|const|let|var)\s+([A-Za-z_$][\w$]*)`),
}

// Assemble builds a change model using only input values and captured object
// bytes. It never resolves refs, reads a repository, or executes a command.
func Assemble(ctx context.Context, input Input) (ChangeModel, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	input.Request = normalizeRequest(input.Request)
	if err := validateInput(input); err != nil {
		return ChangeModel{}, err
	}
	capture := input.Snapshot
	changes := snapshot.BuildChanges(capture.BaseEntries, capture.TargetEntries)
	if !sameChanges(changes, capture.Changes) {
		return ChangeModel{}, fmt.Errorf("assemble review model: snapshot changes do not match manifests")
	}
	gitMetadata, err := normalizeGit(input.Git, capture)
	if err != nil {
		return ChangeModel{}, err
	}
	guidance, contextArtifacts, err := normalizeGuidance(input.Guidance)
	if err != nil {
		return ChangeModel{}, err
	}
	intent, intentArtifacts, err := assembleIntent(input, gitMetadata, guidance)
	if err != nil {
		return ChangeModel{}, err
	}
	contextArtifacts = append(contextArtifacts, intentArtifacts...)
	files, surfaces, err := assembleFiles(ctx, capture, input.Content)
	if err != nil {
		return ChangeModel{}, err
	}
	policies, _, err := resolvePolicies(input, guidance, files)
	if err != nil {
		return ChangeModel{}, err
	}
	sortContext(contextArtifacts)
	model := ChangeModel{
		SchemaVersion: "mire/v1/change-model",
		SessionID:     input.SessionID, SnapshotID: input.SnapshotID,
		SnapshotDigest: capture.ManifestDigest, ComparisonKind: capture.ComparisonKind,
		Requested: capture.RequestedComparison, Git: gitMetadata, Intent: intent,
		Files: files, Surfaces: surfaces, Policies: policies, Context: contextArtifacts,
	}
	digest, err := digestValue(modelWithoutDigest(model))
	if err != nil {
		return ChangeModel{}, fmt.Errorf("assemble review model digest: %w", err)
	}
	model.Digest = digest
	return model, nil
}

func normalizeRequest(request ReviewRequest) ReviewRequest {
	rules := append([]PolicyRule(nil), request.Rules...)
	if len(rules) == 0 && request.Configuration != "" {
		rules = ParsePolicyRules(request.Configuration, PolicyTierPrivate, "private_request")
	}
	for index := range rules {
		rules[index].Tier = PolicyTierPrivate
		if rules[index].Source == "" {
			rules[index].Source = "private_request"
		}
	}
	request.Rules = rules
	return request
}

// CanonicalJSON returns the stable JSON representation used for model input
// hashing and fixture comparisons.
func CanonicalJSON(model ChangeModel) ([]byte, error) {
	return json.Marshal(model)
}

func validateInput(input Input) error {
	if err := input.Snapshot.Validate(); err != nil {
		return fmt.Errorf("assemble review model: %w", err)
	}
	for _, manifest := range []struct {
		name     string
		entries  []snapshot.Entry
		expected string
	}{
		{name: snapshot.TreeSideBase, entries: input.Snapshot.BaseEntries, expected: input.Snapshot.BaseManifestDigest},
		{name: snapshot.TreeSideTarget, entries: input.Snapshot.TargetEntries, expected: input.Snapshot.TargetManifestDigest},
	} {
		digest, err := snapshot.ManifestDigest(manifest.entries)
		if err != nil || digest != manifest.expected {
			return fmt.Errorf("assemble review model: %s manifest digest does not match entries", manifest.name)
		}
	}
	if input.Snapshot.ComparisonKind == snapshot.ComparisonWorktree {
		for _, manifest := range []struct {
			name     string
			entries  []snapshot.Entry
			expected string
		}{
			{name: snapshot.TreeSideHead, entries: input.Snapshot.HeadEntries, expected: input.Snapshot.HeadManifestDigest},
			{name: snapshot.TreeSideIndex, entries: input.Snapshot.IndexEntries, expected: input.Snapshot.IndexManifestDigest},
			{name: snapshot.TreeSideWorktree, entries: input.Snapshot.WorktreeEntries, expected: input.Snapshot.WorktreeManifestDigest},
		} {
			digest, err := snapshot.ManifestDigest(manifest.entries)
			if err != nil || digest != manifest.expected {
				return fmt.Errorf("assemble review model: %s manifest digest does not match entries", manifest.name)
			}
		}
	}
	digest, err := snapshot.OverallManifestDigest(input.Snapshot)
	if err != nil || digest != input.Snapshot.ManifestDigest {
		return fmt.Errorf("assemble review model: snapshot manifest digest does not match immutable inputs")
	}
	if input.EarlierRound != nil && strings.TrimSpace(input.SessionID) == "" {
		return fmt.Errorf("assemble review model: session ID is required with earlier round context")
	}
	if input.EarlierRound != nil && input.EarlierRound.SessionID != input.SessionID {
		return fmt.Errorf("assemble review model: earlier round belongs to another session")
	}
	return nil
}

func normalizeGit(gitMetadata PinnedGit, capture snapshot.Capture) (PinnedGit, error) {
	if gitMetadata.ObjectFormat == "" {
		gitMetadata.ObjectFormat = capture.ObjectFormat
	}
	if gitMetadata.BaseOID == "" {
		gitMetadata.BaseOID = capture.BaseOID
	}
	if gitMetadata.EffectiveBaseOID == "" {
		gitMetadata.EffectiveBaseOID = capture.EffectiveBaseOID
	}
	if gitMetadata.TargetOID == "" {
		gitMetadata.TargetOID = capture.TargetOID
	}
	if gitMetadata.MergeBaseOID == "" {
		gitMetadata.MergeBaseOID = capture.MergeBaseOID
	}
	if gitMetadata.ObjectFormat != capture.ObjectFormat || gitMetadata.EffectiveBaseOID != capture.EffectiveBaseOID ||
		gitMetadata.TargetOID != capture.TargetOID {
		return PinnedGit{}, fmt.Errorf("assemble review model: pinned Git metadata does not match snapshot")
	}
	commits := append([]PinnedCommit(nil), gitMetadata.Commits...)
	for index := range commits {
		digest, err := digestValue(struct {
			OID       string   `json:"oid"`
			Parents   []string `json:"parents,omitempty"`
			Message   string   `json:"message"`
			Author    string   `json:"author,omitempty"`
			Committer string   `json:"committer,omitempty"`
		}{commits[index].OID, commits[index].Parents, commits[index].Message, commits[index].Author, commits[index].Committer})
		if err != nil {
			return PinnedGit{}, err
		}
		if commits[index].Digest != "" && commits[index].Digest != digest {
			return PinnedGit{}, fmt.Errorf("assemble review model: commit %q digest mismatch", commits[index].OID)
		}
		commits[index].Digest = digest
	}
	gitMetadata.Commits = commits
	queries := append([]PinnedGitQuery(nil), gitMetadata.Queries...)
	for index := range queries {
		digest, err := digestValue(struct{ Query, Output string }{queries[index].Query, queries[index].Output})
		if err != nil {
			return PinnedGit{}, err
		}
		if queries[index].Digest != "" && queries[index].Digest != digest {
			return PinnedGit{}, fmt.Errorf("assemble review model: Git query digest mismatch")
		}
		queries[index].Digest = digest
	}
	gitMetadata.Queries = queries
	digest, err := digestValue(struct {
		ObjectFormat, BaseOID, EffectiveBaseOID, TargetOID, MergeBaseOID string
		Commits                                                          []PinnedCommit
		Queries                                                          []PinnedGitQuery
	}{gitMetadata.ObjectFormat, gitMetadata.BaseOID, gitMetadata.EffectiveBaseOID, gitMetadata.TargetOID, gitMetadata.MergeBaseOID, commits, queries})
	if err != nil {
		return PinnedGit{}, err
	}
	if gitMetadata.Digest != "" && gitMetadata.Digest != digest {
		return PinnedGit{}, fmt.Errorf("assemble review model: pinned Git digest mismatch")
	}
	gitMetadata.Digest = digest
	return gitMetadata, nil
}

func normalizeGuidance(guidance []Guidance) ([]Guidance, []ContextArtifact, error) {
	result := append([]Guidance(nil), guidance...)
	artifacts := make([]ContextArtifact, 0, len(result))
	for index := range result {
		item := &result[index]
		if item.Tier < PolicyTierBasePolicy || item.Tier > PolicyTierTargetEvidence {
			return nil, nil, fmt.Errorf("assemble review model: guidance %q has invalid tier", item.Path)
		}
		if item.Path != "" {
			if err := snapshot.ValidateRepositoryPath(item.Path); err != nil {
				return nil, nil, fmt.Errorf("assemble review model: guidance path %q: %w", item.Path, err)
			}
		}
		if len(item.Rules) == 0 && (item.Kind == GuidancePolicy || item.Kind == GuidanceTargetPolicy) {
			item.Rules = ParsePolicyRules(item.Content, item.Tier, item.ID)
		}
		if item.ID == "" {
			item.ID = item.Path + "\x00" + string(item.Kind)
		}
		digest, err := digestValue(struct {
			ID, Path       string
			Kind           GuidanceKind
			Tier           PolicyTier
			Scope, Content string
			Rules          []PolicyRule
		}{item.ID, item.Path, item.Kind, item.Tier, item.Scope, item.Content, item.Rules})
		if err != nil {
			return nil, nil, err
		}
		if item.Digest != "" && item.Digest != digest {
			return nil, nil, fmt.Errorf("assemble review model: guidance %q digest mismatch", item.Path)
		}
		item.Digest = digest
		artifacts = append(
			artifacts,
			ContextArtifact{
				ID:      item.ID,
				Kind:    string(item.Kind),
				Source:  "snapshot_guidance",
				Path:    item.Path,
				Tier:    item.Tier,
				Content: item.Content,
				Digest:  digest,
			},
		)
	}
	sort.Slice(result, func(i, j int) bool { return result[i].ID < result[j].ID })
	return result, artifacts, nil
}

func assembleIntent(input Input, gitMetadata PinnedGit, guidance []Guidance) (Intent, []ContextArtifact, error) {
	base := make([]ContextArtifact, 0)
	for _, item := range guidance {
		if item.Tier == PolicyTierBasePolicy || item.Tier == PolicyTierBaseDocumentation {
			base = append(
				base,
				ContextArtifact{
					ID:      item.ID,
					Kind:    string(item.Kind),
					Source:  "base_snapshot",
					Path:    item.Path,
					Tier:    item.Tier,
					Content: item.Content,
					Digest:  item.Digest,
				},
			)
		}
	}
	var earlier *EarlierRound
	if input.EarlierRound != nil {
		copyEarlier := *input.EarlierRound
		digest, err := digestValue(
			struct{ SessionID, RoundID, SnapshotDigest, Intent string }{
				copyEarlier.SessionID,
				copyEarlier.RoundID,
				copyEarlier.SnapshotDigest,
				copyEarlier.Intent,
			},
		)
		if err != nil {
			return Intent{}, nil, err
		}
		if copyEarlier.Digest != "" && copyEarlier.Digest != digest {
			return Intent{}, nil, fmt.Errorf("assemble review model: earlier round digest mismatch")
		}
		copyEarlier.Digest = digest
		earlier = &copyEarlier
	}
	intent := Intent{
		Prompt:         strings.TrimSpace(input.Request.Prompt),
		CommitMessages: append([]PinnedCommit(nil), gitMetadata.Commits...),
		BaseGuidance:   base,
		EarlierRound:   earlier,
	}
	digest, err := digestValue(struct {
		Prompt  string
		Commits []PinnedCommit
		Base    []ContextArtifact
		Earlier *EarlierRound
	}{intent.Prompt, intent.CommitMessages, intent.BaseGuidance, intent.EarlierRound})
	if err != nil {
		return Intent{}, nil, err
	}
	intent.Digest = digest
	artifacts := make([]ContextArtifact, 0, 2)
	if intent.Prompt != "" {
		artifacts = append(
			artifacts,
			newArtifact("user_prompt", "private_request", "", PolicyTierPrivate, intent.Prompt),
		)
	}
	if input.Request.Configuration != "" || len(input.Request.Rules) > 0 {
		content, err := canonicalText(struct {
			Configuration string
			Rules         []PolicyRule
		}{input.Request.Configuration, input.Request.Rules})
		if err != nil {
			return Intent{}, nil, err
		}
		artifacts = append(
			artifacts,
			newArtifact("private_configuration", "private_request", "", PolicyTierPrivate, content),
		)
	}
	if earlier != nil {
		artifacts = append(
			artifacts,
			newArtifact("earlier_round", "same_session_round", earlier.RoundID, PolicyTierPrivate, earlier.Intent),
		)
	}
	return intent, artifacts, nil
}

func newArtifact(kind, source, artifactPath string, tier PolicyTier, content string) ContextArtifact {
	digest, _ := digestValue(struct{ Kind, Source, Path, Content string }{kind, source, artifactPath, content})
	return ContextArtifact{
		ID:      kind + "\x00" + artifactPath,
		Kind:    kind,
		Source:  source,
		Path:    artifactPath,
		Tier:    tier,
		Content: content,
		Digest:  digest,
	}
}

func assembleFiles(
	ctx context.Context,
	capture snapshot.Capture,
	content ContentReader,
) ([]FileChange, []AffectedSurface, error) {
	baseByPath := entriesByPath(capture.BaseEntries)
	targetByPath := entriesByPath(capture.TargetEntries)
	files := make([]FileChange, 0)
	for _, change := range capture.Changes {
		if change.Status == snapshot.ChangeUnchanged {
			continue
		}
		file := FileChange{
			Status:       change.Status,
			BasePath:     change.BasePath,
			TargetPath:   change.TargetPath,
			BaseDigest:   change.BaseDigest,
			TargetDigest: change.TargetDigest,
		}
		baseEntry := baseByPath[change.BasePath]
		targetEntry := targetByPath[change.TargetPath]
		oldBytes, oldAvailable, err := readEntry(ctx, content, baseEntry)
		if err != nil {
			return nil, nil, err
		}
		newBytes, newAvailable, err := readEntry(ctx, content, targetEntry)
		if err != nil {
			return nil, nil, err
		}
		if change.Status == snapshot.ChangeRenamed && change.BaseDigest == change.TargetDigest {
			hunkPath := file.TargetPath
			if hunkPath == "" {
				hunkPath = file.BasePath
			}
			file.Hunks = []Hunk{makeHunk(hunkPath, "rename", 0, 0, 0, 0, nil, false, true)}
		} else if oldAvailable || newAvailable {
			file.Hunks, file.Patch = diffHunks(file.BasePath, file.TargetPath, oldBytes, newBytes)
		} else {
			file.Hunks = []Hunk{makeHunk(file.TargetPath, "content_unavailable", 0, 0, 0, 0, nil, false, false)}
		}
		file.Symbols = symbolsForFile(file.TargetPath, newBytes, newAvailable)
		file.Surfaces = classifySurfaces(file, targetEntry)
		files = append(files, file)
	}
	sort.Slice(files, func(i, j int) bool { return fileSortKey(files[i]) < fileSortKey(files[j]) })
	surfaces := collectSurfaces(files)
	return files, surfaces, nil
}

func readEntry(ctx context.Context, content ContentReader, entry snapshot.Entry) ([]byte, bool, error) {
	if entry.Path == "" || entry.ContentDigest == "" || content == nil {
		return nil, false, nil
	}
	bytes, err := content(ctx, entry.ContentDigest)
	if err != nil {
		return nil, false, fmt.Errorf("assemble review model: read captured object %q: %w", entry.Path, err)
	}
	digest := sha256.Sum256(bytes)
	if hex.EncodeToString(digest[:]) != entry.ContentDigest {
		return nil, false, fmt.Errorf("assemble review model: captured object digest mismatch for %q", entry.Path)
	}
	return bytes, true, nil
}

func entriesByPath(entries []snapshot.Entry) map[string]snapshot.Entry {
	result := make(map[string]snapshot.Entry, len(entries))
	for _, entry := range entries {
		result[entry.Path] = entry
	}
	return result
}

func fileSortKey(file FileChange) string { return file.BasePath + "\x00" + file.TargetPath }

func classifySurfaces(file FileChange, target snapshot.Entry) []SurfaceKind {
	name := strings.ToLower(file.TargetPath)
	if name == "" {
		name = strings.ToLower(file.BasePath)
	}
	set := make(map[SurfaceKind]bool)
	if strings.HasSuffix(name, "_test.go") || strings.Contains(name, "/test/") || strings.Contains(name, "/tests/") ||
		strings.HasSuffix(name, ".test.js") ||
		strings.HasSuffix(name, ".spec.ts") {
		set[SurfaceTests] = true
	}
	if strings.Contains(name, "migration") || strings.Contains(name, "/migrations/") ||
		strings.Contains(name, "/db/sql/") {
		set[SurfaceMigrations] = true
	}
	if name == "go.mod" || name == "go.sum" || strings.HasSuffix(name, "package.json") ||
		strings.Contains(name, "lock") {
		set[SurfaceDependencies] = true
	}
	if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") || strings.HasSuffix(name, ".toml") ||
		strings.HasSuffix(name, ".ini") ||
		strings.HasSuffix(name, ".env") ||
		strings.Contains(name, "/config/") {
		set[SurfaceConfiguration] = true
	}
	if strings.Contains(name, "schema") || strings.HasSuffix(name, ".proto") || strings.HasSuffix(name, ".graphql") ||
		strings.HasSuffix(name, "openapi.json") {
		set[SurfaceContracts] = true
	}
	if target.Kind == snapshot.EntryKindFile &&
		(strings.HasSuffix(name, ".go") || strings.Contains(name, "/api/") || strings.Contains(name, "/public/")) {
		for _, symbol := range file.Symbols {
			if symbol.Kind == "function" || symbol.Kind == "type" {
				set[SurfacePublicAPI] = true
				break
			}
		}
	}
	result := make([]SurfaceKind, 0, len(set))
	for kind := range set {
		result = append(result, kind)
	}
	sort.Slice(result, func(i, j int) bool { return result[i] < result[j] })
	return result
}

func symbolsForFile(filePath string, content []byte, available bool) []Symbol {
	if !available {
		return nil
	}
	text := string(content)
	result := make([]Symbol, 0)
	for index, pattern := range symbolPatterns {
		matches := pattern.FindAllStringSubmatch(text, -1)
		kind := "function"
		if index == 1 {
			kind = "type"
		}
		if index == 2 {
			kind = "value"
		}
		if index == 3 {
			kind = "export"
		}
		for _, match := range matches {
			if len(match) > 1 {
				result = append(result, Symbol{Path: filePath, Kind: kind, Name: match[1]})
			}
		}
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].Name != result[j].Name {
			return result[i].Name < result[j].Name
		}
		return result[i].Kind < result[j].Kind
	})
	result = uniqueSymbols(result)
	return result
}

func uniqueSymbols(symbols []Symbol) []Symbol {
	result := symbols[:0]
	seen := map[string]bool{}
	for _, symbol := range symbols {
		key := symbol.Kind + "\x00" + symbol.Name
		if !seen[key] {
			seen[key] = true
			result = append(result, symbol)
		}
	}
	return result
}

func collectSurfaces(files []FileChange) []AffectedSurface {
	byKind := map[SurfaceKind][]SurfaceEvidence{}
	for _, file := range files {
		for _, kind := range file.Surfaces {
			ids := make([]string, 0, len(file.Hunks))
			for _, hunk := range file.Hunks {
				ids = append(ids, hunk.ID)
			}
			pathName := file.TargetPath
			if pathName == "" {
				pathName = file.BasePath
			}
			byKind[kind] = append(
				byKind[kind],
				SurfaceEvidence{Kind: kind, Path: pathName, HunkIDs: ids, Reason: "path or captured lexical evidence"},
			)
		}
	}
	kinds := make([]SurfaceKind, 0, len(byKind))
	for kind := range byKind {
		kinds = append(kinds, kind)
	}
	sort.Slice(kinds, func(i, j int) bool { return kinds[i] < kinds[j] })
	result := make([]AffectedSurface, 0, len(kinds))
	for _, kind := range kinds {
		evidence := byKind[kind]
		sort.Slice(evidence, func(i, j int) bool { return evidence[i].Path < evidence[j].Path })
		result = append(result, AffectedSurface{Kind: kind, Evidence: evidence})
	}
	return result
}

func sameChanges(left, right []snapshot.Change) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}

func modelWithoutDigest(model ChangeModel) ChangeModel { model.Digest = ""; return model }

func digestValue(value any) (string, error) {
	encoded, err := json.Marshal(value)
	if err != nil {
		return "", err
	}
	digest := sha256.Sum256(encoded)
	return hex.EncodeToString(digest[:]), nil
}

func canonicalText(value any) (string, error) {
	encoded, err := json.Marshal(value)
	if err != nil {
		return "", err
	}
	return string(encoded), nil
}

func sortContext(artifacts []ContextArtifact) {
	sort.Slice(artifacts, func(i, j int) bool {
		if artifacts[i].Tier != artifacts[j].Tier {
			return artifacts[i].Tier < artifacts[j].Tier
		}
		return artifacts[i].ID < artifacts[j].ID
	})
}

// ObjectStoreContentReader adapts the private object store to ContentReader.
func ObjectStoreContentReader(store *snapshot.ObjectStore) ContentReader {
	return func(ctx context.Context, digest string) ([]byte, error) {
		if store == nil {
			return nil, errors.New("object store is nil")
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		default:
		}
		file, err := store.Open(digest)
		if err != nil {
			return nil, err
		}
		defer file.Close()
		return io.ReadAll(file)
	}
}

// ParsePolicyRules parses the deliberately small, non-executable policy
// syntax used by fixtures and private configuration: one key=value rule per
// line, with an optional path scope written as scope:key=value.
func ParsePolicyRules(content string, tier PolicyTier, source string) []PolicyRule {
	lines := strings.Split(strings.ReplaceAll(content, "\r\n", "\n"), "\n")
	rules := make([]PolicyRule, 0)
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue
		}
		left, value := strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
		if left == "" || value == "" {
			continue
		}
		key, scope := left, ""
		if colon := strings.IndexByte(left, ':'); colon > 0 {
			scope, key = strings.TrimSpace(left[:colon]), strings.TrimSpace(left[colon+1:])
		}
		rules = append(rules, PolicyRule{Key: key, Value: value, Scope: scope, Tier: tier, Source: source})
	}
	sort.Slice(rules, func(i, j int) bool {
		if rules[i].Scope != rules[j].Scope {
			return rules[i].Scope < rules[j].Scope
		}
		return rules[i].Key < rules[j].Key
	})
	return rules
}

// parseInt is kept local to policy resolution so numeric limits can use the
// safer, lower interpretation without making policy a general expression DSL.
func parseInt(value string) (int64, bool) {
	parsed, err := strconv.ParseInt(strings.TrimSpace(value), 10, 64)
	return parsed, err == nil
}
