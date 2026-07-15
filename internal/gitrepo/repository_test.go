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
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

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
