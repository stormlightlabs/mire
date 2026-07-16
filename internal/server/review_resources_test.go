package server

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
	"testing/fstest"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/gitrepo"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestReviewResourcesExposeOnlyCanonicalRoundData(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	stateDir := t.TempDir()
	database, err := db.OpenState(ctx, stateDir)
	if err != nil {
		t.Fatal(err)
	}
	store := db.NewRepositoryStore(database)
	t.Cleanup(func() { _ = store.Close() })
	objects, err := snapshot.OpenObjectStore(stateDir)
	if err != nil {
		t.Fatal(err)
	}
	baseObject, err := objects.Put(ctx, bytes.NewBufferString("before\n"))
	if err != nil {
		t.Fatal(err)
	}
	targetObject, err := objects.Put(ctx, bytes.NewBufferString("after\n"))
	if err != nil {
		t.Fatal(err)
	}
	baseEntries := []snapshot.Entry{{
		Path: "internal/example.go", Kind: snapshot.EntryKindFile, Mode: 0o644,
		Size: 7, ContentDigest: baseObject.Digest,
	}}
	targetEntries := []snapshot.Entry{{
		Path: "internal/example.go", Kind: snapshot.EntryKindFile, Mode: 0o644,
		Size: 6, ContentDigest: targetObject.Digest,
	}}
	baseDigest, _ := snapshot.ManifestDigest(baseEntries)
	targetDigest, _ := snapshot.ManifestDigest(targetEntries)
	capture := snapshot.Capture{
		ComparisonKind: snapshot.ComparisonTwoDot, RequestedComparison: "base..target",
		BaseOID: "base", EffectiveBaseOID: "base", ObjectFormat: "sha256",
		ContextPolicyHash: snapshot.DefaultContextPolicyHash(), CapturedAt: time.Now().UTC(),
		Base:    snapshot.TreeState{OID: "base", Entries: baseEntries, ManifestDigest: baseDigest},
		Target:  snapshot.TreeState{OID: "target", Entries: targetEntries, ManifestDigest: targetDigest},
		Changes: snapshot.BuildChanges(baseEntries, targetEntries),
	}
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	workingDir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	identity, err := gitrepo.Discover(ctx, workingDir)
	if err != nil {
		t.Fatal(err)
	}
	_, round, _, err := store.CreateCapturedSession(ctx, db.RepositoryIdentity{
		CanonicalIdentity: identity.CanonicalIdentity,
		DisplayName:       identity.DisplayName,
		DiscoveredGitDir:  identity.DiscoveredGitDir,
	}, "Browser fixture", capture)
	if err != nil {
		t.Fatal(err)
	}
	webServer, err := New(store, Options{
		WorkingDir: workingDir, StateDir: stateDir, ObjectStore: objects,
		ExpectedHost: "mire.test",
		StaticFiles:  fstest.MapFS{"index.html": &fstest.MapFile{Mode: 0o644, Data: []byte("ok")}},
	})
	if err != nil {
		t.Fatal(err)
	}
	cookie := &http.Cookie{Name: capabilityCookie, Value: webServer.capability}

	for _, resource := range []string{"overview", "diff", "slices", "coverage"} {
		response := callHandler(t, webServer, http.MethodGet,
			"http://mire.test"+apiPrefix+"/rounds/"+round.ID+"/"+resource,
			"", cookie, nil, "mire.test")
		body, readErr := io.ReadAll(response.Body)
		response.Body.Close()
		if readErr != nil {
			t.Fatal(readErr)
		}
		if response.StatusCode != http.StatusOK {
			t.Fatalf("%s status = %d, body = %s", resource, response.StatusCode, body)
		}
		if !json.Valid(body) || !strings.Contains(string(body), round.SnapshotID) {
			t.Fatalf("%s did not return snapshot-bound JSON: %s", resource, body)
		}
	}

	diffResponse := callHandler(t, webServer, http.MethodGet,
		"http://mire.test"+apiPrefix+"/rounds/"+round.ID+"/diff", "", cookie, nil, "mire.test")
	diffBody, _ := io.ReadAll(diffResponse.Body)
	diffResponse.Body.Close()
	if !strings.Contains(string(diffBody), "before") || !strings.Contains(string(diffBody), "after") ||
		!strings.Contains(string(diffBody), "internal/example.go#") {
		t.Fatalf("diff omits frozen content or anchors: %s", diffBody)
	}

	for _, lane := range []string{"verified", "candidate", "refuted"} {
		response := callHandler(t, webServer, http.MethodGet,
			"http://mire.test"+apiPrefix+"/rounds/"+round.ID+"/findings?lane="+lane,
			"", cookie, nil, "mire.test")
		if response.StatusCode != http.StatusOK {
			t.Fatalf("lane %s status = %d", lane, response.StatusCode)
		}
		response.Body.Close()
	}

	badLane := callHandler(t, webServer, http.MethodGet,
		"http://mire.test"+apiPrefix+"/rounds/"+round.ID+"/findings?lane=all",
		"", cookie, nil, "mire.test")
	if badLane.StatusCode != http.StatusBadRequest {
		t.Fatalf("bad lane status = %d, want %d", badLane.StatusCode, http.StatusBadRequest)
	}
	badLane.Body.Close()

	arbitraryPath := callHandler(t, webServer, http.MethodGet,
		"http://mire.test"+apiPrefix+"/rounds/"+round.ID+"/diff/internal/example.go",
		"", cookie, nil, "mire.test")
	if arbitraryPath.StatusCode != http.StatusNotFound {
		t.Fatalf("arbitrary path status = %d, want %d", arbitraryPath.StatusCode, http.StatusNotFound)
	}
	arbitraryPath.Body.Close()
}
