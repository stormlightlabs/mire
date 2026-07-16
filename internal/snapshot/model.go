// Package snapshot contains the immutable data model and private object store
// used by captured review snapshots.
package snapshot

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const (
	ComparisonTwoDot   = "two_dot"
	ComparisonThreeDot = "three_dot"
	ComparisonWorktree = "worktree"

	WorktreeComparison = "HEAD..WORKTREE"

	TreeSideBase     = "base"
	TreeSideTarget   = "target"
	TreeSideHead     = "head"
	TreeSideIndex    = "index"
	TreeSideWorktree = "worktree"

	EntryKindFile      = "file"
	EntryKindSymlink   = "symlink"
	EntryKindSubmodule = "submodule"

	ChangeAdded     = "added"
	ChangeDeleted   = "deleted"
	ChangeModified  = "modified"
	ChangeRenamed   = "renamed"
	ChangeUnchanged = "unchanged"
)

// Entry is one file-like entry in a complete committed tree manifest.
// ContentDigest identifies bytes in MIRE's private object store.
//
// GitOID is retained as provenance and is never used as the durable content copy.
type Entry struct {
	Path          string `json:"path"`
	Kind          string `json:"kind"`
	Mode          uint32 `json:"mode"`
	Size          int64  `json:"size"`
	ContentDigest string `json:"content_digest,omitempty"`
	GitOID        string `json:"git_oid"`
	SymlinkTarget string `json:"symlink_target,omitempty"`
}

// Change describes the durable two-tree comparison. Complete manifests are
// still authoritative for unchanged context and for paths involved in a rename.
type Change struct {
	Status       string `json:"status"`
	BasePath     string `json:"base_path,omitempty"`
	TargetPath   string `json:"target_path,omitempty"`
	BaseDigest   string `json:"base_digest,omitempty"`
	TargetDigest string `json:"target_digest,omitempty"`
}

// Layer identifies one immutable view captured for a review snapshot.
type Layer struct {
	Name           string `json:"name"`
	Identity       string `json:"identity"`
	ManifestDigest string `json:"manifest_digest"`
}

// Capture is the in-memory result of a successful Git capture.
//
// It is safe to persist only after every non-submodule entry has a verified ContentDigest.
type Capture struct {
	ComparisonKind         string
	RequestedComparison    string
	BaseOID                string
	EffectiveBaseOID       string
	TargetOID              string
	MergeBaseOID           string
	IndexOID               string
	WorktreeOID            string
	ObjectFormat           string
	ContextPolicyHash      string
	IgnorePolicy           string
	CapturedAt             time.Time
	BaseEntries            []Entry
	TargetEntries          []Entry
	HeadEntries            []Entry
	IndexEntries           []Entry
	WorktreeEntries        []Entry
	Changes                []Change
	BaseManifestDigest     string
	TargetManifestDigest   string
	HeadManifestDigest     string
	IndexManifestDigest    string
	WorktreeManifestDigest string
	ManifestDigest         string
	Layers                 []Layer
}

// DefaultContextPolicyHash is the recorded hash for the built-in capture policy.
func DefaultContextPolicyHash() string {
	digest := sha256.Sum256([]byte("mire/v1/default-context-policy"))
	return hex.EncodeToString(digest[:])
}

// Validate checks the capture invariants that must hold before a database
// transaction can make it user-visible.
func (capture Capture) Validate() error {
	if strings.TrimSpace(capture.RequestedComparison) == "" {
		return fmt.Errorf("snapshot capture: requested comparison is empty")
	}
	comparisonKind, err := ComparisonKindForComparison(capture.RequestedComparison)
	if err != nil {
		return err
	}
	if capture.ComparisonKind != "" && capture.ComparisonKind != comparisonKind {
		return fmt.Errorf(
			"snapshot capture: comparison kind %q does not match %q",
			capture.ComparisonKind,
			capture.RequestedComparison,
		)
	}
	if strings.TrimSpace(capture.EffectiveBaseOID) == "" || strings.TrimSpace(capture.TargetOID) == "" {
		return fmt.Errorf("snapshot capture: resolved object IDs are incomplete")
	}
	if comparisonKind == ComparisonThreeDot {
		if strings.TrimSpace(capture.BaseOID) == "" || strings.TrimSpace(capture.MergeBaseOID) == "" {
			return fmt.Errorf("snapshot capture: three-dot base and merge-base object IDs are incomplete")
		}
		if capture.EffectiveBaseOID != capture.MergeBaseOID {
			return fmt.Errorf("snapshot capture: effective base and merge-base object IDs differ")
		}
	}
	if capture.ObjectFormat != "sha1" && capture.ObjectFormat != "sha256" {
		return fmt.Errorf("snapshot capture: unsupported object format %q", capture.ObjectFormat)
	}
	if strings.TrimSpace(capture.ContextPolicyHash) == "" {
		return fmt.Errorf("snapshot capture: context policy hash is empty")
	}
	if comparisonKind == ComparisonWorktree && strings.TrimSpace(capture.IndexOID) == "" {
		return fmt.Errorf("snapshot capture: working-tree index identity is empty")
	}
	if capture.CapturedAt.IsZero() {
		return fmt.Errorf("snapshot capture: capture time is zero")
	}
	for side, entries := range map[string][]Entry{
		TreeSideBase: capture.BaseEntries, TreeSideTarget: capture.TargetEntries,
	} {
		if err := validateEntries(side, entries); err != nil {
			return err
		}
	}
	if comparisonKind == ComparisonWorktree {
		for side, entries := range map[string][]Entry{
			TreeSideHead: capture.HeadEntries, TreeSideIndex: capture.IndexEntries,
			TreeSideWorktree: capture.WorktreeEntries,
		} {
			if err := validateEntries(side, entries); err != nil {
				return err
			}
		}
		if capture.HeadManifestDigest == "" || capture.IndexManifestDigest == "" ||
			capture.WorktreeManifestDigest == "" {
			return fmt.Errorf("snapshot capture: working-tree layer manifest digests are incomplete")
		}
		if capture.WorktreeOID != "" && capture.WorktreeOID != capture.WorktreeManifestDigest {
			return fmt.Errorf("snapshot capture: working-tree identity does not match its manifest")
		}
	}
	if capture.BaseManifestDigest == "" || capture.TargetManifestDigest == "" || capture.ManifestDigest == "" {
		return fmt.Errorf("snapshot capture: manifest digests are incomplete")
	}
	return nil
}

// ComparisonKindForComparison identifies the committed comparison syntax in a requested range.
// It rejects malformed or unsupported expressions before any repository object is read.
func ComparisonKindForComparison(requestedComparison string) (string, error) {
	requestedComparison = strings.TrimSpace(requestedComparison)
	if requestedComparison == "" {
		return "", fmt.Errorf("snapshot capture: requested comparison is empty")
	}
	if strings.EqualFold(requestedComparison, WorktreeComparison) {
		return ComparisonWorktree, nil
	}
	if strings.Contains(requestedComparison, "...") {
		if strings.Count(requestedComparison, "...") != 1 || strings.Count(requestedComparison, "..") != 1 {
			return "", fmt.Errorf("snapshot capture: invalid comparison %q", requestedComparison)
		}
		return ComparisonThreeDot, nil
	}
	if strings.Count(requestedComparison, "..") != 1 {
		return "", fmt.Errorf("snapshot capture: unsupported comparison %q", requestedComparison)
	}
	return ComparisonTwoDot, nil
}

func validateEntries(side string, entries []Entry) error {
	previous := ""
	for _, entry := range entries {
		if err := ValidateRepositoryPath(entry.Path); err != nil {
			return fmt.Errorf("snapshot %s manifest: %w", side, err)
		}
		if previous != "" && entry.Path <= previous {
			return fmt.Errorf("snapshot %s manifest: paths are not strictly sorted", side)
		}
		previous = entry.Path
		switch entry.Kind {
		case EntryKindFile, EntryKindSymlink:
			if entry.ContentDigest == "" {
				return fmt.Errorf("snapshot %s manifest: %q has no content digest", side, entry.Path)
			}
			if entry.Size < 0 {
				return fmt.Errorf("snapshot %s manifest: %q has negative size", side, entry.Path)
			}
		case EntryKindSubmodule:
			if entry.GitOID == "" {
				return fmt.Errorf("snapshot %s manifest: %q has no submodule Git OID", side, entry.Path)
			}
		default:
			return fmt.Errorf("snapshot %s manifest: %q has unsupported kind %q", side, entry.Path, entry.Kind)
		}
	}
	return nil
}

// ValidateRepositoryPath validates a Git tree path before it can cross into
// any filesystem or object-store boundary.
func ValidateRepositoryPath(value string) error {
	if value == "" || value == "." || strings.HasPrefix(value, "/") || strings.Contains(value, "\\") ||
		strings.ContainsRune(value, '\x00') {
		return fmt.Errorf("invalid repository path %q", value)
	}
	filesystemPath := filepath.FromSlash(value)
	windowsDrivePath := len(value) >= 3 &&
		((value[0] >= 'a' && value[0] <= 'z') || (value[0] >= 'A' && value[0] <= 'Z')) &&
		value[1] == ':' &&
		value[2] == '/'
	if filepath.IsAbs(filesystemPath) || filepath.VolumeName(filesystemPath) != "" || windowsDrivePath {
		return fmt.Errorf("invalid repository path %q", value)
	}
	parts := strings.Split(value, "/")
	for _, part := range parts {
		if part == "" || part == "." || part == ".." {
			return fmt.Errorf("invalid repository path %q", value)
		}
	}
	return nil
}

// ManifestDigest returns a deterministic digest for one complete tree manifest.
func ManifestDigest(entries []Entry) (string, error) {
	copyEntries := append([]Entry(nil), entries...)
	sort.Slice(copyEntries, func(i, j int) bool { return copyEntries[i].Path < copyEntries[j].Path })
	if err := validateEntries("tree", copyEntries); err != nil {
		return "", err
	}
	encoded, err := json.Marshal(copyEntries)
	if err != nil {
		return "", fmt.Errorf("encode tree manifest: %w", err)
	}
	digest := sha256.Sum256(encoded)
	return hex.EncodeToString(digest[:]), nil
}

// OverallManifestDigest returns the digest for the complete immutable
// snapshot manifest, including provenance and both tree manifests.
func OverallManifestDigest(capture Capture) (string, error) {
	comparisonKind, err := ComparisonKindForComparison(capture.RequestedComparison)
	if err != nil {
		return "", err
	}
	if capture.ComparisonKind != "" && capture.ComparisonKind != comparisonKind {
		return "", fmt.Errorf(
			"snapshot capture: comparison kind %q does not match %q",
			capture.ComparisonKind,
			capture.RequestedComparison,
		)
	}
	baseOID := capture.BaseOID
	if baseOID == "" {
		baseOID = capture.EffectiveBaseOID
	}
	type manifest struct {
		ComparisonKind         string   `json:"comparison_kind"`
		RequestedComparison    string   `json:"requested_comparison"`
		BaseOID                string   `json:"base_oid"`
		EffectiveBaseOID       string   `json:"effective_base_oid"`
		TargetOID              string   `json:"target_oid"`
		MergeBaseOID           string   `json:"merge_base_oid"`
		IndexOID               string   `json:"index_oid"`
		WorktreeOID            string   `json:"worktree_oid"`
		ObjectFormat           string   `json:"object_format"`
		ContextPolicyHash      string   `json:"context_policy_hash"`
		IgnorePolicy           string   `json:"ignore_policy"`
		BaseManifestDigest     string   `json:"base_manifest_digest"`
		TargetManifestDigest   string   `json:"target_manifest_digest"`
		HeadManifestDigest     string   `json:"head_manifest_digest"`
		IndexManifestDigest    string   `json:"index_manifest_digest"`
		WorktreeManifestDigest string   `json:"worktree_manifest_digest"`
		Changes                []Change `json:"changes"`
		Layers                 []Layer  `json:"layers"`
	}
	changes := append([]Change(nil), capture.Changes...)
	sort.Slice(changes, func(i, j int) bool {
		if changes[i].BasePath != changes[j].BasePath {
			return changes[i].BasePath < changes[j].BasePath
		}
		return changes[i].TargetPath < changes[j].TargetPath
	})
	layers := append([]Layer(nil), capture.Layers...)
	if len(layers) == 0 {
		layers = capture.ManifestLayers()
	}
	encoded, err := json.Marshal(manifest{
		ComparisonKind:         comparisonKind,
		RequestedComparison:    capture.RequestedComparison,
		BaseOID:                baseOID,
		EffectiveBaseOID:       capture.EffectiveBaseOID,
		TargetOID:              capture.TargetOID,
		MergeBaseOID:           capture.MergeBaseOID,
		IndexOID:               capture.IndexOID,
		WorktreeOID:            capture.WorktreeOID,
		ObjectFormat:           capture.ObjectFormat,
		ContextPolicyHash:      capture.ContextPolicyHash,
		IgnorePolicy:           capture.IgnorePolicy,
		BaseManifestDigest:     capture.BaseManifestDigest,
		TargetManifestDigest:   capture.TargetManifestDigest,
		HeadManifestDigest:     capture.HeadManifestDigest,
		IndexManifestDigest:    capture.IndexManifestDigest,
		WorktreeManifestDigest: capture.WorktreeManifestDigest,
		Changes:                changes,
		Layers:                 layers,
	})
	if err != nil {
		return "", fmt.Errorf("encode snapshot manifest: %w", err)
	}
	digest := sha256.Sum256(encoded)
	return hex.EncodeToString(digest[:]), nil
}

// ManifestLayers returns the immutable layer metadata represented by capture.
func (capture Capture) ManifestLayers() []Layer {
	if capture.ComparisonKind == ComparisonWorktree ||
		strings.EqualFold(capture.RequestedComparison, WorktreeComparison) {
		return []Layer{
			{Name: TreeSideHead, Identity: capture.BaseOID, ManifestDigest: capture.HeadManifestDigest},
			{Name: TreeSideIndex, Identity: capture.IndexOID, ManifestDigest: capture.IndexManifestDigest},
			{Name: TreeSideWorktree, Identity: capture.WorktreeOID, ManifestDigest: capture.WorktreeManifestDigest},
		}
	}
	return []Layer{
		{Name: TreeSideBase, Identity: capture.EffectiveBaseOID, ManifestDigest: capture.BaseManifestDigest},
		{Name: TreeSideTarget, Identity: capture.TargetOID, ManifestDigest: capture.TargetManifestDigest},
	}
}

// BuildChanges creates deterministic change records and recognizes exact
// content renames without requiring a live worktree or Git command.
func BuildChanges(base, target []Entry) []Change {
	baseByPath := make(map[string]Entry, len(base))
	targetByPath := make(map[string]Entry, len(target))
	for _, entry := range base {
		baseByPath[entry.Path] = entry
	}
	for _, entry := range target {
		targetByPath[entry.Path] = entry
	}
	changes := make([]Change, 0)
	deleted := make([]Entry, 0)
	added := make([]Entry, 0)
	for path, entry := range baseByPath {
		other, ok := targetByPath[path]
		if !ok {
			deleted = append(deleted, entry)
			continue
		}
		status := ChangeUnchanged
		if !sameEntry(entry, other) {
			status = ChangeModified
		}
		changes = append(changes, Change{
			Status: status, BasePath: path, TargetPath: path,
			BaseDigest: entry.ContentDigest, TargetDigest: other.ContentDigest,
		})
	}
	for path, entry := range targetByPath {
		if _, ok := baseByPath[path]; !ok {
			added = append(added, entry)
		}
	}
	sort.Slice(deleted, func(i, j int) bool { return deleted[i].Path < deleted[j].Path })
	sort.Slice(added, func(i, j int) bool { return added[i].Path < added[j].Path })
	usedAdded := make([]bool, len(added))
	for _, old := range deleted {
		rename := -1
		for index, next := range added {
			if usedAdded[index] || !sameContent(old, next) {
				continue
			}
			rename = index
			break
		}
		if rename >= 0 {
			usedAdded[rename] = true
			changes = append(changes, Change{
				Status: ChangeRenamed, BasePath: old.Path,
				TargetPath: added[rename].Path, BaseDigest: old.ContentDigest,
				TargetDigest: added[rename].ContentDigest,
			})
		} else {
			changes = append(changes, Change{
				Status: ChangeDeleted, BasePath: old.Path,
				BaseDigest: old.ContentDigest,
			})
		}
	}
	for index, entry := range added {
		if !usedAdded[index] {
			changes = append(changes, Change{
				Status: ChangeAdded, TargetPath: entry.Path,
				TargetDigest: entry.ContentDigest,
			})
		}
	}
	sort.Slice(changes, func(i, j int) bool {
		left := changes[i].BasePath + "\x00" + changes[i].TargetPath
		right := changes[j].BasePath + "\x00" + changes[j].TargetPath
		return left < right
	})
	return changes
}

func sameEntry(left, right Entry) bool {
	return left.Kind == right.Kind && left.Mode == right.Mode && left.Size == right.Size &&
		left.ContentDigest == right.ContentDigest && left.GitOID == right.GitOID &&
		left.SymlinkTarget == right.SymlinkTarget
}

func sameContent(left, right Entry) bool {
	return left.Kind == right.Kind && left.Mode == right.Mode && left.Size == right.Size &&
		left.ContentDigest != "" && left.ContentDigest == right.ContentDigest
}
