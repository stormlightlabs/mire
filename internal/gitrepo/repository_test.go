package gitrepo

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	git "github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/filemode"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestCaptureWorktreePreservesHeadIndexAndFinalLayers(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "shared.txt", "head\n", 0o644)
	writeFile(t, repositoryPath, "deleted.txt", "delete me\n", 0o644)
	writeFile(t, repositoryPath, "script.sh", "#!/bin/sh\nexit 0\n", 0o755)
	writeFile(t, repositoryPath, ".gitignore", "ignored/\nignored.log\n", 0o644)
	if err := os.Mkdir(filepath.Join(repositoryPath, "ignored"), 0o755); err != nil {
		t.Fatalf("create ignored directory: %v", err)
	}
	if err := os.Symlink("shared.txt", filepath.Join(repositoryPath, "link")); err != nil {
		t.Fatalf("create symlink: %v", err)
	}
	addFiles(t, worktree, "shared.txt", "deleted.txt", "script.sh", ".gitignore", "link")
	commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))

	writeFile(t, repositoryPath, "shared.txt", "index\n", 0o644)
	if _, err := worktree.Add("shared.txt"); err != nil {
		t.Fatalf("stage shared file: %v", err)
	}
	writeFile(t, repositoryPath, "shared.txt", "final\n", 0o644)
	if err := os.Remove(filepath.Join(repositoryPath, "deleted.txt")); err != nil {
		t.Fatalf("delete file: %v", err)
	}
	writeFile(t, repositoryPath, "new name-ユニコード.bin", "\\x00\\x01binary\\xff", 0o644)
	writeFile(t, repositoryPath, "ignored.log", "ignored\n", 0o644)
	writeFile(t, filepath.Join(repositoryPath, "ignored"), "nested.txt", "ignored\n", 0o644)

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	captured, err := CaptureWorktreeWithOptions(context.Background(), repositoryPath, objectStore, CaptureOptions{
		Clock:       func() time.Time { return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC) },
		MaxAttempts: 1,
	})
	if err != nil {
		t.Fatalf("CaptureWorktree() error = %v", err)
	}
	if captured.ComparisonKind != snapshot.ComparisonWorktree || captured.RequestedComparison != snapshot.WorktreeComparison {
		t.Fatalf("comparison = %#v", captured)
	}
	if captured.BaseOID == "" || captured.IndexOID == "" || captured.WorktreeOID == "" || captured.BaseOID == captured.IndexOID || captured.IndexOID == captured.WorktreeOID {
		t.Fatalf("layer identities = head %q index %q worktree %q", captured.BaseOID, captured.IndexOID, captured.WorktreeOID)
	}
	if len(captured.HeadEntries) != 5 || len(captured.IndexEntries) != 5 {
		t.Fatalf("head/index entry counts = %d/%d, want 5/5", len(captured.HeadEntries), len(captured.IndexEntries))
	}
	if findEntry(captured.WorktreeEntries, "ignored.log").Path != "" || findEntry(captured.WorktreeEntries, "ignored/nested.txt").Path != "" {
		t.Fatalf("ignored entries were captured: %#v", captured.WorktreeEntries)
	}
	if findEntry(captured.WorktreeEntries, "new name-ユニコード.bin").Path == "" {
		t.Fatalf("untracked Unicode path was not captured: %#v", captured.WorktreeEntries)
	}
	if findEntry(captured.WorktreeEntries, "deleted.txt").Path != "" {
		t.Fatalf("deleted path was captured in final layer: %#v", captured.WorktreeEntries)
	}
	if got := string(readObject(t, objectStore, findEntry(captured.WorktreeEntries, "shared.txt").ContentDigest)); got != "final\n" {
		t.Fatalf("final shared content = %q", got)
	}
	if got := string(readObject(t, objectStore, findEntry(captured.IndexEntries, "shared.txt").ContentDigest)); got != "index\n" {
		t.Fatalf("index shared content = %q", got)
	}
	if link := findEntry(captured.WorktreeEntries, "link"); link.Kind != snapshot.EntryKindSymlink || link.SymlinkTarget != "shared.txt" {
		t.Fatalf("symlink entry = %#v", link)
	}
	if script := findEntry(captured.WorktreeEntries, "script.sh"); script.Mode != 0o100755 {
		t.Fatalf("executable mode = %o, want 100755", script.Mode)
	}
	if !hasChange(captured.Changes, snapshot.ChangeModified, "shared.txt", "shared.txt") ||
		!hasChange(captured.Changes, snapshot.ChangeDeleted, "deleted.txt", "") ||
		!hasChange(captured.Changes, snapshot.ChangeAdded, "", "new name-ユニコード.bin") {
		t.Fatalf("working-tree changes = %#v", captured.Changes)
	}
}

func readObject(t *testing.T, store *snapshot.ObjectStore, digest string) []byte {
	t.Helper()
	file, err := store.Open(digest)
	if err != nil {
		t.Fatalf("open object %q: %v", digest, err)
	}
	content, err := io.ReadAll(file)
	_ = file.Close()
	if err != nil {
		t.Fatalf("read object %q: %v", digest, err)
	}
	return content
}

func TestCaptureWorktreeRejectsDirtySubmodule(t *testing.T) {
	t.Parallel()

	parentPath := t.TempDir()
	parent := initRepository(t, parentPath)
	parentWorktree, err := parent.Worktree()
	if err != nil {
		t.Fatalf("parent Worktree() error = %v", err)
	}
	nestedPath := filepath.Join(parentPath, "nested")
	nested := initRepository(t, nestedPath)
	nestedWorktree, err := nested.Worktree()
	if err != nil {
		t.Fatalf("nested Worktree() error = %v", err)
	}
	writeFile(t, nestedPath, "nested.txt", "clean\n", 0o644)
	addFiles(t, nestedWorktree, "nested.txt")
	nestedOID := commit(t, nested, nestedWorktree, "nested base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	parentIndex, err := parent.Storer.Index()
	if err != nil {
		t.Fatalf("read parent index: %v", err)
	}
	submoduleEntry, err := parentIndex.Add("nested")
	if err != nil {
		t.Fatalf("add submodule index entry: %v", err)
	}
	submoduleEntry.Hash = nestedOID
	submoduleEntry.Mode = filemode.Submodule
	if err := parent.Storer.SetIndex(parentIndex); err != nil {
		t.Fatalf("write parent index: %v", err)
	}
	commit(t, parent, parentWorktree, "record submodule", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))
	writeFile(t, nestedPath, "nested.txt", "dirty\n", 0o644)

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	_, err = CaptureWorktreeWithOptions(context.Background(), parentPath, objectStore, CaptureOptions{MaxAttempts: 1})
	if !errors.Is(err, ErrDirtySubmodule) {
		t.Fatalf("CaptureWorktree() error = %v, want ErrDirtySubmodule", err)
	}
}

func TestCaptureRangeStoresCompleteTreesAndDurableChanges(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "unchanged.txt", "unchanged\n", 0o644)
	writeFile(t, repositoryPath, "old.txt", "renamed content\n", 0o644)
	writeFile(t, repositoryPath, "delete.txt", "delete me\n", 0o644)
	writeFile(t, repositoryPath, "script.sh", "#!/bin/sh\nexit 0\n", 0o755)
	if err := os.Symlink("unchanged.txt", filepath.Join(repositoryPath, "link")); err != nil {
		t.Fatalf("create symlink: %v", err)
	}
	addFiles(t, worktree, "unchanged.txt", "old.txt", "delete.txt", "script.sh", "link")
	base := commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))

	if err := os.Rename(filepath.Join(repositoryPath, "old.txt"), filepath.Join(repositoryPath, "renamed.txt")); err != nil {
		t.Fatalf("rename file: %v", err)
	}
	if err := os.Remove(filepath.Join(repositoryPath, "delete.txt")); err != nil {
		t.Fatalf("remove file: %v", err)
	}
	writeFile(t, repositoryPath, "new.txt", "new content\n", 0o644)
	writeFile(t, repositoryPath, "script.sh", "#!/bin/sh\nexit 1\n", 0o755)
	if _, err := worktree.Add("renamed.txt"); err != nil {
		t.Fatalf("add renamed file: %v", err)
	}
	if _, err := worktree.Remove("old.txt"); err != nil {
		t.Fatalf("remove renamed source from index: %v", err)
	}
	if _, err := worktree.Remove("delete.txt"); err != nil {
		t.Fatalf("remove deleted file from index: %v", err)
	}
	addFiles(t, worktree, "new.txt", "script.sh")
	target := commit(t, repository, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	captured, err := CaptureRangeWithOptions(context.Background(), repositoryPath,
		base.String()+".."+target.String(), objectStore, CaptureOptions{
			Clock: func() time.Time { return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC) },
		})
	if err != nil {
		t.Fatalf("CaptureRange() error = %v", err)
	}
	if captured.EffectiveBaseOID != base.String() || captured.TargetOID != target.String() {
		t.Fatalf("resolved IDs = %s..%s, want %s..%s", captured.EffectiveBaseOID, captured.TargetOID, base, target)
	}
	if len(captured.BaseEntries) != 5 || len(captured.TargetEntries) != 5 {
		t.Fatalf("complete entry counts = %d/%d, want 5/5", len(captured.BaseEntries), len(captured.TargetEntries))
	}
	if captured.ObjectFormat != "sha1" || captured.ContextPolicyHash == "" || captured.ManifestDigest == "" {
		t.Fatalf("capture provenance = %#v", captured)
	}
	if !hasChange(captured.Changes, snapshot.ChangeRenamed, "old.txt", "renamed.txt") ||
		!hasChange(captured.Changes, snapshot.ChangeDeleted, "delete.txt", "") ||
		!hasChange(captured.Changes, snapshot.ChangeAdded, "", "new.txt") {
		t.Fatalf("changes = %#v", captured.Changes)
	}
	link := findEntry(captured.BaseEntries, "link")
	if link.Kind != snapshot.EntryKindSymlink || link.SymlinkTarget != "unchanged.txt" || link.ContentDigest == "" {
		t.Fatalf("symlink entry = %#v", link)
	}
	script := findEntry(captured.TargetEntries, "script.sh")
	if script.Mode != 0o100755 {
		t.Fatalf("executable mode = %o, want 100755", script.Mode)
	}
	file, err := objectStore.Open(findEntry(captured.TargetEntries, "new.txt").ContentDigest)
	if err != nil {
		t.Fatalf("open captured object: %v", err)
	}
	content, err := io.ReadAll(file)
	_ = file.Close()
	if err != nil || string(content) != "new content\n" {
		t.Fatalf("captured object = %q, error=%v", content, err)
	}
}

func TestCaptureRangeThreeDotUsesUniqueMergeBaseAndFreezesRefs(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "common.txt", "common\n", 0o644)
	addFiles(t, worktree, "common.txt")
	common := commit(t, repository, worktree, "common", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	mainBranch, err := repository.Head()
	if err != nil {
		t.Fatalf("Head() error = %v", err)
	}
	if err := repository.Storer.SetReference(plumbing.NewHashReference(
		plumbing.NewBranchReferenceName("base"), common)); err != nil {
		t.Fatalf("create base branch: %v", err)
	}
	if err := worktree.Checkout(&git.CheckoutOptions{Branch: mainBranch.Name(), Force: true}); err != nil {
		t.Fatalf("checkout main branch: %v", err)
	}
	writeFile(t, repositoryPath, "target.txt", "target\n", 0o644)
	addFiles(t, worktree, "target.txt")
	target := commit(t, repository, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	requested := "base..." + mainBranch.Name().Short()
	captured, err := CaptureRangeWithOptions(context.Background(), repositoryPath, requested, objectStore, CaptureOptions{
		Clock: func() time.Time { return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("CaptureRange() error = %v", err)
	}
	if captured.ComparisonKind != snapshot.ComparisonThreeDot || captured.RequestedComparison != requested {
		t.Fatalf("comparison provenance = %#v", captured)
	}
	if captured.BaseOID != common.String() || captured.TargetOID != target.String() ||
		captured.EffectiveBaseOID != common.String() || captured.MergeBaseOID != common.String() {
		t.Fatalf("resolved IDs = base %s effective %s target %s merge-base %s, want %s %s %s %s",
			captured.BaseOID, captured.EffectiveBaseOID, captured.TargetOID, captured.MergeBaseOID,
			common, common, target, common)
	}
	if findEntry(captured.BaseEntries, "target.txt").Path != "" || findEntry(captured.TargetEntries, "target.txt").Path == "" {
		t.Fatalf("three-dot trees = base %#v target %#v", captured.BaseEntries, captured.TargetEntries)
	}
	if !hasChange(captured.Changes, snapshot.ChangeAdded, "", "target.txt") {
		t.Fatalf("three-dot changes = %#v", captured.Changes)
	}

	writeFile(t, repositoryPath, "target.txt", "moved after capture\n", 0o644)
	addFiles(t, worktree, "target.txt")
	commit(t, repository, worktree, "moved ref", time.Date(2026, time.July, 14, 13, 0, 0, 0, time.UTC))
	if captured.TargetOID != target.String() {
		t.Fatalf("captured target OID changed after ref movement = %q, want %s", captured.TargetOID, target)
	}
}

func TestCaptureRangeThreeDotRejectsMissingMergeBase(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "common.txt", "common\n", 0o644)
	addFiles(t, worktree, "common.txt")
	common := commit(t, repository, worktree, "common", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	if err := repository.Storer.SetReference(plumbing.NewHashReference(
		plumbing.NewBranchReferenceName("base"), common)); err != nil {
		t.Fatalf("create base branch: %v", err)
	}
	head, err := repository.Head()
	if err != nil {
		t.Fatalf("Head() error = %v", err)
	}
	if err := repository.Storer.RemoveReference(head.Name()); err != nil {
		t.Fatalf("remove main branch for orphan commit: %v", err)
	}
	writeFile(t, repositoryPath, "orphan.txt", "orphan\n", 0o644)
	addFiles(t, worktree, "orphan.txt")
	if _, err := worktree.Commit("orphan", &git.CommitOptions{
		Parents: nil,
		Author:  &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC)},
	}); err != nil {
		t.Fatalf("create orphan commit: %v", err)
	}

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	_, err = CaptureRange(context.Background(), repositoryPath, "base...HEAD", objectStore)
	if !errors.Is(err, ErrNoMergeBase) {
		t.Fatalf("CaptureRange() error = %v, want ErrNoMergeBase", err)
	}
}

func TestCaptureRangeThreeDotRejectsMultipleMergeBases(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "common.txt", "common\n", 0o644)
	addFiles(t, worktree, "common.txt")
	common := commit(t, repository, worktree, "common", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	mainBranch, err := repository.Head()
	if err != nil {
		t.Fatalf("Head() error = %v", err)
	}
	if err := repository.Storer.SetReference(plumbing.NewHashReference(
		plumbing.NewBranchReferenceName("side"), common)); err != nil {
		t.Fatalf("create side branch: %v", err)
	}
	writeFile(t, repositoryPath, "base.txt", "base\n", 0o644)
	addFiles(t, worktree, "base.txt")
	baseTip := commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))
	if err := worktree.Checkout(&git.CheckoutOptions{Branch: plumbing.NewBranchReferenceName("side"), Force: true}); err != nil {
		t.Fatalf("checkout side branch: %v", err)
	}
	writeFile(t, repositoryPath, "side.txt", "side\n", 0o644)
	addFiles(t, worktree, "side.txt")
	sideTip := commit(t, repository, worktree, "side", time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC))
	if err := worktree.Checkout(&git.CheckoutOptions{Branch: mainBranch.Name(), Force: true}); err != nil {
		t.Fatalf("checkout main branch: %v", err)
	}
	firstMerge := commitWithParents(t, worktree, "first cross merge", []plumbing.Hash{baseTip, sideTip}, time.Date(2026, time.July, 14, 13, 0, 0, 0, time.UTC))
	if err := worktree.Checkout(&git.CheckoutOptions{Branch: plumbing.NewBranchReferenceName("side"), Force: true}); err != nil {
		t.Fatalf("checkout side branch for second merge: %v", err)
	}
	secondMerge := commitWithParents(t, worktree, "second cross merge", []plumbing.Hash{sideTip, baseTip}, time.Date(2026, time.July, 14, 14, 0, 0, 0, time.UTC))
	if firstMerge == secondMerge {
		t.Fatal("cross merges have identical object IDs")
	}

	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	_, err = CaptureRange(context.Background(), repositoryPath, mainBranch.Name().Short()+"...side", objectStore)
	if !errors.Is(err, ErrMultipleMergeBases) {
		t.Fatalf("CaptureRange() error = %v, want ErrMultipleMergeBases", err)
	}
}

func TestCaptureRangeRejectsAmbiguousBranchAndTag(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "file.txt", "content\n", 0o644)
	addFiles(t, worktree, "file.txt")
	commitOID := commit(t, repository, worktree, "commit", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	if err := repository.Storer.SetReference(plumbing.NewHashReference(
		plumbing.NewBranchReferenceName("same"), commitOID)); err != nil {
		t.Fatalf("create branch: %v", err)
	}
	if _, err := repository.CreateTag("same", commitOID, nil); err != nil {
		t.Fatalf("CreateTag() error = %v", err)
	}
	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	_, err = CaptureRange(context.Background(), repositoryPath, "same..HEAD", objectStore)
	if !errors.Is(err, ErrAmbiguousRevision) {
		t.Fatalf("CaptureRange() error = %v, want ErrAmbiguousRevision", err)
	}
}

func TestCaptureRangeRejectsConfiguredResourceLimitsBeforeCopying(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "base.txt", "base", 0o644)
	addFiles(t, worktree, "base.txt")
	base := commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	writeFile(t, repositoryPath, "base.txt", "target-content", 0o644)
	writeFile(t, repositoryPath, "new.txt", "new", 0o644)
	addFiles(t, worktree, "base.txt", "new.txt")
	target := commit(t, repository, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	tests := []struct {
		name     string
		limits   CaptureLimits
		resource string
	}{
		{name: "file count", limits: CaptureLimits{MaxFileCount: 1, MaxObjectBytes: 1024, MaxCapturedBytes: 1024}, resource: "file count"},
		{name: "object size", limits: CaptureLimits{MaxFileCount: 10, MaxObjectBytes: 4, MaxCapturedBytes: 1024}, resource: "individual object bytes"},
		{name: "aggregate bytes", limits: CaptureLimits{MaxFileCount: 10, MaxObjectBytes: 1024, MaxCapturedBytes: 10}, resource: "aggregate captured bytes"},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			stateDir := t.TempDir()
			objectStore, err := snapshot.OpenObjectStore(stateDir)
			if err != nil {
				t.Fatalf("OpenObjectStore() error = %v", err)
			}
			_, err = CaptureRangeWithOptions(context.Background(), repositoryPath, base.String()+".."+target.String(), objectStore, CaptureOptions{Limits: test.limits})
			var limitErr *CaptureLimitError
			if !errors.As(err, &limitErr) || limitErr.Resource != test.resource {
				t.Fatalf("CaptureRange() error = %v, want %q limit", err, test.resource)
			}
			if !errors.Is(err, ErrCaptureLimit) {
				t.Fatalf("CaptureRange() error = %v, want ErrCaptureLimit", err)
			}
			entries, err := os.ReadDir(filepath.Join(stateDir, "objects", "sha256"))
			if err != nil {
				t.Fatalf("ReadDir() error = %v", err)
			}
			if len(entries) != 0 {
				t.Fatalf("resource planning created object shards = %v", entries)
			}
		})
	}
}

func TestCaptureWorktreeRetriesAfterAConcurrentChange(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "file.txt", "before", 0o644)
	addFiles(t, worktree, "file.txt")
	commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))

	var clockCalls int
	objectStore, err := snapshot.OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	captured, err := CaptureWorktreeWithOptions(context.Background(), repositoryPath, objectStore, CaptureOptions{
		MaxAttempts: 2,
		Clock: func() time.Time {
			clockCalls++
			if clockCalls == 1 {
				writeFile(t, repositoryPath, "file.txt", "after", 0o644)
			}
			return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC)
		},
	})
	if err != nil {
		t.Fatalf("CaptureWorktree() error = %v", err)
	}
	if clockCalls != 2 {
		t.Fatalf("capture attempts = %d, want 2", clockCalls)
	}
	entry := findEntry(captured.WorktreeEntries, "file.txt")
	if got := string(readObject(t, objectStore, entry.ContentDigest)); got != "after" {
		t.Fatalf("captured content = %q, want retried version", got)
	}
}

func TestCheckDivergenceReportsChangedPathsAndUnavailableRefs(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeFile(t, repositoryPath, "base.txt", "base\n", 0o644)
	addFiles(t, worktree, "base.txt")
	base := commit(t, repository, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	if err := repository.Storer.SetReference(plumbing.NewHashReference(plumbing.NewBranchReferenceName("review-base"), base)); err != nil {
		t.Fatalf("set review-base: %v", err)
	}
	writeFile(t, repositoryPath, "target.txt", "before\n", 0o644)
	addFiles(t, worktree, "target.txt")
	commit(t, repository, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	stateDir := t.TempDir()
	objectStore, err := snapshot.OpenObjectStore(stateDir)
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	storeDatabase, err := db.OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := db.NewRepositoryStore(storeDatabase)
	t.Cleanup(func() { _ = store.Close() })
	identity := db.RepositoryIdentity{CanonicalIdentity: repositoryPath, DisplayName: "fixture", DiscoveredGitDir: filepath.Join(repositoryPath, ".git")}
	capture, err := CaptureRange(context.Background(), repositoryPath, "review-base..HEAD", objectStore)
	if err != nil {
		t.Fatalf("CaptureRange() error = %v", err)
	}
	_, _, frozen, err := store.CreateCapturedSession(context.Background(), identity, "Review", capture)
	if err != nil {
		t.Fatalf("CreateCapturedSession() error = %v", err)
	}
	report, err := CheckDivergence(context.Background(), repositoryPath, store, frozen, objectStore)
	if err != nil || report.Status != snapshot.DivergenceUnchanged {
		t.Fatalf("unchanged divergence = %#v, error = %v", report, err)
	}

	writeFile(t, repositoryPath, "target.txt", "after\n", 0o644)
	addFiles(t, worktree, "target.txt")
	commit(t, repository, worktree, "target moved", time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC))
	report, err = CheckDivergence(context.Background(), repositoryPath, store, frozen, objectStore)
	if err != nil || report.Status != snapshot.DivergenceChanged {
		t.Fatalf("changed divergence = %#v, error = %v", report, err)
	}
	if len(report.AffectedPaths) != 1 || report.AffectedPaths[0] != "target.txt" {
		t.Fatalf("changed paths = %#v, want target.txt", report.AffectedPaths)
	}
	if len(report.AffectedRefs) != 1 || report.AffectedRefs[0] != "target" {
		t.Fatalf("changed refs = %#v, want target", report.AffectedRefs)
	}

	if err := repository.Storer.RemoveReference(plumbing.NewBranchReferenceName("review-base")); err != nil {
		t.Fatalf("remove review-base: %v", err)
	}
	report, err = CheckDivergence(context.Background(), repositoryPath, store, frozen, objectStore)
	if err != nil || report.Status != snapshot.DivergenceUnavailable {
		t.Fatalf("unavailable divergence = %#v, error = %v", report, err)
	}
}

func initRepository(t *testing.T, path string) *git.Repository {
	t.Helper()
	repository, err := git.PlainInit(path, false)
	if err != nil {
		t.Fatalf("PlainInit() error = %v", err)
	}
	config, err := repository.Config()
	if err != nil {
		t.Fatalf("Config() error = %v", err)
	}
	config.User.Name = "MIRE Test"
	config.User.Email = "mire@example.test"
	if err := repository.SetConfig(config); err != nil {
		t.Fatalf("SetConfig() error = %v", err)
	}
	return repository
}

func writeFile(t *testing.T, root, name, content string, mode os.FileMode) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(root, name), []byte(content), mode); err != nil {
		t.Fatalf("write %s: %v", name, err)
	}
	if err := os.Chmod(filepath.Join(root, name), mode); err != nil {
		t.Fatalf("chmod %s: %v", name, err)
	}
}

func addFiles(t *testing.T, worktree *git.Worktree, names ...string) {
	t.Helper()
	for _, name := range names {
		if _, err := worktree.Add(name); err != nil {
			t.Fatalf("add %s: %v", name, err)
		}
	}
}

func commit(t *testing.T, repository *git.Repository, worktree *git.Worktree, message string, when time.Time) plumbing.Hash {
	t.Helper()
	hash, err := worktree.Commit(message, &git.CommitOptions{
		Author:    &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
		Committer: &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
	})
	if err != nil {
		t.Fatalf("Commit() error = %v", err)
	}
	return hash
}

func commitWithParents(t *testing.T, worktree *git.Worktree, message string, parents []plumbing.Hash, when time.Time) plumbing.Hash {
	t.Helper()
	hash, err := worktree.Commit(message, &git.CommitOptions{
		Parents:           parents,
		AllowEmptyCommits: true,
		Author:            &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
		Committer:         &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
	})
	if err != nil {
		t.Fatalf("Commit() error = %v", err)
	}
	return hash
}

func findEntry(entries []snapshot.Entry, name string) snapshot.Entry {
	for _, entry := range entries {
		if entry.Path == name {
			return entry
		}
	}
	return snapshot.Entry{}
}

func hasChange(changes []snapshot.Change, status, basePath, targetPath string) bool {
	for _, change := range changes {
		if change.Status == status && change.BasePath == basePath && change.TargetPath == targetPath {
			return true
		}
	}
	return false
}
