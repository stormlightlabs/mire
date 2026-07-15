package server

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/gitrepo"
)

type cancelingRecorder struct {
	*httptest.ResponseRecorder
	cancel  context.CancelFunc
	flushes int
}

func (recorder *cancelingRecorder) Flush() {
	recorder.flushes++
	recorder.ResponseRecorder.Flush()
	if recorder.flushes >= 2 {
		recorder.cancel()
	}
}

func callHandler(t *testing.T, webServer *Server, method, target, body string, cookie *http.Cookie, headers map[string]string, host string) *http.Response {
	t.Helper()
	request := httptest.NewRequest(method, target, strings.NewReader(body))
	request.Host = host
	if cookie != nil {
		request.AddCookie(cookie)
	}
	for key, value := range headers {
		request.Header.Set(key, value)
	}
	recorder := httptest.NewRecorder()
	webServer.ServeHTTP(recorder, request)
	return recorder.Result()
}

func TestLoopbackServerAuthenticatesAndReplaysDurableResources(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	stateDir := t.TempDir()
	database, err := db.OpenState(ctx, stateDir)
	if err != nil {
		t.Fatalf("open state: %v", err)
	}
	store := db.NewRepositoryStore(database)
	defer store.Close()
	workingDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("get working directory: %v", err)
	}
	identity, err := gitrepo.Discover(ctx, workingDir)
	if err != nil {
		t.Fatalf("discover repository: %v", err)
	}
	seed, err := store.CreateSession(ctx, db.RepositoryIdentity{
		CanonicalIdentity: identity.CanonicalIdentity,
		DisplayName:       identity.DisplayName,
		DiscoveredGitDir:  identity.DiscoveredGitDir,
	}, "Seed session")
	if err != nil {
		t.Fatalf("seed session: %v", err)
	}

	staticFiles := fstest.MapFS{"index.html": &fstest.MapFile{Mode: 0o644, Data: []byte("<!doctype html><title>test</title>")}}
	webServer, err := New(store, Options{WorkingDir: workingDir, StaticFiles: staticFiles, ExpectedHost: "mire.test", SelectedSessionID: seed.ID})
	if err != nil {
		t.Fatalf("create server: %v", err)
	}
	launchURL := "http://mire.test/?launch=" + url.QueryEscape(webServer.capability)
	launchResponse := callHandler(t, webServer, http.MethodGet, launchURL, "", nil, nil, "mire.test")
	if launchResponse.StatusCode != http.StatusSeeOther {
		t.Fatalf("launch status = %d, want %d", launchResponse.StatusCode, http.StatusSeeOther)
	}
	launchCookie := launchResponse.Cookies()[0]
	launchResponse.Body.Close()
	if launchCookie.Name != capabilityCookie {
		t.Fatalf("launch cookie = %q, want %q", launchCookie.Name, capabilityCookie)
	}

	rootResponse := callHandler(t, webServer, http.MethodGet, "http://mire.test/", "", launchCookie, nil, "mire.test")
	if rootResponse.StatusCode != http.StatusOK {
		t.Fatalf("root status = %d, want %d", rootResponse.StatusCode, http.StatusOK)
	}
	rootBody, _ := io.ReadAll(rootResponse.Body)
	rootResponse.Body.Close()
	if !strings.Contains(string(rootBody), "test") {
		t.Fatalf("root body did not use static fallback: %q", rootBody)
	}

	bootstrap := callHandler(t, webServer, http.MethodGet, "http://mire.test"+apiPrefix+"/bootstrap", "", launchCookie, nil, "mire.test")
	if bootstrap.StatusCode != http.StatusOK {
		t.Fatalf("bootstrap status = %d, want %d", bootstrap.StatusCode, http.StatusOK)
	}
	bootstrapBody, _ := io.ReadAll(bootstrap.Body)
	bootstrap.Body.Close()
	if !strings.Contains(string(bootstrapBody), seed.ID) {
		t.Fatalf("bootstrap did not select seeded session: %q", bootstrapBody)
	}

	badHost := callHandler(t, webServer, http.MethodGet, "http://mire.test"+apiPrefix, "", launchCookie, nil, "evil.example")
	if badHost.StatusCode != http.StatusForbidden {
		t.Fatalf("bad Host status = %d, want %d", badHost.StatusCode, http.StatusForbidden)
	}

	createRequest := []byte(`{"title":"Browser session"}`)
	created := callHandler(t, webServer, http.MethodPost, "http://mire.test"+apiPrefix+"/sessions", string(createRequest), launchCookie, map[string]string{"Content-Type": "application/json", "Idempotency-Key": "session-create-1"}, "mire.test")
	if created.StatusCode != http.StatusCreated {
		t.Fatalf("create session status = %d, want %d", created.StatusCode, http.StatusCreated)
	}
	createdBody, _ := io.ReadAll(created.Body)
	created.Body.Close()
	replayed := callHandler(t, webServer, http.MethodPost, "http://mire.test"+apiPrefix+"/sessions", string(createRequest), launchCookie, map[string]string{"Content-Type": "application/json", "Idempotency-Key": "session-create-1"}, "mire.test")
	replayedBody, _ := io.ReadAll(replayed.Body)
	replayed.Body.Close()
	if string(createdBody) != string(replayedBody) {
		t.Fatalf("idempotent response changed:\nfirst %s\nsecond %s", createdBody, replayedBody)
	}
	var createdPayload struct {
		Session struct {
			ID string `json:"id"`
		} `json:"session"`
	}
	if err := json.Unmarshal(createdBody, &createdPayload); err != nil {
		t.Fatalf("decode created session: %v", err)
	}

	roundURL := "http://mire.test" + apiPrefix + "/sessions/" + createdPayload.Session.ID + "/rounds"
	round := callHandler(t, webServer, http.MethodPost, roundURL, "{}", launchCookie, map[string]string{"Content-Type": "application/json", "Idempotency-Key": "round-create-1"}, "mire.test")
	if round.StatusCode != http.StatusAccepted {
		t.Fatalf("create round status = %d, want %d", round.StatusCode, http.StatusAccepted)
	}
	roundBody, _ := io.ReadAll(round.Body)
	round.Body.Close()
	var roundPayload struct {
		Round struct {
			ID string `json:"id"`
		} `json:"round"`
	}
	if err := json.Unmarshal(roundBody, &roundPayload); err != nil {
		t.Fatalf("decode round: %v", err)
	}
	review := callHandler(t, webServer, http.MethodPost, "http://mire.test"+apiPrefix+"/rounds/"+roundPayload.Round.ID+"/reviews", "{}", launchCookie, map[string]string{"Content-Type": "application/json", "Idempotency-Key": "review-start-1"}, "mire.test")
	if review.StatusCode != http.StatusAccepted {
		t.Fatalf("start review status = %d, want %d", review.StatusCode, http.StatusAccepted)
	}
	review.Body.Close()

	divergence := callHandler(t, webServer, http.MethodGet, "http://mire.test"+apiPrefix+"/rounds/"+roundPayload.Round.ID+"/divergence", "", launchCookie, nil, "mire.test")
	divergenceBody, _ := io.ReadAll(divergence.Body)
	divergence.Body.Close()
	if divergence.StatusCode != http.StatusOK || !strings.Contains(string(divergenceBody), "unavailable") {
		t.Fatalf("unexpected divergence response: %d %s", divergence.StatusCode, divergenceBody)
	}

	sseContext, cancelSSE := context.WithCancel(ctx)
	sseRequest := httptest.NewRequest(http.MethodGet, "http://mire.test"+apiPrefix+"/events?sessionId="+url.QueryEscape(createdPayload.Session.ID), nil).WithContext(sseContext)
	sseRequest.Host = "mire.test"
	sseRequest.AddCookie(launchCookie)
	sseRecorder := &cancelingRecorder{ResponseRecorder: httptest.NewRecorder(), cancel: cancelSSE}
	webServer.ServeHTTP(sseRecorder, sseRequest)
	sseResponse := sseRecorder.Result()
	reader := bufio.NewReader(sseResponse.Body)
	sseBody, _ := io.ReadAll(reader)
	sseResponse.Body.Close()
	if !strings.Contains(string(sseBody), "event: activity") {
		t.Fatal("SSE did not replay a durable activity event")
	}

	secondLaunch := callHandler(t, webServer, http.MethodGet, launchURL, "", nil, nil, "mire.test")
	secondLaunch.Body.Close()
	if secondLaunch.StatusCode != http.StatusUnauthorized {
		t.Fatalf("reused launch status = %d, want %d", secondLaunch.StatusCode, http.StatusUnauthorized)
	}
}
