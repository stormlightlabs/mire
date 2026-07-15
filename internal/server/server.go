// Package server serves MIRE's authenticated loopback web application.
package server

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net"
	"net/http"
	"net/url"
	"path"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/gitrepo"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

const (
	apiPrefix        = "/api/v1"
	launchQuery      = "launch"
	capabilityCookie = "mire_capability"
	schemaVersion    = "v1"
	pollInterval     = 250 * time.Millisecond
	maxJSONBody      = 1 << 20
)

var identifierPattern = regexp.MustCompile(`^[a-f0-9-]{8,64}$`)

// Options configures a loopback Server.
type Options struct {
	// WorkingDir is the repository directory used for repository-scoped reads.
	WorkingDir string
	// StateDir is used to locate the private object store when ObjectStore is nil.
	StateDir string
	// ObjectStore supplies the private snapshot objects used by divergence checks.
	ObjectStore *snapshot.ObjectStore
	// StaticFiles overrides embedded assets, which is useful for contract tests.
	StaticFiles fs.FS
	// ExpectedHost pins the Host header for a test server or an already-bound listener.
	ExpectedHost string
	// SelectedSessionID is the optional session opened by `mire web [session]`.
	SelectedSessionID string
}

// Server is a foreground, authenticated web server over the application store.
// It does not expose a remote-bind mode and never accepts arbitrary commands.
type Server struct {
	store             *db.RepositoryStore
	workingDir        string
	stateDir          string
	objectStore       *snapshot.ObjectStore
	staticFiles       fs.FS
	expectedHost      string
	selectedSessionID string

	capability     string
	capabilityHash [32]byte
	capabilityUsed bool
	listener       net.Listener
	mu             sync.Mutex
	idempotency    map[string]idempotencyResult
}

type idempotencyResult struct {
	method   string
	path     string
	status   int
	body     []byte
	location string
}

// New creates a server over the same durable service used by the CLI.
func New(store *db.RepositoryStore, options Options) (*Server, error) {
	if store == nil {
		return nil, errors.New("create web server: store is nil")
	}
	capabilityBytes := make([]byte, 32)
	if _, err := rand.Read(capabilityBytes); err != nil {
		return nil, fmt.Errorf("create launch capability: %w", err)
	}
	capability := base64.RawURLEncoding.EncodeToString(capabilityBytes)
	return &Server{
		store:             store,
		workingDir:        options.WorkingDir,
		stateDir:          options.StateDir,
		objectStore:       options.ObjectStore,
		staticFiles:       chooseStaticFiles(options.StaticFiles),
		expectedHost:      strings.TrimSpace(options.ExpectedHost),
		selectedSessionID: strings.TrimSpace(options.SelectedSessionID),
		capability:        capability,
		capabilityHash:    sha256.Sum256([]byte(capability)),
		idempotency:       make(map[string]idempotencyResult),
	}, nil
}

func chooseStaticFiles(files fs.FS) fs.FS {
	if files != nil {
		return files
	}
	return embeddedStaticFiles()
}

// Listen opens a loopback-only listener.
func (s *Server) Listen(address string) (net.Listener, error) {
	if s == nil {
		return nil, errors.New("listen web server: server is nil")
	}
	host, port, err := net.SplitHostPort(strings.TrimSpace(address))
	if err != nil {
		return nil, fmt.Errorf("listen web server: address must be host:port: %w", err)
	}
	if host == "" {
		host = "127.0.0.1"
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return nil, fmt.Errorf("listen web server: %q is not a loopback address", host)
	}
	listener, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		return nil, fmt.Errorf("listen web server: %w", err)
	}
	s.mu.Lock()
	s.listener = listener
	if s.expectedHost == "" {
		s.expectedHost = listener.Addr().String()
	}
	s.mu.Unlock()
	return listener, nil
}

// LaunchURL returns the one-time URL for the currently bound server.
func (s *Server) LaunchURL() (string, error) {
	if s == nil {
		return "", errors.New("create launch URL: server is nil")
	}
	s.mu.Lock()
	listener := s.listener
	capability := s.capability
	s.mu.Unlock()
	if listener == nil {
		return "", errors.New("create launch URL: server is not listening")
	}
	host := listener.Addr().String()
	return (&url.URL{Scheme: "http", Host: host, Path: "/", RawQuery: launchQuery + "=" + url.QueryEscape(capability)}).String(), nil
}

// Serve runs the server until ctx is cancelled or the listener fails.
func (s *Server) Serve(ctx context.Context, listener net.Listener) error {
	if s == nil || listener == nil {
		return errors.New("serve web server: server and listener are required")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	httpServer := &http.Server{Handler: s}
	done := make(chan struct{})
	go func() {
		select {
		case <-ctx.Done():
			shutdownContext, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			_ = httpServer.Shutdown(shutdownContext)
		case <-done:
		}
	}()
	err := httpServer.Serve(listener)
	close(done)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

// Handler returns the authenticated HTTP handler for contract tests and
// callers that own the listener lifecycle.
func (s *Server) Handler() http.Handler { return s }

func (s *Server) ServeHTTP(response http.ResponseWriter, request *http.Request) {
	if err := s.validateRequestOrigin(request); err != nil {
		writeError(response, http.StatusForbidden, err)
		return
	}
	if request.URL.Path == "/" || !strings.HasPrefix(request.URL.Path, apiPrefix) {
		s.serveStatic(response, request)
		return
	}
	if !s.hasSessionCookie(request) {
		writeError(response, http.StatusUnauthorized, errors.New("web session is not authenticated"))
		return
	}
	s.serveAPI(response, request)
}

func (s *Server) validateRequestOrigin(request *http.Request) error {
	s.mu.Lock()
	expectedHost := s.expectedHost
	s.mu.Unlock()
	if expectedHost != "" && request.Host != expectedHost {
		return fmt.Errorf("unexpected Host header")
	}
	if origin := strings.TrimSpace(request.Header.Get("Origin")); origin != "" {
		expectedOrigin := "http://" + expectedHost
		if origin != expectedOrigin {
			return fmt.Errorf("unexpected Origin header")
		}
	}
	return nil
}

func (s *Server) hasSessionCookie(request *http.Request) bool {
	cookie, err := request.Cookie(capabilityCookie)
	if err != nil {
		return false
	}
	providedHash := sha256.Sum256([]byte(cookie.Value))
	return subtle.ConstantTimeCompare(providedHash[:], s.capabilityHash[:]) == 1
}

func (s *Server) serveStatic(response http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodGet && request.Method != http.MethodHead {
		writeError(response, http.StatusMethodNotAllowed, errors.New("static assets only support GET"))
		return
	}
	if request.URL.Path == "/" && request.URL.Query().Get(launchQuery) != "" {
		s.authorizeLaunch(response, request)
		return
	}
	if !s.hasSessionCookie(request) {
		writeError(response, http.StatusUnauthorized, errors.New("web session is not authenticated"))
		return
	}
	if _, err := fs.Stat(s.staticFiles, "index.html"); errors.Is(err, fs.ErrNotExist) {
		writeError(response, http.StatusServiceUnavailable, errors.New("web assets are not built; run pnpm --dir app build"))
		return
	}
	cleanPath := strings.TrimPrefix(path.Clean(request.URL.Path), "/")
	if cleanPath == "." || cleanPath == "" {
		cleanPath = "index.html"
	}
	if !fs.ValidPath(cleanPath) {
		writeError(response, http.StatusNotFound, errors.New("invalid static asset path"))
		return
	}
	if _, err := fs.Stat(s.staticFiles, cleanPath); err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			cleanPath = "index.html"
		} else {
			writeError(response, http.StatusInternalServerError, fmt.Errorf("inspect static asset: %w", err))
			return
		}
	}
	if cleanPath == "index.html" {
		response.Header().Set("Cache-Control", "no-cache")
	} else {
		response.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
	}
	http.ServeFileFS(response, request, s.staticFiles, cleanPath)
}

func (s *Server) authorizeLaunch(response http.ResponseWriter, request *http.Request) {
	provided := request.URL.Query().Get(launchQuery)
	providedHash := sha256.Sum256([]byte(provided))
	if subtle.ConstantTimeCompare(providedHash[:], s.capabilityHash[:]) != 1 {
		writeError(response, http.StatusUnauthorized, errors.New("invalid launch capability"))
		return
	}
	s.mu.Lock()
	alreadyUsed := s.capabilityUsed
	if !alreadyUsed {
		s.capabilityUsed = true
	}
	s.mu.Unlock()
	if alreadyUsed {
		writeError(response, http.StatusUnauthorized, errors.New("launch capability has already been used"))
		return
	}
	http.SetCookie(response, &http.Cookie{
		Name: capabilityCookie, Value: provided, Path: "/", HttpOnly: true,
		SameSite: http.SameSiteStrictMode, MaxAge: 86400,
	})
	http.Redirect(response, request, "/", http.StatusSeeOther)
}

func (s *Server) serveAPI(response http.ResponseWriter, request *http.Request) {
	if strings.HasSuffix(request.URL.Path, "/") && request.URL.Path != apiPrefix+"/" {
		writeError(response, http.StatusNotFound, errors.New("invalid API path"))
		return
	}
	if request.URL.Path == apiPrefix || request.URL.Path == apiPrefix+"/" {
		if request.Method != http.MethodGet {
			writeError(response, http.StatusMethodNotAllowed, errors.New("API root only supports GET"))
			return
		}
		s.writeJSON(response, http.StatusOK, map[string]any{"schema_version": schemaVersion, "service": "mire"})
		return
	}
	parts := strings.Split(strings.TrimPrefix(request.URL.Path, apiPrefix+"/"), "/")
	for _, part := range parts {
		if !identifierPattern.MatchString(part) && part != "bootstrap" && part != "sessions" && part != "events" && part != "rounds" && part != "operations" && part != "divergence" && part != "reviews" && part != "cancel" {
			writeError(response, http.StatusNotFound, errors.New("invalid API path"))
			return
		}
	}
	switch {
	case request.Method == http.MethodGet && len(parts) == 1 && parts[0] == "bootstrap":
		s.handleBootstrap(response, request)
	case len(parts) == 1 && parts[0] == "sessions":
		s.handleSessions(response, request)
	case len(parts) == 2 && parts[0] == "sessions":
		s.handleSession(response, request, parts[1])
	case len(parts) == 3 && parts[0] == "sessions" && parts[2] == "rounds":
		s.handleCreateRound(response, request, parts[1])
	case len(parts) == 4 && parts[0] == "rounds" && parts[2] == "divergence" && parts[3] == "":
		writeError(response, http.StatusNotFound, errors.New("invalid API path"))
	case len(parts) == 3 && parts[0] == "rounds" && parts[2] == "divergence":
		s.handleDivergence(response, request, parts[1])
	case len(parts) == 3 && parts[0] == "rounds" && parts[2] == "reviews":
		s.handleReview(response, request, parts[1])
	case len(parts) == 2 && parts[0] == "operations":
		s.handleOperation(response, request, parts[1])
	case len(parts) == 3 && parts[0] == "operations" && parts[2] == "cancel":
		s.handleCancel(response, request, parts[1])
	case len(parts) == 1 && parts[0] == "events":
		s.handleEvents(response, request)
	default:
		writeError(response, http.StatusNotFound, errors.New("API route not found"))
	}
}

func (s *Server) handleBootstrap(response http.ResponseWriter, request *http.Request) {
	sessions, err := s.listRepositorySessions(request.Context())
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	var selected *db.Session
	sessionID := request.URL.Query().Get("session")
	if sessionID == "" {
		sessionID = s.selectedSessionID
	}
	if sessionID != "" {
		session, getErr := s.store.GetSession(request.Context(), sessionID)
		if getErr != nil {
			writeStoreError(response, getErr)
			return
		}
		if getErr := s.ensureCurrentRepository(session); getErr != nil {
			writeError(response, http.StatusNotFound, getErr)
			return
		}
		selected = &session
	} else if len(sessions) > 0 {
		selected = &sessions[len(sessions)-1]
	}
	var round *roundDTO
	if selected != nil && selected.CurrentRoundID != "" {
		roundValue, getErr := s.store.GetRound(request.Context(), selected.CurrentRoundID)
		if getErr == nil {
			dto := makeRoundDTO(roundValue)
			round = &dto
		}
	}
	s.writeJSON(response, http.StatusOK, map[string]any{
		"schema_version": schemaVersion, "authenticated": true,
		"sessions": mapSessions(sessions), "selected_session": mapSession(selected), "current_round": round,
		"capabilities": map[string]bool{"review_data": false, "actions": false, "sse": true},
	})
}

func (s *Server) handleSessions(response http.ResponseWriter, request *http.Request) {
	sessions, err := s.listRepositorySessions(request.Context())
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	if request.Method == http.MethodGet {
		s.writeJSON(response, http.StatusOK, map[string]any{"schema_version": schemaVersion, "sessions": mapSessions(sessions)})
		return
	}
	if request.Method != http.MethodPost {
		writeError(response, http.StatusMethodNotAllowed, errors.New("sessions route does not support this method"))
		return
	}
	if result, ok := s.replayIdempotency(request); ok {
		writeStoredResult(response, result)
		return
	}
	if err := requireJSON(request); err != nil {
		writeError(response, http.StatusUnsupportedMediaType, err)
		return
	}
	key, err := requireIdempotencyKey(request)
	if err != nil {
		writeError(response, http.StatusBadRequest, err)
		return
	}
	var input struct {
		Title string `json:"title"`
	}
	if err := decodeJSON(response, request, &input); err != nil {
		return
	}
	identity, err := s.repositoryIdentity(request.Context())
	if err != nil {
		writeError(response, http.StatusBadRequest, err)
		return
	}
	session, err := s.store.CreateSession(request.Context(), identity, input.Title)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	body, _ := json.Marshal(map[string]any{"schema_version": schemaVersion, "session": mapSession(&session)})
	result := idempotencyResult{method: request.Method, path: request.URL.Path, status: http.StatusCreated, body: body, location: apiPrefix + "/sessions/" + session.ID}
	s.rememberIdempotency(key, result)
	writeStoredResult(response, result)
}

func (s *Server) handleSession(response http.ResponseWriter, request *http.Request, sessionID string) {
	session, err := s.store.GetSession(request.Context(), sessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	if request.Method != http.MethodGet {
		writeError(response, http.StatusMethodNotAllowed, errors.New("session route only supports GET"))
		return
	}
	rounds, err := s.store.ListRounds(request.Context(), session.ID)
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	operations, err := s.store.ListOperations(request.Context(), session.ID)
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	s.writeJSON(response, http.StatusOK, map[string]any{
		"schema_version": schemaVersion, "session": mapSession(&session), "rounds": mapRounds(rounds), "operations": mapOperations(operations),
	})
}

func (s *Server) handleCreateRound(response http.ResponseWriter, request *http.Request, sessionID string) {
	if request.Method != http.MethodPost {
		writeError(response, http.StatusMethodNotAllowed, errors.New("round creation only supports POST"))
		return
	}
	if err := requireJSON(request); err != nil {
		writeError(response, http.StatusUnsupportedMediaType, err)
		return
	}
	key, err := requireIdempotencyKey(request)
	if err != nil {
		writeError(response, http.StatusBadRequest, err)
		return
	}
	if result, ok := s.replayIdempotency(request); ok {
		writeStoredResult(response, result)
		return
	}
	var input struct{}
	if err := decodeJSON(response, request, &input); err != nil {
		return
	}
	session, err := s.store.GetSession(request.Context(), sessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	round, err := s.store.CreateRound(request.Context(), sessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	body, _ := json.Marshal(map[string]any{"schema_version": schemaVersion, "round": makeRoundDTO(round)})
	result := idempotencyResult{method: request.Method, path: request.URL.Path, status: http.StatusAccepted, body: body, location: apiPrefix + "/sessions/" + sessionID}
	s.rememberIdempotency(key, result)
	writeStoredResult(response, result)
}

func (s *Server) handleReview(response http.ResponseWriter, request *http.Request, roundID string) {
	if request.Method != http.MethodPost {
		writeError(response, http.StatusMethodNotAllowed, errors.New("review start only supports POST"))
		return
	}
	if err := requireJSON(request); err != nil {
		writeError(response, http.StatusUnsupportedMediaType, err)
		return
	}
	key, err := requireIdempotencyKey(request)
	if err != nil {
		writeError(response, http.StatusBadRequest, err)
		return
	}
	if result, ok := s.replayIdempotency(request); ok {
		writeStoredResult(response, result)
		return
	}
	var input struct{}
	if err := decodeJSON(response, request, &input); err != nil {
		return
	}
	round, err := s.store.GetRound(request.Context(), roundID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	session, err := s.store.GetSession(request.Context(), round.SessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	operation, err := s.store.CreateOperation(request.Context(), round.SessionID, round.ID, db.OperationKindReview)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	body, _ := json.Marshal(map[string]any{"schema_version": schemaVersion, "operation": makeOperationDTO(operation)})
	result := idempotencyResult{method: request.Method, path: request.URL.Path, status: http.StatusAccepted, body: body, location: apiPrefix + "/operations/" + operation.ID}
	s.rememberIdempotency(key, result)
	writeStoredResult(response, result)
}

func (s *Server) handleOperation(response http.ResponseWriter, request *http.Request, operationID string) {
	if request.Method != http.MethodGet {
		writeError(response, http.StatusMethodNotAllowed, errors.New("operation route only supports GET"))
		return
	}
	operation, err := s.store.GetOperation(request.Context(), operationID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	session, err := s.store.GetSession(request.Context(), operation.SessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	s.writeJSON(response, http.StatusOK, map[string]any{"schema_version": schemaVersion, "operation": makeOperationDTO(operation)})
}

func (s *Server) handleCancel(response http.ResponseWriter, request *http.Request, operationID string) {
	if request.Method != http.MethodPost {
		writeError(response, http.StatusMethodNotAllowed, errors.New("operation cancellation only supports POST"))
		return
	}
	if err := requireJSON(request); err != nil {
		writeError(response, http.StatusUnsupportedMediaType, err)
		return
	}
	var input struct {
		ExpectedRevision string `json:"expected_revision"`
	}
	if err := decodeJSON(response, request, &input); err != nil {
		return
	}
	operation, err := s.store.GetOperation(request.Context(), operationID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	session, err := s.store.GetSession(request.Context(), operation.SessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	if input.ExpectedRevision != "" && input.ExpectedRevision != revision(operation.UpdatedAt) {
		writeError(response, http.StatusConflict, errors.New("operation revision is stale"))
		return
	}
	operation, err = s.store.CancelOperation(request.Context(), operationID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	s.writeJSON(response, http.StatusAccepted, map[string]any{"schema_version": schemaVersion, "operation": makeOperationDTO(operation)})
}

func (s *Server) handleDivergence(response http.ResponseWriter, request *http.Request, roundID string) {
	if request.Method != http.MethodGet {
		writeError(response, http.StatusMethodNotAllowed, errors.New("divergence route only supports GET"))
		return
	}
	round, err := s.store.GetRound(request.Context(), roundID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	session, err := s.store.GetSession(request.Context(), round.SessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	if round.SnapshotID == "" {
		s.writeJSON(response, http.StatusOK, map[string]any{"schema_version": schemaVersion, "divergence": map[string]any{"status": "unavailable", "message": "Round has no captured snapshot."}})
		return
	}
	frozen, err := s.store.GetSnapshot(request.Context(), round.SnapshotID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	objectStore := s.objectStore
	if objectStore == nil {
		stateDir := s.stateDir
		if stateDir == "" {
			stateDir, err = db.DefaultStateDirectory()
		}
		if err == nil {
			objectStore, err = snapshot.OpenObjectStore(stateDir)
		}
	}
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	report, err := gitrepo.CheckDivergence(request.Context(), s.workingDir, s.store, frozen, objectStore)
	if err != nil {
		writeError(response, http.StatusInternalServerError, err)
		return
	}
	s.writeJSON(response, http.StatusOK, map[string]any{"schema_version": schemaVersion, "divergence": makeDivergenceDTO(report)})
}

func (s *Server) handleEvents(response http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodGet {
		writeError(response, http.StatusMethodNotAllowed, errors.New("events only support GET"))
		return
	}
	sessionID := strings.TrimSpace(request.URL.Query().Get("sessionId"))
	if !identifierPattern.MatchString(sessionID) {
		writeError(response, http.StatusBadRequest, errors.New("sessionId is required"))
		return
	}
	session, err := s.store.GetSession(request.Context(), sessionID)
	if err != nil {
		writeStoreError(response, err)
		return
	}
	if err := s.ensureCurrentRepository(session); err != nil {
		writeError(response, http.StatusNotFound, err)
		return
	}
	afterID := int64(0)
	if value := strings.TrimSpace(request.Header.Get("Last-Event-ID")); value != "" {
		parsed, err := strconv.ParseInt(value, 10, 64)
		if err != nil || parsed < 0 {
			writeError(response, http.StatusBadRequest, errors.New("Last-Event-ID must be a non-negative integer"))
			return
		}
		afterID = parsed
	}
	flusher, ok := response.(http.Flusher)
	if !ok {
		writeError(response, http.StatusInternalServerError, errors.New("SSE is not supported by this server"))
		return
	}
	response.Header().Set("Content-Type", "text/event-stream")
	response.Header().Set("Cache-Control", "no-cache")
	response.Header().Set("Connection", "keep-alive")
	response.WriteHeader(http.StatusOK)
	_, _ = io.WriteString(response, ": connected\n\n")
	flusher.Flush()
	ticker := time.NewTicker(pollInterval)
	defer ticker.Stop()
	for {
		activities, err := s.store.ListActivity(request.Context(), sessionID, afterID)
		if err != nil {
			return
		}
		for _, activity := range activities {
			event := map[string]any{
				"schema_version": schemaVersion, "activity_id": activity.ID, "session_id": activity.SessionID,
				"operation_id": activity.OperationID, "round_id": activity.RoundID, "event_kind": activity.Kind,
				"status": activity.Status, "message": activity.Message, "created_at": activity.CreatedAt.UTC().Format(time.RFC3339Nano),
			}
			encoded, _ := json.Marshal(event)
			_, _ = fmt.Fprintf(response, "id: %d\nevent: activity\ndata: %s\n\n", activity.ID, encoded)
			flusher.Flush()
			afterID = activity.ID
		}
		select {
		case <-request.Context().Done():
			return
		case <-ticker.C:
		}
	}
}

func (s *Server) listRepositorySessions(ctx context.Context) ([]db.Session, error) {
	identity, err := s.repositoryIdentity(ctx)
	if err != nil {
		return nil, err
	}
	return s.store.ListSessionsForRepository(ctx, identity.CanonicalIdentity)
}

func (s *Server) repositoryIdentity(ctx context.Context) (db.RepositoryIdentity, error) {
	identity, err := gitrepo.Discover(ctx, s.workingDir)
	if err != nil {
		return db.RepositoryIdentity{}, fmt.Errorf("discover current repository: %w", err)
	}
	return db.RepositoryIdentity{CanonicalIdentity: identity.CanonicalIdentity, DisplayName: identity.DisplayName, DiscoveredGitDir: identity.DiscoveredGitDir}, nil
}

func (s *Server) ensureCurrentRepository(session db.Session) error {
	identity, err := s.repositoryIdentity(context.Background())
	if err != nil || identity.CanonicalIdentity != session.RepositoryIdentity {
		return errors.New("session is not for the current repository")
	}
	return nil
}

func (s *Server) replayIdempotency(request *http.Request) (idempotencyResult, bool) {
	key := strings.TrimSpace(request.Header.Get("Idempotency-Key"))
	if key == "" {
		return idempotencyResult{}, false
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	result, ok := s.idempotency[key]
	if !ok || result.method != request.Method || result.path != request.URL.Path {
		return idempotencyResult{}, false
	}
	return result, true
}

func (s *Server) rememberIdempotency(key string, result idempotencyResult) {
	s.mu.Lock()
	s.idempotency[key] = result
	s.mu.Unlock()
}

func requireJSON(request *http.Request) error {
	contentType := strings.ToLower(strings.TrimSpace(request.Header.Get("Content-Type")))
	if !strings.HasPrefix(contentType, "application/json") {
		return errors.New("mutations require Content-Type: application/json")
	}
	return nil
}

func requireIdempotencyKey(request *http.Request) (string, error) {
	key := strings.TrimSpace(request.Header.Get("Idempotency-Key"))
	if key == "" {
		return "", errors.New("mutations require an Idempotency-Key header")
	}
	if len(key) > 200 {
		return "", errors.New("Idempotency-Key is too long")
	}
	return key, nil
}

func decodeJSON(response http.ResponseWriter, request *http.Request, target any) error {
	decoder := json.NewDecoder(io.LimitReader(request.Body, maxJSONBody))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		writeError(response, http.StatusBadRequest, fmt.Errorf("decode JSON: %w", err))
		return err
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		writeError(response, http.StatusBadRequest, errors.New("request body must contain one JSON value"))
		return errors.New("request body contains multiple JSON values")
	}
	return nil
}

func (s *Server) writeJSON(response http.ResponseWriter, status int, value any) {
	response.Header().Set("Content-Type", "application/json; charset=utf-8")
	response.Header().Set("X-Content-Type-Options", "nosniff")
	response.WriteHeader(status)
	_ = json.NewEncoder(response).Encode(value)
}

func writeStoredResult(response http.ResponseWriter, result idempotencyResult) {
	response.Header().Set("Content-Type", "application/json; charset=utf-8")
	response.Header().Set("X-Content-Type-Options", "nosniff")
	if result.location != "" {
		response.Header().Set("Location", result.location)
	}
	response.WriteHeader(result.status)
	_, _ = response.Write(result.body)
}

func writeError(response http.ResponseWriter, status int, err error) {
	response.Header().Set("Content-Type", "application/json; charset=utf-8")
	response.Header().Set("X-Content-Type-Options", "nosniff")
	response.WriteHeader(status)
	_ = json.NewEncoder(response).Encode(map[string]any{"schema_version": schemaVersion, "error": err.Error()})
}

func writeStoreError(response http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, db.ErrSessionNotFound), errors.Is(err, db.ErrRoundNotFound), errors.Is(err, db.ErrOperationNotFound), errors.Is(err, db.ErrSnapshotNotFound):
		writeError(response, http.StatusNotFound, err)
	case errors.Is(err, db.ErrOperationActive), errors.Is(err, db.ErrOperationAlreadyOwned), errors.Is(err, db.ErrOperationNotAcquirable):
		writeError(response, http.StatusConflict, err)
	default:
		writeError(response, http.StatusBadRequest, err)
	}
}

func revision(value time.Time) string { return value.UTC().Format(time.RFC3339Nano) }

type sessionDTO struct {
	ID             string `json:"id"`
	RepositoryID   string `json:"repository_id"`
	RepositoryName string `json:"repository_name"`
	Title          string `json:"title"`
	CreatedAt      string `json:"created_at"`
	CurrentRoundID string `json:"current_round_id,omitempty"`
}

type roundDTO struct {
	ID         string `json:"id"`
	SessionID  string `json:"session_id"`
	SnapshotID string `json:"snapshot_id,omitempty"`
	Number     int    `json:"number"`
	Status     string `json:"status"`
	Revision   string `json:"revision"`
	CreatedAt  string `json:"created_at"`
	UpdatedAt  string `json:"updated_at"`
}

type operationDTO struct {
	ID         string `json:"id"`
	SessionID  string `json:"session_id"`
	RoundID    string `json:"round_id,omitempty"`
	Kind       string `json:"kind"`
	Status     string `json:"status"`
	Failure    string `json:"failure,omitempty"`
	Revision   string `json:"revision"`
	CreatedAt  string `json:"created_at"`
	UpdatedAt  string `json:"updated_at"`
	StartedAt  string `json:"started_at,omitempty"`
	FinishedAt string `json:"finished_at,omitempty"`
}

func mapSession(session *db.Session) *sessionDTO {
	if session == nil {
		return nil
	}
	return &sessionDTO{session.ID, session.RepositoryID, session.RepositoryName, session.Title, session.CreatedAt.UTC().Format(time.RFC3339Nano), session.CurrentRoundID}
}
func mapSessions(sessions []db.Session) []*sessionDTO {
	result := make([]*sessionDTO, 0, len(sessions))
	for i := range sessions {
		result = append(result, mapSession(&sessions[i]))
	}
	return result
}
func makeRoundDTO(round db.Round) roundDTO {
	return roundDTO{round.ID, round.SessionID, round.SnapshotID, round.Number, string(round.Status), revision(round.UpdatedAt), round.CreatedAt.UTC().Format(time.RFC3339Nano), round.UpdatedAt.UTC().Format(time.RFC3339Nano)}
}
func mapRounds(rounds []db.Round) []roundDTO {
	result := make([]roundDTO, 0, len(rounds))
	for _, round := range rounds {
		result = append(result, makeRoundDTO(round))
	}
	return result
}
func makeOperationDTO(operation db.Operation) operationDTO {
	return operationDTO{operation.ID, operation.SessionID, operation.RoundID, string(operation.Kind), string(operation.Status), operation.Failure, revision(operation.UpdatedAt), operation.CreatedAt.UTC().Format(time.RFC3339Nano), operation.UpdatedAt.UTC().Format(time.RFC3339Nano), optionalTime(operation.StartedAt), optionalTime(operation.FinishedAt)}
}
func mapOperations(operations []db.Operation) []operationDTO {
	result := make([]operationDTO, 0, len(operations))
	for _, operation := range operations {
		result = append(result, makeOperationDTO(operation))
	}
	return result
}
func optionalTime(value time.Time) string {
	if value.IsZero() {
		return ""
	}
	return value.UTC().Format(time.RFC3339Nano)
}
func makeDivergenceDTO(report snapshot.DivergenceReport) map[string]any {
	return map[string]any{"snapshot_id": report.SnapshotID, "status": report.Status, "affected_paths": report.AffectedPaths, "affected_refs": report.AffectedRefs, "message": report.Message}
}
