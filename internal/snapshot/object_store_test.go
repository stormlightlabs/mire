package snapshot

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestObjectStorePublishesDigestVerifiedObjectsAndCleansFailedWrites(t *testing.T) {
	t.Parallel()

	store, err := OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	object, err := store.Put(context.Background(), strings.NewReader("captured bytes"))
	if err != nil {
		t.Fatalf("Put() error = %v", err)
	}
	file, err := store.Open(object.Digest)
	if err != nil {
		t.Fatalf("Open() error = %v", err)
	}
	content, err := io.ReadAll(file)
	_ = file.Close()
	if err != nil || string(content) != "captured bytes" || object.Size != int64(len(content)) {
		t.Fatalf("stored object = %q/%d, error=%v", content, object.Size, err)
	}
	if _, err := store.Put(context.Background(), failingReader{}); err == nil {
		t.Fatal("Put(failingReader) succeeded, want error")
	}
	entries, err := os.ReadDir(store.root)
	if err != nil {
		t.Fatalf("ReadDir() error = %v", err)
	}
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), ".object-") {
			t.Fatalf("failed temporary object remains: %s", entry.Name())
		}
	}
	if _, err := store.Open(strings.Repeat("a", 63)); err == nil {
		t.Fatal("Open(invalid digest) succeeded")
	}
}

func TestObjectStoreRejectsEscapingObjectPaths(t *testing.T) {
	t.Parallel()

	store, err := OpenObjectStore(t.TempDir())
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	if _, err := store.PathForDigest("../escape"); err == nil {
		t.Fatal("PathForDigest() accepted traversal")
	}
	object, err := store.Put(context.Background(), strings.NewReader("safe"))
	if err != nil {
		t.Fatalf("Put() error = %v", err)
	}
	target, err := store.PathForDigest(object.Digest)
	if err != nil {
		t.Fatalf("PathForDigest() error = %v", err)
	}
	outside := filepath.Join(t.TempDir(), "outside")
	if err := os.WriteFile(outside, []byte("outside"), 0o600); err != nil {
		t.Fatalf("WriteFile() error = %v", err)
	}
	if err := os.Remove(target); err != nil {
		t.Fatalf("Remove() error = %v", err)
	}
	if err := os.Symlink(outside, target); err != nil {
		t.Fatalf("Symlink() error = %v", err)
	}
	if _, err := store.Open(object.Digest); err == nil {
		t.Fatal("Open() accepted a symlinked object path")
	}
}

func TestValidateRepositoryPathRejectsBoundaryEscapes(t *testing.T) {
	t.Parallel()

	for _, value := range []string{"", ".", "../outside", "nested/../../outside", "/absolute", "C:/absolute", "nested\\file", "nested/\x00file"} {
		if err := ValidateRepositoryPath(value); err == nil {
			t.Errorf("ValidateRepositoryPath(%q) succeeded, want rejection", value)
		}
	}
	for _, value := range []string{"file.txt", "directory/file.txt", "name with spaces/ユニコード.txt"} {
		if err := ValidateRepositoryPath(value); err != nil {
			t.Errorf("ValidateRepositoryPath(%q) error = %v", value, err)
		}
	}
}

type failingReader struct{}

func (failingReader) Read(buffer []byte) (int, error) {
	copy(buffer, []byte("partial"))
	return len([]byte("partial")), errors.New("fixture read failure")
}
