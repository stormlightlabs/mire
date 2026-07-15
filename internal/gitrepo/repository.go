// Package gitrepo provides read-only Git discovery and committed snapshot capture.
// It relies heavily on go-git ([git]).
package gitrepo

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"time"

	git "github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/filemode"
	"github.com/go-git/go-git/v6/plumbing/format/index"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/plumbing/storer"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

var (
	// ErrNotGitRepository is returned when a directory is not inside a
	// non-bare Git worktree.
	ErrNotGitRepository = errors.New("not inside a Git repository")
	// ErrAmbiguousRevision is returned when a revision expression has more than
	// one valid committed interpretation.
	ErrAmbiguousRevision = errors.New("ambiguous Git revision")
	// ErrNoMergeBase is returned when the two revisions have no common ancestor.
	ErrNoMergeBase = errors.New("no merge base")
	// ErrMultipleMergeBases is returned when Git's best common ancestor set is
	// not unique.
	ErrMultipleMergeBases = errors.New("multiple merge bases")
	// ErrTornWorktree is returned when the repository changes during capture.
	ErrTornWorktree = errors.New("working tree changed during capture")
	// ErrDirtySubmodule is returned when a submodule contains state that cannot
	// be represented as an opaque Git link in the containing snapshot.
	ErrDirtySubmodule = errors.New("dirty submodule")
	// ErrCaptureLimit is returned when a capture exceeds an explicit resource
	// ceiling.
	ErrCaptureLimit = errors.New("capture resource limit exceeded")
)

// Repository is an opened read-only view of a local worktree.
type Repository struct {
	GitDir string
	Root   string
	Git    *git.Repository
}

// Open discovers and opens the repository containing directory. go-git walks
// parent directories when DetectDotGit is enabled and never writes while
// opening or reading the repository.
func Open(ctx context.Context, directory string) (*Repository, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	select {
	case <-ctx.Done():
		return nil, fmt.Errorf("open Git repository: %w", ctx.Err())
	default:
	}
	if strings.TrimSpace(directory) == "" {
		var err error
		directory, err = os.Getwd()
		if err != nil {
			return nil, fmt.Errorf("find current directory: %w", err)
		}
	}
	absolute, err := filepath.Abs(directory)
	if err != nil {
		return nil, fmt.Errorf("resolve repository directory: %w", err)
	}
	repository, err := git.PlainOpenWithOptions(absolute, &git.PlainOpenOptions{DetectDotGit: true})
	if err != nil {
		if errors.Is(err, git.ErrRepositoryNotExists) || errors.Is(err, os.ErrNotExist) {
			return nil, fmt.Errorf("%w: %s", ErrNotGitRepository, absolute)
		}
		return nil, fmt.Errorf("open Git repository from %s: %w", absolute, err)
	}
	worktree, err := repository.Worktree()
	if err != nil {
		_ = repository.Close()
		if errors.Is(err, git.ErrIsBareRepository) {
			return nil, fmt.Errorf("%w: bare repositories are not supported", ErrNotGitRepository)
		}
		return nil, fmt.Errorf("open Git worktree: %w", err)
	}
	root, err := canonicalPath(worktree.Filesystem().Root())
	if err != nil {
		_ = repository.Close()
		return nil, fmt.Errorf("canonicalize Git worktree: %w", err)
	}
	gitDir := ""
	if filesystemStorer, ok := repository.Storer.(storer.FilesystemStorer); ok {
		gitDir, err = canonicalPath(filesystemStorer.Filesystem().Root())
		if err != nil {
			_ = repository.Close()
			return nil, fmt.Errorf("canonicalize Git directory: %w", err)
		}
	} else {
		_ = repository.Close()
		return nil, fmt.Errorf("open Git repository: storage has no filesystem identity")
	}
	return &Repository{GitDir: gitDir, Root: root, Git: repository}, nil
}

// Discover returns the stable repository identity used by the application
// store without retaining an open Git handle.
func Discover(ctx context.Context, directory string) (db.RepositoryIdentity, error) {
	repository, err := Open(ctx, directory)
	if err != nil {
		return db.RepositoryIdentity{}, err
	}
	defer repository.Git.Close()
	displayName := filepath.Base(repository.Root)
	if displayName == "." || displayName == string(filepath.Separator) || displayName == "" {
		displayName = repository.Root
	}
	return db.RepositoryIdentity{
		CanonicalIdentity: repository.Root,
		DisplayName:       displayName,
		DiscoveredGitDir:  repository.GitDir,
	}, nil
}

// CaptureLimits bounds the resources a capture may consume. File count is the
// number of distinct repository paths represented across all captured layers;
// byte limits apply to each object and to all layer content read by a capture.
// Zero values use the defaults returned by DefaultCaptureLimits.
type CaptureLimits struct {
	MaxFileCount     int
	MaxObjectBytes   int64
	MaxCapturedBytes int64
}

// CaptureLimitError explains which configured resource ceiling rejected a
// capture.
type CaptureLimitError struct {
	Resource   string
	Configured int64
	Observed   int64
	Path       string
}

func (err *CaptureLimitError) Error() string {
	message := fmt.Sprintf("%s limit %d exceeded by observed value %d", err.Resource, err.Configured, err.Observed)
	if err.Path != "" {
		message += fmt.Sprintf(" at %q", err.Path)
	}
	return fmt.Sprintf("%s: %s", ErrCaptureLimit, message)
}

// Unwrap lets callers distinguish a resource-limit failure from other
// capture failures while retaining the configured-limit diagnostic.
func (err *CaptureLimitError) Unwrap() error { return ErrCaptureLimit }

// DefaultCaptureLimits returns resource ceilings.
func DefaultCaptureLimits() CaptureLimits {
	return CaptureLimits{
		MaxFileCount:     100_000,
		MaxObjectBytes:   256 << 20,
		MaxCapturedBytes: 1 << 30,
	}
}

// CaptureOptions controls deterministic capture metadata and resource policy.
type CaptureOptions struct {
	Clock       func() time.Time
	PolicyHash  string
	MaxAttempts int
	Limits      CaptureLimits
}

func normalizeCaptureLimits(requested CaptureLimits) (CaptureLimits, error) {
	defaults := DefaultCaptureLimits()
	limits := requested
	if limits.MaxFileCount < 0 || limits.MaxObjectBytes < 0 || limits.MaxCapturedBytes < 0 {
		return CaptureLimits{}, fmt.Errorf("capture limits must not be negative")
	}
	if limits.MaxFileCount == 0 {
		limits.MaxFileCount = defaults.MaxFileCount
	}
	if limits.MaxObjectBytes == 0 {
		limits.MaxObjectBytes = defaults.MaxObjectBytes
	}
	if limits.MaxCapturedBytes == 0 {
		limits.MaxCapturedBytes = defaults.MaxCapturedBytes
	}
	return limits, nil
}

type capturePlan struct {
	limits CaptureLimits
	paths  map[string]struct{}
	bytes  int64
}

func newCapturePlan(limits CaptureLimits) *capturePlan {
	return &capturePlan{limits: limits, paths: make(map[string]struct{})}
}

func (plan *capturePlan) add(path string, size int64) error {
	if size < 0 {
		return fmt.Errorf("capture plan: negative object size for %q", path)
	}
	if size > plan.limits.MaxObjectBytes {
		return &CaptureLimitError{Resource: "individual object bytes", Configured: plan.limits.MaxObjectBytes, Observed: size, Path: path}
	}
	if _, exists := plan.paths[path]; !exists {
		if len(plan.paths) >= plan.limits.MaxFileCount {
			return &CaptureLimitError{Resource: "file count", Configured: int64(plan.limits.MaxFileCount), Observed: int64(len(plan.paths) + 1), Path: path}
		}
		plan.paths[path] = struct{}{}
	}
	if size > plan.limits.MaxCapturedBytes-plan.bytes {
		return &CaptureLimitError{Resource: "aggregate captured bytes", Configured: plan.limits.MaxCapturedBytes, Observed: plan.bytes + size, Path: path}
	}
	plan.bytes += size
	return nil
}

type captureBudget struct {
	limits CaptureLimits
	paths  map[string]struct{}
	bytes  int64
}

type captureReservation struct {
	expected int64
}

func newCaptureBudget(limits CaptureLimits) *captureBudget {
	return &captureBudget{limits: limits, paths: make(map[string]struct{})}
}

func (budget *captureBudget) reserve(path string, size int64) (captureReservation, error) {
	if size < 0 {
		return captureReservation{}, fmt.Errorf("capture: negative object size for %q", path)
	}
	if size > budget.limits.MaxObjectBytes {
		return captureReservation{}, &CaptureLimitError{Resource: "individual object bytes", Configured: budget.limits.MaxObjectBytes, Observed: size, Path: path}
	}
	if _, exists := budget.paths[path]; !exists {
		if len(budget.paths) >= budget.limits.MaxFileCount {
			return captureReservation{}, &CaptureLimitError{Resource: "file count", Configured: int64(budget.limits.MaxFileCount), Observed: int64(len(budget.paths) + 1), Path: path}
		}
		budget.paths[path] = struct{}{}
	}
	if size > budget.limits.MaxCapturedBytes-budget.bytes {
		return captureReservation{}, &CaptureLimitError{Resource: "aggregate captured bytes", Configured: budget.limits.MaxCapturedBytes, Observed: budget.bytes + size, Path: path}
	}
	budget.bytes += size
	return captureReservation{expected: size}, nil
}

func (budget *captureBudget) finalize(reservation captureReservation, actual int64, path string) error {
	if actual < 0 {
		return fmt.Errorf("capture: negative captured size for %q", path)
	}
	if actual > budget.limits.MaxObjectBytes {
		return &CaptureLimitError{Resource: "individual object bytes", Configured: budget.limits.MaxObjectBytes, Observed: actual, Path: path}
	}
	if actual > reservation.expected {
		increase := actual - reservation.expected
		if increase > budget.limits.MaxCapturedBytes-budget.bytes {
			return &CaptureLimitError{Resource: "aggregate captured bytes", Configured: budget.limits.MaxCapturedBytes, Observed: budget.bytes + increase, Path: path}
		}
		budget.bytes += increase
	} else {
		budget.bytes -= reservation.expected - actual
	}
	return nil
}

type captureSizeReader struct {
	reader io.Reader
	max    int64
	path   string
	read   int64
}

func (reader *captureSizeReader) Read(buffer []byte) (int, error) {
	if reader.read >= reader.max {
		var probe [1]byte
		count, err := reader.reader.Read(probe[:])
		if count > 0 {
			return 0, &CaptureLimitError{Resource: "individual object bytes", Configured: reader.max, Observed: reader.max + int64(count), Path: reader.path}
		}
		return 0, err
	}
	remaining := reader.max - reader.read
	if int64(len(buffer)) > remaining {
		buffer = buffer[:remaining]
	}
	count, err := reader.reader.Read(buffer)
	reader.read += int64(count)
	return count, err
}

func putCapturedObject(ctx context.Context, objectStore *snapshot.ObjectStore, budget *captureBudget, path string, reader io.Reader, expectedSize int64) (snapshot.Object, error) {
	reservation, err := budget.reserve(path, expectedSize)
	if err != nil {
		return snapshot.Object{}, err
	}
	stored, err := objectStore.Put(ctx, &captureSizeReader{reader: reader, max: budget.limits.MaxObjectBytes, path: path})
	if err != nil {
		return snapshot.Object{}, err
	}
	if err := budget.finalize(reservation, stored.Size, path); err != nil {
		return snapshot.Object{}, err
	}
	return stored, nil
}

// CaptureRange opens directory, resolves a committed range exactly once, and
// copies complete effective-base and target trees into objectStore.
func CaptureRange(ctx context.Context, directory, requestedComparison string, objectStore *snapshot.ObjectStore) (snapshot.Capture, error) {
	return CaptureRangeWithOptions(ctx, directory, requestedComparison, objectStore, CaptureOptions{})
}

// CaptureRangeWithOptions is CaptureRange with injectable capture metadata.
func CaptureRangeWithOptions(ctx context.Context, directory, requestedComparison string, objectStore *snapshot.ObjectStore, options CaptureOptions) (snapshot.Capture, error) {
	if objectStore == nil {
		return snapshot.Capture{}, fmt.Errorf("capture Git range: object store is nil")
	}
	limits, err := normalizeCaptureLimits(options.Limits)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture Git range: %w", err)
	}
	baseRevision, targetRevision, comparisonKind, err := parseComparisonRange(requestedComparison)
	if err != nil {
		return snapshot.Capture{}, err
	}
	repository, err := Open(ctx, directory)
	if err != nil {
		return snapshot.Capture{}, err
	}
	defer repository.Git.Close()
	baseOID, err := resolveCommitRevision(repository.Git, baseRevision)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("resolve base revision %q: %w", baseRevision, err)
	}
	targetOID, err := resolveCommitRevision(repository.Git, targetRevision)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("resolve target revision %q: %w", targetRevision, err)
	}
	effectiveBaseOID := baseOID
	mergeBaseOID := ""
	if comparisonKind == snapshot.ComparisonThreeDot {
		mergeBaseOID, err = resolveMergeBase(repository.Git, baseOID, targetOID)
		if err != nil {
			return snapshot.Capture{}, fmt.Errorf("resolve merge base for %q...%q: %w", baseRevision, targetRevision, err)
		}
		effectiveBaseOID = plumbing.NewHash(mergeBaseOID)
	}
	objectFormat, err := gitObjectFormat(repository.Git)
	if err != nil {
		return snapshot.Capture{}, err
	}
	clock := options.Clock
	if clock == nil {
		clock = time.Now
	}
	capturedAt := clock().UTC()
	if capturedAt.IsZero() {
		return snapshot.Capture{}, fmt.Errorf("capture Git range: clock returned zero time")
	}
	policyHash := strings.TrimSpace(options.PolicyHash)
	if policyHash == "" {
		policyHash = snapshot.DefaultContextPolicyHash()
	}
	plan := newCapturePlan(limits)
	if _, err := planTree(ctx, repository.Git, effectiveBaseOID, plan); err != nil {
		return snapshot.Capture{}, fmt.Errorf("plan effective base tree %s: %w", effectiveBaseOID, err)
	}
	if _, err := planTree(ctx, repository.Git, targetOID, plan); err != nil {
		return snapshot.Capture{}, fmt.Errorf("plan target tree %s: %w", targetOID, err)
	}
	budget := newCaptureBudget(limits)
	baseEntries, err := captureTreeWithBudget(ctx, repository.Git, effectiveBaseOID, objectStore, budget)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture effective base tree %s: %w", effectiveBaseOID, err)
	}
	targetEntries, err := captureTreeWithBudget(ctx, repository.Git, targetOID, objectStore, budget)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture target tree %s: %w", targetOID, err)
	}
	baseManifestDigest, err := snapshot.ManifestDigest(baseEntries)
	if err != nil {
		return snapshot.Capture{}, err
	}
	targetManifestDigest, err := snapshot.ManifestDigest(targetEntries)
	if err != nil {
		return snapshot.Capture{}, err
	}
	capture := snapshot.Capture{
		ComparisonKind:       comparisonKind,
		RequestedComparison:  requestedComparison,
		BaseOID:              baseOID.String(),
		EffectiveBaseOID:     effectiveBaseOID.String(),
		TargetOID:            targetOID.String(),
		MergeBaseOID:         mergeBaseOID,
		ObjectFormat:         objectFormat,
		ContextPolicyHash:    policyHash,
		CapturedAt:           capturedAt,
		BaseEntries:          baseEntries,
		TargetEntries:        targetEntries,
		BaseManifestDigest:   baseManifestDigest,
		TargetManifestDigest: targetManifestDigest,
	}
	capture.Changes = snapshot.BuildChanges(baseEntries, targetEntries)
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		return snapshot.Capture{}, err
	}
	if err := capture.Validate(); err != nil {
		return snapshot.Capture{}, err
	}
	return capture, nil
}

// CaptureWorktree captures HEAD, the index, and the final working tree into
// MIRE-owned objects without changing the target repository.
func CaptureWorktree(ctx context.Context, directory string, objectStore *snapshot.ObjectStore) (snapshot.Capture, error) {
	return CaptureWorktreeWithOptions(ctx, directory, objectStore, CaptureOptions{})
}

// CaptureWorktreeWithOptions is CaptureWorktree with injectable capture
// metadata and a bounded retry count for a changing working tree.
func CaptureWorktreeWithOptions(ctx context.Context, directory string, objectStore *snapshot.ObjectStore, options CaptureOptions) (snapshot.Capture, error) {
	if objectStore == nil {
		return snapshot.Capture{}, fmt.Errorf("capture Git worktree: object store is nil")
	}
	limits, err := normalizeCaptureLimits(options.Limits)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture Git worktree: %w", err)
	}
	repository, err := Open(ctx, directory)
	if err != nil {
		return snapshot.Capture{}, err
	}
	defer repository.Git.Close()
	objectFormat, err := gitObjectFormat(repository.Git)
	if err != nil {
		return snapshot.Capture{}, err
	}
	clock := options.Clock
	if clock == nil {
		clock = time.Now
	}
	policyHash := strings.TrimSpace(options.PolicyHash)
	if policyHash == "" {
		policyHash = snapshot.DefaultContextPolicyHash()
	}
	attempts := options.MaxAttempts
	if attempts == 0 {
		attempts = 3
	}
	if attempts < 1 {
		return snapshot.Capture{}, fmt.Errorf("capture Git worktree: max attempts must be positive")
	}

	var lastErr error
	for attempt := 1; attempt <= attempts; attempt++ {
		captured, err := captureWorktreeAttempt(ctx, repository, objectStore, objectFormat, policyHash, limits, clock)
		if err != nil {
			if errors.Is(err, ErrTornWorktree) && !errors.Is(err, context.Canceled) && !errors.Is(err, context.DeadlineExceeded) {
				lastErr = err
				continue
			}
			return snapshot.Capture{}, fmt.Errorf("capture Git worktree: %w", err)
		}
		if err := verifyWorktreeStable(ctx, repository, captured); err == nil {
			return captured, nil
		} else {
			if !errors.Is(err, ErrTornWorktree) {
				return snapshot.Capture{}, fmt.Errorf("capture Git worktree: %w", err)
			}
			lastErr = err
		}
	}
	return snapshot.Capture{}, fmt.Errorf("%w after %d attempts: %v", ErrTornWorktree, attempts, lastErr)
}

const worktreeIgnorePolicy = "Git ignore rules exclude ignored untracked paths; tracked paths remain captured."

func captureWorktreeAttempt(ctx context.Context, repository *Repository, objectStore *snapshot.ObjectStore, objectFormat, policyHash string, limits CaptureLimits, clock func() time.Time) (snapshot.Capture, error) {
	head, err := repository.Git.Head()
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read HEAD: %w", err)
	}
	headOID := head.Hash()
	if headOID.IsZero() {
		return snapshot.Capture{}, fmt.Errorf("read HEAD: object ID is empty")
	}
	indexIdentity, err := readIndexIdentity(repository.Git)
	if err != nil {
		return snapshot.Capture{}, err
	}
	worktree, err := repository.Git.Worktree()
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("open Git worktree: %w", err)
	}
	status, err := worktree.StatusWithOptions(git.StatusOptions{Strategy: git.Preload})
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read Git worktree status: %w", err)
	}
	indexFile, err := repository.Git.Storer.Index()
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("read Git index: %w", err)
	}
	plan := newCapturePlan(limits)
	headInventory, err := planTree(ctx, repository.Git, headOID, plan)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("plan HEAD tree %s: %w", headOID, err)
	}
	indexInventory, err := planIndex(ctx, repository.Git, indexFile, plan)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("plan index: %w", err)
	}
	paths, err := worktreeInventory(headInventory, indexFile, status)
	if err != nil {
		return snapshot.Capture{}, err
	}
	if err := planWorktree(ctx, repository.Root, headInventory, indexInventory, paths, plan); err != nil {
		return snapshot.Capture{}, fmt.Errorf("plan final worktree: %w", err)
	}
	budget := newCaptureBudget(limits)
	headEntries, err := captureTreeWithBudget(ctx, repository.Git, headOID, objectStore, budget)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture HEAD tree %s: %w", headOID, err)
	}
	indexEntries, err := captureIndexWithBudget(ctx, repository.Git, indexFile, objectStore, budget)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture index: %w", err)
	}
	worktreeEntries, err := captureWorktreeEntries(ctx, repository.Root, headEntries, indexEntries, paths, objectStore, budget)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture final worktree: %w", err)
	}
	headManifestDigest, err := snapshot.ManifestDigest(headEntries)
	if err != nil {
		return snapshot.Capture{}, err
	}
	indexManifestDigest, err := snapshot.ManifestDigest(indexEntries)
	if err != nil {
		return snapshot.Capture{}, err
	}
	worktreeManifestDigest, err := snapshot.ManifestDigest(worktreeEntries)
	if err != nil {
		return snapshot.Capture{}, err
	}
	capturedAt := clock().UTC()
	if capturedAt.IsZero() {
		return snapshot.Capture{}, fmt.Errorf("capture Git worktree: clock returned zero time")
	}
	capture := snapshot.Capture{
		ComparisonKind:         snapshot.ComparisonWorktree,
		RequestedComparison:    snapshot.WorktreeComparison,
		BaseOID:                headOID.String(),
		EffectiveBaseOID:       headOID.String(),
		TargetOID:              worktreeManifestDigest,
		IndexOID:               indexIdentity,
		WorktreeOID:            worktreeManifestDigest,
		ObjectFormat:           objectFormat,
		ContextPolicyHash:      policyHash,
		IgnorePolicy:           worktreeIgnorePolicy,
		CapturedAt:             capturedAt,
		BaseEntries:            headEntries,
		TargetEntries:          worktreeEntries,
		HeadEntries:            headEntries,
		IndexEntries:           indexEntries,
		WorktreeEntries:        worktreeEntries,
		BaseManifestDigest:     headManifestDigest,
		TargetManifestDigest:   worktreeManifestDigest,
		HeadManifestDigest:     headManifestDigest,
		IndexManifestDigest:    indexManifestDigest,
		WorktreeManifestDigest: worktreeManifestDigest,
		Changes:                snapshot.BuildChanges(headEntries, worktreeEntries),
	}
	capture.Layers = capture.ManifestLayers()
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		return snapshot.Capture{}, err
	}
	if err := capture.Validate(); err != nil {
		return snapshot.Capture{}, err
	}
	return capture, nil
}

func readIndexIdentity(repository *git.Repository) (string, error) {
	filesystemStorer, ok := repository.Storer.(storer.FilesystemStorer)
	if !ok {
		return "", fmt.Errorf("read Git index: storage has no filesystem identity")
	}
	file, err := filesystemStorer.Filesystem().Open("index")
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return digestBytes(nil), nil
		}
		return "", fmt.Errorf("open Git index: %w", err)
	}
	content, readErr := io.ReadAll(file)
	closeErr := file.Close()
	if readErr != nil {
		return "", fmt.Errorf("read Git index: %w", readErr)
	}
	if closeErr != nil {
		return "", fmt.Errorf("close Git index: %w", closeErr)
	}
	return digestBytes(content), nil
}

func digestBytes(content []byte) string {
	digest := sha256.Sum256(content)
	return hex.EncodeToString(digest[:])
}

func captureIndex(ctx context.Context, repository *git.Repository, gitIndex *index.Index, objectStore *snapshot.ObjectStore) ([]snapshot.Entry, error) {
	limits, _ := normalizeCaptureLimits(CaptureLimits{})
	return captureIndexWithBudget(ctx, repository, gitIndex, objectStore, newCaptureBudget(limits))
}

func captureIndexWithBudget(ctx context.Context, repository *git.Repository, gitIndex *index.Index, objectStore *snapshot.ObjectStore, budget *captureBudget) ([]snapshot.Entry, error) {
	if gitIndex == nil {
		return nil, fmt.Errorf("Git index is nil")
	}
	entries := make([]snapshot.Entry, 0, len(gitIndex.Entries))
	seen := make(map[string]struct{}, len(gitIndex.Entries))
	for _, indexEntry := range gitIndex.Entries {
		if indexEntry == nil {
			return nil, fmt.Errorf("Git index contains a nil entry")
		}
		if indexEntry.Stage != 0 {
			return nil, fmt.Errorf("Git index contains unresolved merge entries; resolve the index before reviewing")
		}
		if indexEntry.IntentToAdd {
			return nil, fmt.Errorf("Git index contains intent-to-add entry %q; stage the file before reviewing", indexEntry.Name)
		}
		entryPath := filepath.ToSlash(indexEntry.Name)
		if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
			return nil, fmt.Errorf("index entry %q: %w", indexEntry.Name, err)
		}
		if _, ok := seen[entryPath]; ok {
			return nil, fmt.Errorf("Git index contains duplicate entry %q", entryPath)
		}
		seen[entryPath] = struct{}{}
		if indexEntry.Mode == filemode.Submodule {
			if _, err := budget.reserve(entryPath, 0); err != nil {
				return nil, err
			}
			entries = append(entries, snapshot.Entry{
				Path: entryPath, Kind: snapshot.EntryKindSubmodule, Mode: uint32(indexEntry.Mode), GitOID: indexEntry.Hash.String(),
			})
			continue
		}
		if indexEntry.Hash.IsZero() {
			return nil, fmt.Errorf("Git index entry %q has no Git object", entryPath)
		}
		blob, err := repository.BlobObject(indexEntry.Hash)
		if err != nil {
			return nil, fmt.Errorf("read index blob %q: %w", entryPath, err)
		}
		reader, err := blob.Reader()
		if err != nil {
			return nil, fmt.Errorf("open index blob %q: %w", entryPath, err)
		}
		stored, storeErr := objectStore.Put(ctx, reader)
		closeErr := reader.Close()
		if storeErr != nil {
			return nil, fmt.Errorf("store index blob %q: %w", entryPath, storeErr)
		}
		if closeErr != nil {
			return nil, fmt.Errorf("close index blob %q: %w", entryPath, closeErr)
		}
		entry := snapshot.Entry{
			Path: entryPath, Kind: snapshot.EntryKindFile, Mode: uint32(indexEntry.Mode), Size: stored.Size,
			ContentDigest: stored.Digest, GitOID: indexEntry.Hash.String(),
		}
		if indexEntry.Mode == filemode.Symlink {
			content, err := readStoredObject(objectStore, stored.Digest)
			if err != nil {
				return nil, fmt.Errorf("read index symlink target %q: %w", entryPath, err)
			}
			entry.Kind = snapshot.EntryKindSymlink
			entry.SymlinkTarget = string(content)
		}
		entries = append(entries, entry)
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Path < entries[j].Path })
	return entries, nil
}

func worktreeInventory(headEntries []snapshot.Entry, gitIndex *index.Index, status git.Status) ([]string, error) {
	paths := make(map[string]struct{}, len(headEntries)+len(gitIndex.Entries)+len(status))
	for _, entry := range headEntries {
		paths[entry.Path] = struct{}{}
	}
	for _, entry := range gitIndex.Entries {
		if entry == nil {
			return nil, fmt.Errorf("Git index contains a nil entry")
		}
		entryPath := filepath.ToSlash(entry.Name)
		if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
			return nil, fmt.Errorf("index entry %q: %w", entry.Name, err)
		}
		paths[entryPath] = struct{}{}
	}
	for name, fileStatus := range status {
		if fileStatus == nil || fileStatus.Worktree != git.Untracked || fileStatus.Staging != git.Untracked {
			continue
		}
		name = filepath.ToSlash(name)
		if err := snapshot.ValidateRepositoryPath(name); err != nil {
			return nil, fmt.Errorf("untracked path %q: %w", name, err)
		}
		paths[name] = struct{}{}
	}
	result := make([]string, 0, len(paths))
	for name := range paths {
		result = append(result, name)
	}
	sort.Strings(result)
	return result, nil
}

func captureWorktreeEntries(ctx context.Context, root string, headEntries, indexEntries []snapshot.Entry, paths []string, objectStore *snapshot.ObjectStore, budget *captureBudget) ([]snapshot.Entry, error) {
	expected := make(map[string]snapshot.Entry, len(headEntries)+len(indexEntries))
	for _, entry := range headEntries {
		expected[entry.Path] = entry
	}
	for _, entry := range indexEntries {
		expected[entry.Path] = entry
	}
	entries := make([]snapshot.Entry, 0, len(paths))
	for _, entryPath := range paths {
		fullPath, err := safeWorktreePath(root, entryPath)
		if err != nil {
			return nil, err
		}
		info, err := os.Lstat(fullPath)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, fmt.Errorf("inspect %q: %w", entryPath, err)
		}
		prior, hasPrior := expected[entryPath]
		if info.IsDir() {
			if !hasPrior || prior.Kind != snapshot.EntryKindSubmodule {
				return nil, fmt.Errorf("working-tree path %q is a directory; only clean submodules may be captured as directories", entryPath)
			}
			if err := verifySubmodule(root, entryPath, prior.GitOID); err != nil {
				return nil, err
			}
			if _, err := budget.reserve(entryPath, 0); err != nil {
				return nil, err
			}
			entries = append(entries, snapshot.Entry{Path: entryPath, Kind: snapshot.EntryKindSubmodule, Mode: uint32(filemode.Submodule), GitOID: prior.GitOID})
			continue
		}
		if !info.Mode().IsRegular() && info.Mode()&os.ModeSymlink == 0 {
			return nil, fmt.Errorf("working-tree path %q is a special device or unsupported filesystem entry", entryPath)
		}
		mode, err := filemode.NewFromOSFileMode(info.Mode())
		if err != nil {
			return nil, fmt.Errorf("working-tree path %q: unsupported file type: %w", entryPath, err)
		}
		if mode == filemode.Symlink {
			if hasPrior && prior.Kind == snapshot.EntryKindSubmodule {
				return nil, fmt.Errorf("%w: %q is not a Git directory", ErrDirtySubmodule, entryPath)
			}
			target, err := os.Readlink(fullPath)
			if err != nil {
				if errors.Is(err, os.ErrNotExist) {
					return nil, fmt.Errorf("%w: symlink %q disappeared during capture", ErrTornWorktree, entryPath)
				}
				return nil, fmt.Errorf("read symlink %q: %w", entryPath, err)
			}
			stored, err := putCapturedObject(ctx, objectStore, budget, entryPath, strings.NewReader(target), int64(len(target)))
			if err != nil {
				return nil, fmt.Errorf("store symlink %q: %w", entryPath, err)
			}
			entry := snapshot.Entry{Path: entryPath, Kind: snapshot.EntryKindSymlink, Mode: uint32(mode), Size: stored.Size, ContentDigest: stored.Digest, SymlinkTarget: target}
			if hasPrior && sameWorktreeContent(prior, entry) {
				entry.GitOID = prior.GitOID
			}
			entries = append(entries, entry)
			continue
		}
		if prior.Kind == snapshot.EntryKindSubmodule {
			return nil, fmt.Errorf("%w: %q is not a Git directory", ErrDirtySubmodule, entryPath)
		}
		file, err := os.Open(fullPath)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				return nil, fmt.Errorf("%w: path %q disappeared during capture", ErrTornWorktree, entryPath)
			}
			return nil, fmt.Errorf("open working-tree file %q: %w", entryPath, err)
		}
		stored, storeErr := putCapturedObject(ctx, objectStore, budget, entryPath, file, info.Size())
		closeErr := file.Close()
		if storeErr != nil {
			return nil, fmt.Errorf("store working-tree file %q: %w", entryPath, storeErr)
		}
		if closeErr != nil {
			return nil, fmt.Errorf("close working-tree file %q: %w", entryPath, closeErr)
		}
		entry := snapshot.Entry{Path: entryPath, Kind: snapshot.EntryKindFile, Mode: uint32(mode), Size: stored.Size, ContentDigest: stored.Digest}
		if hasPrior && sameWorktreeContent(prior, entry) {
			entry.GitOID = prior.GitOID
		}
		entries = append(entries, entry)
	}
	return entries, nil
}

func sameWorktreeContent(left, right snapshot.Entry) bool {
	return left.Kind == right.Kind && left.Mode == right.Mode && left.Size == right.Size &&
		left.ContentDigest != "" && left.ContentDigest == right.ContentDigest && left.SymlinkTarget == right.SymlinkTarget
}

func verifyWorktreeStable(ctx context.Context, repository *Repository, capture snapshot.Capture) error {
	head, err := repository.Git.Head()
	if err != nil {
		return fmt.Errorf("re-read HEAD: %w", err)
	}
	if head.Hash().String() != capture.BaseOID {
		return fmt.Errorf("%w: HEAD changed from %s to %s", ErrTornWorktree, capture.BaseOID, head.Hash())
	}
	indexIdentity, err := readIndexIdentity(repository.Git)
	if err != nil {
		return err
	}
	if indexIdentity != capture.IndexOID {
		return fmt.Errorf("%w: index changed", ErrTornWorktree)
	}
	worktree, err := repository.Git.Worktree()
	if err != nil {
		return fmt.Errorf("re-open Git worktree: %w", err)
	}
	status, err := worktree.StatusWithOptions(git.StatusOptions{Strategy: git.Preload})
	if err != nil {
		return fmt.Errorf("re-read Git worktree status: %w", err)
	}
	currentIndex, err := repository.Git.Storer.Index()
	if err != nil {
		return fmt.Errorf("re-read Git index: %w", err)
	}
	paths, err := worktreeInventory(capture.HeadEntries, currentIndex, status)
	if err != nil {
		return err
	}
	currentPresent := make(map[string]struct{}, len(paths))
	for _, entryPath := range paths {
		fullPath, err := safeWorktreePath(repository.Root, entryPath)
		if err != nil {
			return err
		}
		if _, err := os.Lstat(fullPath); err == nil {
			currentPresent[entryPath] = struct{}{}
		} else if !errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("re-inspect %q: %w", entryPath, err)
		}
	}
	capturedPresent := make(map[string]struct{}, len(capture.WorktreeEntries))
	for _, entry := range capture.WorktreeEntries {
		capturedPresent[entry.Path] = struct{}{}
	}
	if !samePathSet(currentPresent, capturedPresent) {
		return fmt.Errorf("%w: working-tree inventory changed", ErrTornWorktree)
	}
	for _, entry := range capture.WorktreeEntries {
		if err := verifyWorktreeEntry(ctx, repository.Root, entry); err != nil {
			return err
		}
	}
	return nil
}

func samePathSet(left, right map[string]struct{}) bool {
	if len(left) != len(right) {
		return false
	}
	for path := range left {
		if _, ok := right[path]; !ok {
			return false
		}
	}
	return true
}

func verifyWorktreeEntry(ctx context.Context, root string, entry snapshot.Entry) error {
	fullPath, err := safeWorktreePath(root, entry.Path)
	if err != nil {
		return err
	}
	info, err := os.Lstat(fullPath)
	if err != nil {
		return fmt.Errorf("%w: path %q changed: %v", ErrTornWorktree, entry.Path, err)
	}
	if entry.Kind == snapshot.EntryKindSubmodule {
		if err := verifySubmodule(root, entry.Path, entry.GitOID); err != nil {
			return err
		}
		return nil
	}
	mode, err := filemode.NewFromOSFileMode(info.Mode())
	if err != nil || uint32(mode) != entry.Mode {
		return fmt.Errorf("%w: mode for %q changed", ErrTornWorktree, entry.Path)
	}
	if mode == filemode.Symlink {
		target, err := os.Readlink(fullPath)
		if err != nil {
			return fmt.Errorf("%w: symlink %q changed: %v", ErrTornWorktree, entry.Path, err)
		}
		if target != entry.SymlinkTarget || digestBytes([]byte(target)) != entry.ContentDigest {
			return fmt.Errorf("%w: symlink %q content changed", ErrTornWorktree, entry.Path)
		}
		return nil
	}
	file, err := os.Open(fullPath)
	if err != nil {
		return fmt.Errorf("%w: open %q for verification: %v", ErrTornWorktree, entry.Path, err)
	}
	defer file.Close()
	hasher := sha256.New()
	size, err := io.Copy(hasher, file)
	if err != nil {
		return fmt.Errorf("%w: read %q for verification: %v", ErrTornWorktree, entry.Path, err)
	}
	if size != entry.Size || hex.EncodeToString(hasher.Sum(nil)) != entry.ContentDigest {
		return fmt.Errorf("%w: content for %q changed", ErrTornWorktree, entry.Path)
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

func verifySubmodule(root, entryPath, expectedOID string) error {
	fullPath, err := safeWorktreePath(root, entryPath)
	if err != nil {
		return err
	}
	info, err := os.Lstat(fullPath)
	if err != nil {
		return fmt.Errorf("%w: inspect %q: %v", ErrDirtySubmodule, entryPath, err)
	}
	if !info.IsDir() {
		return fmt.Errorf("%w: %q is not a directory; review that repository separately", ErrDirtySubmodule, entryPath)
	}
	nested, err := git.PlainOpen(fullPath)
	if err != nil {
		contents, readErr := os.ReadDir(fullPath)
		if readErr == nil && len(contents) == 0 {
			return nil
		}
		return fmt.Errorf("%w: %q is not initialized cleanly; review that repository separately", ErrDirtySubmodule, entryPath)
	}
	defer nested.Close()
	nestedWorktree, err := nested.Worktree()
	if err != nil {
		return fmt.Errorf("%w: open %q worktree: %v", ErrDirtySubmodule, entryPath, err)
	}
	nestedStatus, err := nestedWorktree.StatusWithOptions(git.StatusOptions{Strategy: git.Preload})
	if err != nil {
		return fmt.Errorf("%w: read %q status: %v", ErrDirtySubmodule, entryPath, err)
	}
	if !nestedStatus.IsClean() {
		return fmt.Errorf("%w: %q has uncommitted changes; review that repository separately", ErrDirtySubmodule, entryPath)
	}
	nestedHead, err := nested.Head()
	if err != nil || nestedHead.Hash().String() != expectedOID {
		return fmt.Errorf("%w: %q HEAD does not match the recorded Git link; review that repository separately", ErrDirtySubmodule, entryPath)
	}
	return nil
}

func safeWorktreePath(root, entryPath string) (string, error) {
	if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
		return "", err
	}
	parts := strings.Split(entryPath, "/")
	current := root
	for index, part := range parts {
		current = filepath.Join(current, filepath.FromSlash(part))
		info, err := os.Lstat(current)
		if errors.Is(err, os.ErrNotExist) {
			return filepath.Join(root, filepath.FromSlash(entryPath)), nil
		}
		if err != nil {
			return "", fmt.Errorf("inspect repository path %q: %w", entryPath, err)
		}
		if index < len(parts)-1 && info.Mode()&os.ModeSymlink != 0 {
			return "", fmt.Errorf("repository path %q traverses a symlink", entryPath)
		}
	}
	return current, nil
}

func parseComparisonRange(expression string) (string, string, string, error) {
	expression = strings.TrimSpace(expression)
	if expression == "" {
		return "", "", "", fmt.Errorf("capture Git range: comparison is empty")
	}
	comparisonKind, err := snapshot.ComparisonKindForComparison(expression)
	if err != nil {
		return "", "", "", fmt.Errorf("capture Git range: %w", err)
	}
	parts := strings.SplitN(expression, "..", 2)
	if comparisonKind == snapshot.ComparisonThreeDot {
		parts = strings.SplitN(expression, "...", 2)
	}
	base, target := strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
	if base == "" || target == "" || strings.ContainsAny(base, " \t\r\n") || strings.ContainsAny(target, " \t\r\n") {
		return "", "", "", fmt.Errorf("capture Git range: invalid comparison %q", expression)
	}
	return base, target, comparisonKind, nil
}

func resolveMergeBase(repository *git.Repository, baseOID, targetOID plumbing.Hash) (string, error) {
	baseCommit, err := repository.CommitObject(baseOID)
	if err != nil {
		return "", fmt.Errorf("read base commit %s: %w", baseOID, err)
	}
	targetCommit, err := repository.CommitObject(targetOID)
	if err != nil {
		return "", fmt.Errorf("read target commit %s: %w", targetOID, err)
	}
	mergeBases, err := baseCommit.MergeBase(targetCommit)
	if err != nil {
		return "", fmt.Errorf("compute common ancestors: %w", err)
	}
	if len(mergeBases) == 0 {
		return "", fmt.Errorf("%w for %s and %s", ErrNoMergeBase, baseOID, targetOID)
	}
	if len(mergeBases) > 1 {
		identifiers := make([]string, 0, len(mergeBases))
		for _, mergeBase := range mergeBases {
			if mergeBase == nil || mergeBase.Hash.IsZero() {
				continue
			}
			identifiers = append(identifiers, mergeBase.Hash.String())
		}
		sort.Strings(identifiers)
		return "", fmt.Errorf("%w for %s and %s: %s", ErrMultipleMergeBases, baseOID, targetOID, strings.Join(identifiers, ", "))
	}
	if mergeBases[0] == nil || mergeBases[0].Hash.IsZero() {
		return "", fmt.Errorf("%w: Git returned an empty object ID", ErrNoMergeBase)
	}
	return mergeBases[0].Hash.String(), nil
}

func resolveCommitRevision(repository *git.Repository, expression string) (plumbing.Hash, error) {
	if err := detectAmbiguousRevision(repository, expression); err != nil {
		return plumbing.ZeroHash, err
	}
	hash, err := repository.ResolveRevision(plumbing.Revision(expression))
	if err != nil {
		return plumbing.ZeroHash, err
	}
	if hash == nil || hash.IsZero() {
		return plumbing.ZeroHash, plumbing.ErrReferenceNotFound
	}
	if _, err := repository.CommitObject(*hash); err != nil {
		return plumbing.ZeroHash, fmt.Errorf("revision does not resolve to a commit: %w", err)
	}
	return *hash, nil
}

func detectAmbiguousRevision(repository *git.Repository, expression string) error {
	if isSimpleRevision(expression) {
		refMatches, err := matchingReferences(repository, expression)
		if err != nil {
			return fmt.Errorf("inspect revision references: %w", err)
		}
		if len(refMatches) > 1 {
			return fmt.Errorf("%w: %q matches %s", ErrAmbiguousRevision, expression, strings.Join(refMatches, ", "))
		}
	}
	format, err := gitObjectFormat(repository)
	if err != nil {
		return err
	}
	if isHexRevision(expression) && len(expression) < formatHexSize(format) {
		iter, err := repository.Storer.IterEncodedObjects(plumbing.CommitObject)
		if err != nil {
			return fmt.Errorf("inspect commit revisions: %w", err)
		}
		defer iter.Close()
		matches := 0
		for {
			object, err := iter.Next()
			if errors.Is(err, io.EOF) {
				break
			}
			if err != nil {
				return fmt.Errorf("inspect commit revisions: %w", err)
			}
			if strings.HasPrefix(object.Hash().String(), strings.ToLower(expression)) {
				matches++
				if matches > 1 {
					return fmt.Errorf("%w: abbreviated object ID %q matches multiple commits", ErrAmbiguousRevision, expression)
				}
			}
		}
	}
	return nil
}

func matchingReferences(repository *git.Repository, expression string) ([]string, error) {
	iter, err := repository.References()
	if err != nil {
		return nil, err
	}
	defer iter.Close()
	candidates := make([]string, 0)
	for {
		reference, err := iter.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, err
		}
		name := reference.Name().String()
		if name == expression || name == "refs/heads/"+expression || name == "refs/tags/"+expression || name == "refs/remotes/"+expression {
			candidates = append(candidates, name)
		}
	}
	sort.Strings(candidates)
	return candidates, nil
}

func isSimpleRevision(expression string) bool {
	return expression != "" && !strings.ContainsAny(expression, "~^:{}")
}

func isHexRevision(expression string) bool {
	if expression == "" {
		return false
	}
	for _, char := range expression {
		if !((char >= '0' && char <= '9') || (char >= 'a' && char <= 'f') || (char >= 'A' && char <= 'F')) {
			return false
		}
	}
	return true
}

func gitObjectFormat(repository *git.Repository) (string, error) {
	config, err := repository.Config()
	if err != nil {
		return "", fmt.Errorf("read Git object format: %w", err)
	}
	format := config.Extensions.ObjectFormat.String()
	if format == "" {
		format = "sha1"
	}
	if format != "sha1" && format != "sha256" {
		return "", fmt.Errorf("read Git object format: unsupported format %q", format)
	}
	return format, nil
}

func formatHexSize(format string) int {
	if format == "sha256" {
		return 64
	}
	return 40
}

func planTree(ctx context.Context, repository *git.Repository, commitOID plumbing.Hash, plan *capturePlan) ([]snapshot.Entry, error) {
	commit, err := repository.CommitObject(commitOID)
	if err != nil {
		return nil, err
	}
	tree, err := commit.Tree()
	if err != nil {
		return nil, err
	}
	entries := make([]snapshot.Entry, 0)
	if err := walkTreePlan(ctx, repository, tree, "", plan, &entries); err != nil {
		return nil, err
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Path < entries[j].Path })
	return entries, nil
}

func walkTreePlan(ctx context.Context, repository *git.Repository, tree *object.Tree, prefix string, plan *capturePlan, entries *[]snapshot.Entry) error {
	for index := range tree.Entries {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}
		treeEntry := tree.Entries[index]
		if treeEntry.Name == "" || treeEntry.Name == "." || treeEntry.Name == ".." || strings.ContainsAny(treeEntry.Name, "/\\\x00") {
			return fmt.Errorf("capture plan: invalid Git tree entry name %q", treeEntry.Name)
		}
		entryPath := treeEntry.Name
		if prefix != "" {
			entryPath = path.Join(prefix, treeEntry.Name)
		}
		if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
			return err
		}
		switch treeEntry.Mode {
		case filemode.Dir:
			child, err := repository.TreeObject(treeEntry.Hash)
			if err != nil {
				return fmt.Errorf("read tree %q: %w", entryPath, err)
			}
			if err := walkTreePlan(ctx, repository, child, entryPath, plan, entries); err != nil {
				return err
			}
		case filemode.Submodule:
			if err := plan.add(entryPath, 0); err != nil {
				return err
			}
			*entries = append(*entries, snapshot.Entry{Path: entryPath, Kind: snapshot.EntryKindSubmodule, Mode: uint32(treeEntry.Mode), GitOID: treeEntry.Hash.String()})
		case filemode.Regular, filemode.Deprecated, filemode.Executable, filemode.Symlink:
			blob, err := repository.BlobObject(treeEntry.Hash)
			if err != nil {
				return fmt.Errorf("read blob %q: %w", entryPath, err)
			}
			if err := plan.add(entryPath, blob.Size); err != nil {
				return err
			}
			kind := snapshot.EntryKindFile
			if treeEntry.Mode == filemode.Symlink {
				kind = snapshot.EntryKindSymlink
			}
			*entries = append(*entries, snapshot.Entry{Path: entryPath, Kind: kind, Mode: uint32(treeEntry.Mode), Size: blob.Size, GitOID: treeEntry.Hash.String()})
		default:
			return fmt.Errorf("capture plan %q: unsupported Git tree mode %s", entryPath, treeEntry.Mode)
		}
	}
	return nil
}

func planIndex(ctx context.Context, repository *git.Repository, gitIndex *index.Index, plan *capturePlan) ([]snapshot.Entry, error) {
	if gitIndex == nil {
		return nil, fmt.Errorf("Git index is nil")
	}
	entries := make([]snapshot.Entry, 0, len(gitIndex.Entries))
	seen := make(map[string]struct{}, len(gitIndex.Entries))
	for _, indexEntry := range gitIndex.Entries {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		default:
		}
		if indexEntry == nil {
			return nil, fmt.Errorf("Git index contains a nil entry")
		}
		if indexEntry.Stage != 0 {
			return nil, fmt.Errorf("Git index contains unresolved merge entries; resolve the index before reviewing")
		}
		if indexEntry.IntentToAdd {
			return nil, fmt.Errorf("Git index contains intent-to-add entry %q; stage the file before reviewing", indexEntry.Name)
		}
		entryPath := filepath.ToSlash(indexEntry.Name)
		if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
			return nil, fmt.Errorf("index entry %q: %w", indexEntry.Name, err)
		}
		if _, ok := seen[entryPath]; ok {
			return nil, fmt.Errorf("Git index contains duplicate entry %q", entryPath)
		}
		seen[entryPath] = struct{}{}
		if indexEntry.Mode == filemode.Submodule {
			if err := plan.add(entryPath, 0); err != nil {
				return nil, err
			}
			entries = append(entries, snapshot.Entry{Path: entryPath, Kind: snapshot.EntryKindSubmodule, Mode: uint32(indexEntry.Mode), GitOID: indexEntry.Hash.String()})
			continue
		}
		if indexEntry.Hash.IsZero() {
			return nil, fmt.Errorf("Git index entry %q has no Git object", entryPath)
		}
		blob, err := repository.BlobObject(indexEntry.Hash)
		if err != nil {
			return nil, fmt.Errorf("read index blob %q: %w", entryPath, err)
		}
		if err := plan.add(entryPath, blob.Size); err != nil {
			return nil, err
		}
		kind := snapshot.EntryKindFile
		if indexEntry.Mode == filemode.Symlink {
			kind = snapshot.EntryKindSymlink
		}
		entries = append(entries, snapshot.Entry{Path: entryPath, Kind: kind, Mode: uint32(indexEntry.Mode), Size: blob.Size, GitOID: indexEntry.Hash.String()})
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Path < entries[j].Path })
	return entries, nil
}

func planWorktree(ctx context.Context, root string, headEntries, indexEntries []snapshot.Entry, paths []string, plan *capturePlan) error {
	expected := make(map[string]snapshot.Entry, len(headEntries)+len(indexEntries))
	for _, entry := range headEntries {
		expected[entry.Path] = entry
	}
	for _, entry := range indexEntries {
		expected[entry.Path] = entry
	}
	for _, entryPath := range paths {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}
		fullPath, err := safeWorktreePath(root, entryPath)
		if err != nil {
			return err
		}
		info, err := os.Lstat(fullPath)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return fmt.Errorf("inspect %q: %w", entryPath, err)
		}
		prior, hasPrior := expected[entryPath]
		if info.IsDir() {
			if !hasPrior || prior.Kind != snapshot.EntryKindSubmodule {
				return fmt.Errorf("working-tree path %q is a directory; only clean submodules may be captured as directories", entryPath)
			}
			if err := verifySubmodule(root, entryPath, prior.GitOID); err != nil {
				return err
			}
			if err := plan.add(entryPath, 0); err != nil {
				return err
			}
			continue
		}
		if !info.Mode().IsRegular() && info.Mode()&os.ModeSymlink == 0 {
			return fmt.Errorf("working-tree path %q is a special device or unsupported filesystem entry", entryPath)
		}
		mode, err := filemode.NewFromOSFileMode(info.Mode())
		if err != nil {
			return fmt.Errorf("working-tree path %q: unsupported file type: %w", entryPath, err)
		}
		if hasPrior && prior.Kind == snapshot.EntryKindSubmodule {
			return fmt.Errorf("%w: %q is not a Git directory", ErrDirtySubmodule, entryPath)
		}
		var size int64
		if mode == filemode.Symlink {
			target, err := os.Readlink(fullPath)
			if err != nil {
				return fmt.Errorf("read symlink %q: %w", entryPath, err)
			}
			size = int64(len(target))
		} else {
			size = info.Size()
		}
		if err := plan.add(entryPath, size); err != nil {
			return err
		}
	}
	return nil
}

func captureTree(ctx context.Context, repository *git.Repository, commitOID plumbing.Hash, objectStore *snapshot.ObjectStore) ([]snapshot.Entry, error) {
	limits, _ := normalizeCaptureLimits(CaptureLimits{})
	return captureTreeWithBudget(ctx, repository, commitOID, objectStore, newCaptureBudget(limits))
}

func captureTreeWithBudget(ctx context.Context, repository *git.Repository, commitOID plumbing.Hash, objectStore *snapshot.ObjectStore, budget *captureBudget) ([]snapshot.Entry, error) {
	commit, err := repository.CommitObject(commitOID)
	if err != nil {
		return nil, err
	}
	tree, err := commit.Tree()
	if err != nil {
		return nil, err
	}
	entries := make([]snapshot.Entry, 0)
	if err := walkTreeWithBudget(ctx, repository, tree, "", objectStore, budget, &entries); err != nil {
		return nil, err
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Path < entries[j].Path })
	return entries, nil
}

func walkTree(ctx context.Context, repository *git.Repository, tree *object.Tree, prefix string, objectStore *snapshot.ObjectStore, entries *[]snapshot.Entry) error {
	limits, _ := normalizeCaptureLimits(CaptureLimits{})
	return walkTreeWithBudget(ctx, repository, tree, prefix, objectStore, newCaptureBudget(limits), entries)
}

func walkTreeWithBudget(ctx context.Context, repository *git.Repository, tree *object.Tree, prefix string, objectStore *snapshot.ObjectStore, budget *captureBudget, entries *[]snapshot.Entry) error {
	for index := range tree.Entries {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}
		treeEntry := tree.Entries[index]
		if treeEntry.Name == "" || treeEntry.Name == "." || treeEntry.Name == ".." ||
			strings.ContainsAny(treeEntry.Name, "/\\\x00") {
			return fmt.Errorf("capture: invalid Git tree entry name %q", treeEntry.Name)
		}
		entryPath := treeEntry.Name
		if prefix != "" {
			entryPath = path.Join(prefix, treeEntry.Name)
		}
		if err := snapshot.ValidateRepositoryPath(entryPath); err != nil {
			return err
		}
		switch treeEntry.Mode {
		case filemode.Dir:
			child, err := repository.TreeObject(treeEntry.Hash)
			if err != nil {
				return fmt.Errorf("read tree %q: %w", entryPath, err)
			}
			if err := walkTreeWithBudget(ctx, repository, child, entryPath, objectStore, budget, entries); err != nil {
				return err
			}
		case filemode.Submodule:
			if _, err := budget.reserve(entryPath, 0); err != nil {
				return err
			}
			*entries = append(*entries, snapshot.Entry{
				Path:   entryPath,
				Kind:   snapshot.EntryKindSubmodule,
				Mode:   uint32(treeEntry.Mode),
				GitOID: treeEntry.Hash.String(),
			})
		case filemode.Regular, filemode.Deprecated, filemode.Executable, filemode.Symlink:
			blob, err := repository.BlobObject(treeEntry.Hash)
			if err != nil {
				return fmt.Errorf("read blob %q: %w", entryPath, err)
			}
			reader, err := blob.Reader()
			if err != nil {
				return fmt.Errorf("open blob %q: %w", entryPath, err)
			}
			stored, storeErr := putCapturedObject(ctx, objectStore, budget, entryPath, reader, int64(blob.Size))
			closeErr := reader.Close()
			if storeErr != nil {
				return fmt.Errorf("store blob %q: %w", entryPath, storeErr)
			}
			if closeErr != nil {
				return fmt.Errorf("close blob %q: %w", entryPath, closeErr)
			}
			entry := snapshot.Entry{
				Path:          entryPath,
				Kind:          snapshot.EntryKindFile,
				Mode:          uint32(treeEntry.Mode),
				Size:          stored.Size,
				ContentDigest: stored.Digest,
				GitOID:        treeEntry.Hash.String(),
			}
			if treeEntry.Mode == filemode.Symlink {
				entry.Kind = snapshot.EntryKindSymlink
				content, err := readStoredObject(objectStore, stored.Digest)
				if err != nil {
					return fmt.Errorf("read symlink target %q: %w", entryPath, err)
				}
				entry.SymlinkTarget = string(content)
			}
			*entries = append(*entries, entry)
		default:
			return fmt.Errorf("capture %q: unsupported Git tree mode %s", entryPath, treeEntry.Mode)
		}
	}
	return nil
}

func readStoredObject(store *snapshot.ObjectStore, digest string) ([]byte, error) {
	file, err := store.Open(digest)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	return io.ReadAll(file)
}

func canonicalPath(value string) (string, error) {
	absolute, err := filepath.Abs(value)
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(absolute)
	if err != nil {
		return "", err
	}
	return filepath.Clean(resolved), nil
}
