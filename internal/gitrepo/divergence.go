package gitrepo

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

// CheckDivergence compares a persisted snapshot with the current repository.
// It reports live Git problems as unavailable or unsupported statuses so a
// caller never has to mistake an inconclusive check for an unchanged round.
func CheckDivergence(ctx context.Context, directory string, store *db.RepositoryStore, frozen db.Snapshot, objectStore *snapshot.ObjectStore) (snapshot.DivergenceReport, error) {
	if strings.TrimSpace(frozen.ID) == "" {
		return snapshot.DivergenceReport{}, fmt.Errorf("check divergence: snapshot ID is empty")
	}
	kind, err := snapshot.ComparisonKindForComparison(frozen.RequestedComparison)
	if err != nil || kind != frozen.Kind {
		return snapshot.DivergenceReport{
			SnapshotID: frozen.ID, Status: snapshot.DivergenceUnsupported,
			Message: fmt.Sprintf("Snapshot comparison %q is not supported.", frozen.RequestedComparison),
		}, nil
	}
	if frozen.ObjectFormat != "sha1" && frozen.ObjectFormat != "sha256" {
		return snapshot.DivergenceReport{
			SnapshotID: frozen.ID, Status: snapshot.DivergenceUnsupported,
			Message: fmt.Sprintf("Snapshot uses unsupported Git object format %q.", frozen.ObjectFormat),
		}, nil
	}
	if ctx == nil {
		ctx = context.Background()
	}
	repository, err := Open(ctx, directory)
	if err != nil {
		return unavailableReport(frozen.ID, fmt.Sprintf("Unable to open the live Git repository: %v.", err)), nil
	}
	defer repository.Git.Close()
	objectFormat, err := gitObjectFormat(repository.Git)
	if err != nil {
		return unavailableReport(frozen.ID, fmt.Sprintf("Unable to read the live Git object format: %v.", err)), nil
	}
	if objectFormat != frozen.ObjectFormat {
		return snapshot.DivergenceReport{
			SnapshotID: frozen.ID, Status: snapshot.DivergenceUnsupported,
			Message: fmt.Sprintf("Live Git object format %q differs from frozen format %q.", objectFormat, frozen.ObjectFormat),
		}, nil
	}

	if kind == snapshot.ComparisonWorktree {
		return checkWorktreeDivergence(ctx, repository, store, frozen, objectStore)
	}
	baseRevision, targetRevision, _, err := parseComparisonRange(frozen.RequestedComparison)
	if err != nil {
		return snapshot.DivergenceReport{SnapshotID: frozen.ID, Status: snapshot.DivergenceUnsupported, Message: "Frozen comparison syntax is unsupported."}, nil
	}
	baseOID, err := resolveCommitRevision(repository.Git, baseRevision)
	if err != nil {
		return unavailableReport(frozen.ID, fmt.Sprintf("Unable to resolve the frozen base revision %q: %v.", baseRevision, err)), nil
	}
	targetOID, err := resolveCommitRevision(repository.Git, targetRevision)
	if err != nil {
		return unavailableReport(frozen.ID, fmt.Sprintf("Unable to resolve the frozen target revision %q: %v.", targetRevision, err)), nil
	}
	mergeBaseOID := ""
	if kind == snapshot.ComparisonThreeDot {
		mergeBaseOID, err = resolveMergeBase(repository.Git, baseOID, targetOID)
		if err != nil {
			return unavailableReport(frozen.ID, fmt.Sprintf("Unable to resolve the live merge base: %v.", err)), nil
		}
	}

	report := snapshot.DivergenceReport{SnapshotID: frozen.ID, Status: snapshot.DivergenceUnchanged}
	if kind == snapshot.ComparisonThreeDot {
		if baseOID.String() != frozen.BaseOID {
			report.AffectedRefs = append(report.AffectedRefs, "base")
		}
		if targetOID.String() != frozen.TargetOID {
			report.AffectedRefs = append(report.AffectedRefs, "target")
		}
		if mergeBaseOID != frozen.MergeBaseOID {
			report.AffectedRefs = append(report.AffectedRefs, "merge-base")
		}
	} else {
		if baseOID.String() != frozen.EffectiveBaseOID {
			report.AffectedRefs = append(report.AffectedRefs, "base")
		}
		if targetOID.String() != frozen.TargetOID {
			report.AffectedRefs = append(report.AffectedRefs, "target")
		}
	}
	if len(report.AffectedRefs) == 0 {
		return report, nil
	}
	report.Status = snapshot.DivergenceChanged
	report.Message = "Live revisions differ from the frozen round."
	if store != nil && objectStore != nil {
		current, captureErr := CaptureRange(ctx, directory, frozen.RequestedComparison, objectStore)
		if captureErr == nil {
			report.AffectedPaths = changedPaths(ctx, store, frozen, current, snapshot.TreeSideBase, snapshot.TreeSideTarget)
		} else if !errors.Is(captureErr, context.Canceled) && !errors.Is(captureErr, context.DeadlineExceeded) {
			report.Message += fmt.Sprintf(" Changed paths could not be determined: %v.", captureErr)
		}
	}
	return report, nil
}

// CompareDivergence is an alias for callers that prefer comparison-oriented
// naming.
func CompareDivergence(ctx context.Context, directory string, store *db.RepositoryStore, frozen db.Snapshot, objectStore *snapshot.ObjectStore) (snapshot.DivergenceReport, error) {
	return CheckDivergence(ctx, directory, store, frozen, objectStore)
}

func checkWorktreeDivergence(ctx context.Context, repository *Repository, store *db.RepositoryStore, frozen db.Snapshot, objectStore *snapshot.ObjectStore) (snapshot.DivergenceReport, error) {
	if objectStore == nil {
		return snapshot.DivergenceReport{
			SnapshotID: frozen.ID, Status: snapshot.DivergenceUnsupported,
			Message: "Working-tree divergence requires a private object store.",
		}, nil
	}
	current, err := CaptureWorktree(ctx, repository.Root, objectStore)
	if err != nil {
		return unavailableReport(frozen.ID, fmt.Sprintf("Unable to capture the live working tree coherently: %v.", err)), nil
	}
	report := snapshot.DivergenceReport{SnapshotID: frozen.ID, Status: snapshot.DivergenceUnchanged}
	if current.BaseOID != frozen.BaseOID {
		report.AffectedRefs = append(report.AffectedRefs, "HEAD")
	}
	if current.IndexOID != frozen.IndexOID {
		report.AffectedRefs = append(report.AffectedRefs, "index")
	}
	if current.WorktreeOID != frozen.TargetOID {
		report.AffectedRefs = append(report.AffectedRefs, "worktree")
	}
	if len(report.AffectedRefs) == 0 {
		return report, nil
	}
	report.Status = snapshot.DivergenceChanged
	report.Message = "Live working-tree layers differ from the frozen round."
	if store != nil {
		report.AffectedPaths = changedPaths(ctx, store, frozen, current, snapshot.TreeSideHead, snapshot.TreeSideWorktree)
	}
	return report, nil
}

func unavailableReport(snapshotID, message string) snapshot.DivergenceReport {
	return snapshot.DivergenceReport{SnapshotID: snapshotID, Status: snapshot.DivergenceUnavailable, Message: message}
}

func changedPaths(ctx context.Context, store *db.RepositoryStore, frozen db.Snapshot, current snapshot.Capture, baseSide, targetSide string) []string {
	oldBase, err := store.ListSnapshotEntries(ctx, frozen.ID, baseSide)
	if err != nil {
		return nil
	}
	oldTarget, err := store.ListSnapshotEntries(ctx, frozen.ID, targetSide)
	if err != nil {
		return nil
	}
	var currentBase, currentTarget []snapshot.Entry
	if frozen.Kind == snapshot.ComparisonWorktree {
		currentBase, currentTarget = current.HeadEntries, current.WorktreeEntries
	} else {
		currentBase, currentTarget = current.BaseEntries, current.TargetEntries
	}
	paths := make(map[string]struct{})
	for _, path := range changedEntryPaths(oldBase, currentBase) {
		paths[path] = struct{}{}
	}
	for _, path := range changedEntryPaths(oldTarget, currentTarget) {
		paths[path] = struct{}{}
	}
	result := make([]string, 0, len(paths))
	for path := range paths {
		result = append(result, path)
	}
	sort.Strings(result)
	return result
}

func changedEntryPaths(oldEntries []db.SnapshotEntry, currentEntries []snapshot.Entry) []string {
	oldByPath := make(map[string]db.SnapshotEntry, len(oldEntries))
	for _, entry := range oldEntries {
		oldByPath[entry.Path] = entry
	}
	currentByPath := make(map[string]snapshot.Entry, len(currentEntries))
	for _, entry := range currentEntries {
		currentByPath[entry.Path] = entry
	}
	paths := make([]string, 0)
	for path, oldEntry := range oldByPath {
		currentEntry, ok := currentByPath[path]
		if !ok || !sameEntry(oldEntry, currentEntry) {
			paths = append(paths, path)
		}
	}
	for path := range currentByPath {
		if _, ok := oldByPath[path]; !ok {
			paths = append(paths, path)
		}
	}
	return paths
}

func sameEntry(old db.SnapshotEntry, current snapshot.Entry) bool {
	return old.Kind == current.Kind && old.Mode == current.Mode && old.Size == current.Size &&
		old.ContentDigest == current.ContentDigest && old.GitOID == current.GitOID &&
		old.SymlinkTarget == current.SymlinkTarget
}
