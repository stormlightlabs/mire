// Package gitrepo provides read-only Git discovery and committed snapshot capture.
// It relies heavily on go-git ([git]).
package gitrepo

import (
	"context"
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

// CaptureOptions controls deterministic capture metadata.
type CaptureOptions struct {
	Clock      func() time.Time
	PolicyHash string
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
	baseEntries, err := captureTree(ctx, repository.Git, effectiveBaseOID, objectStore)
	if err != nil {
		return snapshot.Capture{}, fmt.Errorf("capture effective base tree %s: %w", effectiveBaseOID, err)
	}
	targetEntries, err := captureTree(ctx, repository.Git, targetOID, objectStore)
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

func captureTree(ctx context.Context, repository *git.Repository, commitOID plumbing.Hash, objectStore *snapshot.ObjectStore) ([]snapshot.Entry, error) {
	commit, err := repository.CommitObject(commitOID)
	if err != nil {
		return nil, err
	}
	tree, err := commit.Tree()
	if err != nil {
		return nil, err
	}
	entries := make([]snapshot.Entry, 0)
	if err := walkTree(ctx, repository, tree, "", objectStore, &entries); err != nil {
		return nil, err
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Path < entries[j].Path })
	return entries, nil
}

func walkTree(ctx context.Context, repository *git.Repository, tree *object.Tree, prefix string, objectStore *snapshot.ObjectStore, entries *[]snapshot.Entry) error {
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
			if err := walkTree(ctx, repository, child, entryPath, objectStore, entries); err != nil {
				return err
			}
		case filemode.Submodule:
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
			stored, storeErr := objectStore.Put(ctx, reader)
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
