package snapshot

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

const objectStoreDirectory = "objects"

// Object is a verified private content-addressed object.
type Object struct {
	Digest string
	Size   int64
}

// ObjectStore stores snapshot bytes outside the reviewed repository.
type ObjectStore struct {
	root string
}

// OpenObjectStore opens the content-addressed store below stateDir, creating
// only private directories as needed.
func OpenObjectStore(stateDir string) (*ObjectStore, error) {
	stateDir = strings.TrimSpace(stateDir)
	if stateDir == "" {
		return nil, fmt.Errorf("open snapshot object store: state directory is empty")
	}
	absolute, err := filepath.Abs(stateDir)
	if err != nil {
		return nil, fmt.Errorf("resolve snapshot object store: %w", err)
	}
	if err := ensurePrivateDirectory(absolute); err != nil {
		return nil, err
	}
	root := filepath.Join(absolute, objectStoreDirectory, "sha256")
	if err := ensurePrivateDirectory(filepath.Join(absolute, objectStoreDirectory)); err != nil {
		return nil, err
	}
	if err := ensurePrivateDirectory(root); err != nil {
		return nil, err
	}
	return &ObjectStore{root: root}, nil
}

// Put copies and verifies bytes before making their digest path visible.
// Failed writes remove their temporary file and never create a partial object.
func (store *ObjectStore) Put(ctx context.Context, reader io.Reader) (Object, error) {
	if store == nil || store.root == "" {
		return Object{}, fmt.Errorf("write snapshot object: object store is nil")
	}
	if reader == nil {
		return Object{}, fmt.Errorf("write snapshot object: reader is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	temporary, err := os.CreateTemp(store.root, ".object-")
	if err != nil {
		return Object{}, fmt.Errorf("create snapshot object temporary file: %w", err)
	}
	temporaryName := temporary.Name()
	defer func() { _ = os.Remove(temporaryName) }()

	hasher := sha256.New()
	writer := io.MultiWriter(temporary, hasher)
	var size int64
	buffer := make([]byte, 128*1024)
	for {
		select {
		case <-ctx.Done():
			_ = temporary.Close()
			return Object{}, fmt.Errorf("write snapshot object: %w", ctx.Err())
		default:
		}
		read, readErr := reader.Read(buffer)
		if read > 0 {
			written, writeErr := writer.Write(buffer[:read])
			if writeErr != nil {
				_ = temporary.Close()
				return Object{}, fmt.Errorf("write snapshot object: %w", writeErr)
			}
			if written != read {
				_ = temporary.Close()
				return Object{}, fmt.Errorf("write snapshot object: short write")
			}
			size += int64(read)
		}
		if readErr != nil {
			if errors.Is(readErr, io.EOF) {
				break
			}
			_ = temporary.Close()
			return Object{}, fmt.Errorf("read snapshot object: %w", readErr)
		}
	}
	if err := temporary.Sync(); err != nil {
		_ = temporary.Close()
		return Object{}, fmt.Errorf("sync snapshot object: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return Object{}, fmt.Errorf("close snapshot object: %w", err)
	}
	digest := hex.EncodeToString(hasher.Sum(nil))
	target, err := store.pathForDigest(digest)
	if err != nil {
		return Object{}, err
	}
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		return Object{}, fmt.Errorf("create snapshot object directory: %w", err)
	}
	if err := ensurePrivateDirectory(filepath.Dir(target)); err != nil {
		return Object{}, err
	}
	if info, statErr := os.Lstat(target); statErr == nil {
		if info.Mode()&os.ModeSymlink != 0 || !info.Mode().IsRegular() {
			return Object{}, fmt.Errorf("snapshot object path is not a regular file: %s", target)
		}
		if err := verifyObjectFile(target, digest, size); err != nil {
			return Object{}, err
		}
		return Object{Digest: digest, Size: size}, nil
	} else if !errors.Is(statErr, os.ErrNotExist) {
		return Object{}, fmt.Errorf("inspect snapshot object: %w", statErr)
	}
	if err := os.Rename(temporaryName, target); err != nil {
		if info, statErr := os.Lstat(target); statErr == nil && info.Mode().IsRegular() && info.Mode()&os.ModeSymlink == 0 {
			if verifyErr := verifyObjectFile(target, digest, size); verifyErr != nil {
				return Object{}, verifyErr
			}
			return Object{Digest: digest, Size: size}, nil
		}
		return Object{}, fmt.Errorf("publish snapshot object: %w", err)
	}
	if err := os.Chmod(target, 0o600); err != nil {
		return Object{}, fmt.Errorf("restrict snapshot object permissions: %w", err)
	}
	return Object{Digest: digest, Size: size}, nil
}

// Open returns a read-only handle to a verified digest path.
func (store *ObjectStore) Open(digest string) (*os.File, error) {
	if store == nil || store.root == "" {
		return nil, fmt.Errorf("open snapshot object: object store is nil")
	}
	target, err := store.pathForDigest(digest)
	if err != nil {
		return nil, err
	}
	for _, directory := range []string{filepath.Dir(store.root), store.root, filepath.Dir(target)} {
		if err := inspectPrivateDirectory(directory); err != nil {
			return nil, err
		}
	}
	info, err := os.Lstat(target)
	if err != nil {
		return nil, fmt.Errorf("open snapshot object %q: %w", digest, err)
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.Mode().IsRegular() {
		return nil, fmt.Errorf("open snapshot object %q: object is not a regular file", digest)
	}
	return os.Open(target)
}

// PathForDigest returns the private path for a validated object digest. It is
// useful to tests and garbage collection without exposing arbitrary paths.
func (store *ObjectStore) PathForDigest(digest string) (string, error) {
	if store == nil || store.root == "" {
		return "", fmt.Errorf("snapshot object path: object store is nil")
	}
	return store.pathForDigest(digest)
}

func (store *ObjectStore) pathForDigest(digest string) (string, error) {
	if len(digest) != sha256.Size*2 {
		return "", fmt.Errorf("snapshot object: invalid digest %q", digest)
	}
	if _, err := hex.DecodeString(digest); err != nil || strings.ToLower(digest) != digest {
		return "", fmt.Errorf("snapshot object: invalid digest %q", digest)
	}
	return filepath.Join(store.root, digest[:2], digest[2:]), nil
}

func verifyObjectFile(path, expectedDigest string, expectedSize int64) error {
	file, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("verify snapshot object: %w", err)
	}
	defer file.Close()
	hasher := sha256.New()
	size, err := io.Copy(hasher, file)
	if err != nil {
		return fmt.Errorf("verify snapshot object: %w", err)
	}
	if size != expectedSize || hex.EncodeToString(hasher.Sum(nil)) != expectedDigest {
		return fmt.Errorf("verify snapshot object: digest or size mismatch for %q", expectedDigest)
	}
	return nil
}

func ensurePrivateDirectory(path string) error {
	info, err := os.Lstat(path)
	switch {
	case err == nil:
		if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
			return fmt.Errorf("snapshot object store: %s is not a directory", path)
		}
	case errors.Is(err, os.ErrNotExist):
		if err := os.MkdirAll(path, 0o700); err != nil {
			return fmt.Errorf("create private snapshot directory: %w", err)
		}
	default:
		return fmt.Errorf("inspect private snapshot directory: %w", err)
	}
	if err := os.Chmod(path, 0o700); err != nil {
		return fmt.Errorf("restrict private snapshot directory: %w", err)
	}
	return nil
}

func inspectPrivateDirectory(path string) error {
	info, err := os.Lstat(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("inspect private snapshot directory: %w", err)
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return fmt.Errorf("snapshot object store: %s is not a directory", path)
	}
	return nil
}
