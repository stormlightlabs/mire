package snapshot

import (
	"context"
	"errors"
	"io"
	"os"
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

type failingReader struct{}

func (failingReader) Read(buffer []byte) (int, error) {
	copy(buffer, []byte("partial"))
	return len([]byte("partial")), errors.New("fixture read failure")
}
