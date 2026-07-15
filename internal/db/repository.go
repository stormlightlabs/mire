package db

import (
	"context"
	"crypto/rand"
	"database/sql"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/shared"
)

var (
	// ErrSessionNotFound is returned when a session ID is not persisted.
	ErrSessionNotFound = errors.New("session not found")
	// ErrRepositoryNotFound is returned when a repository ID is not persisted.
	ErrRepositoryNotFound = errors.New("repository not found")
	// ErrSessionRepositoryMismatch is returned when a session is used from a
	// different repository than the one that owns it.
	ErrSessionRepositoryMismatch = errors.New("session belongs to another repository")
	// ErrInvalidRepositoryIdentity is returned when the identity is incomplete.
	ErrInvalidRepositoryIdentity = errors.New("invalid repository identity")
)

// RepositoryIdentity is the read-only identity discovered from a Git
// worktree. It is stored with every session so future versions can distinguish
// repositories without consulting the live worktree.
type RepositoryIdentity struct {
	CanonicalIdentity string
	DisplayName       string
	DiscoveredGitDir  string
}

// Repository is persisted repository metadata.
type Repository struct {
	ID                string
	CanonicalIdentity string
	DisplayName       string
	DiscoveredGitDir  string
	CreatedAt         time.Time
}

// Session is persisted session metadata. RepositoryName and
// RepositoryIdentity are denormalized query fields populated from the related
// repository record; they are never accepted as write input.
type Session struct {
	ID                 string
	RepositoryID       string
	RepositoryName     string
	RepositoryIdentity string
	Title              string
	CreatedAt          time.Time
	CurrentRoundID     string
}

// IDGenerator creates a new stable identifier.
type IDGenerator func() (string, error)

// StoreOption customizes a RepositoryStore for deterministic tests.
type StoreOption func(*RepositoryStore)

// RepositoryStore is the application-facing repository and session service.
// Session creation persists the repository and session in one transaction.
type RepositoryStore struct {
	database      *DB
	now           func() time.Time
	newID         IDGenerator
	processID     string
	processIDErr  error
	leaseDuration time.Duration
}

// NewRepositoryStore creates a repository/session service over database.
func NewRepositoryStore(database *DB, options ...StoreOption) *RepositoryStore {
	processID, processIDErr := NewID()
	store := &RepositoryStore{
		database:      database,
		now:           time.Now,
		newID:         NewID,
		processID:     processID,
		processIDErr:  processIDErr,
		leaseDuration: DefaultOperationLeaseDuration,
	}
	for _, option := range options {
		if option != nil {
			option(store)
		}
	}
	return store
}

// WithProcessInstanceID overrides the random process-instance owner ID. It is
// intended for deterministic tests and does not affect persisted entity IDs.
func WithProcessInstanceID(processID string) StoreOption {
	return func(store *RepositoryStore) {
		processID = strings.TrimSpace(processID)
		if processID != "" {
			store.processID = processID
			store.processIDErr = nil
		}
	}
}

// WithOperationLeaseDuration sets the bounded duration of an acquired
// operation lease and each subsequent heartbeat.
func WithOperationLeaseDuration(duration time.Duration) StoreOption {
	return func(store *RepositoryStore) {
		store.leaseDuration = duration
	}
}

// WithClock injects the timestamp source used by a RepositoryStore.
func WithClock(clock func() time.Time) StoreOption {
	return func(store *RepositoryStore) {
		if clock != nil {
			store.now = clock
		}
	}
}

// WithIDGenerator injects the identifier source used by a RepositoryStore.
func WithIDGenerator(generator IDGenerator) StoreOption {
	return func(store *RepositoryStore) {
		if generator != nil {
			store.newID = generator
		}
	}
}

// Close closes the underlying database.
func (store *RepositoryStore) Close() error {
	if store == nil || store.database == nil || store.database.DB == nil {
		return nil
	}
	return store.database.Close()
}

// CreateSession creates a session for identity. Repository insertion or
// refresh and session insertion commit together, so a failed session write
// cannot leave a newly-created repository record behind.
func (store *RepositoryStore) CreateSession(ctx context.Context, identity RepositoryIdentity, title string) (Session, error) {
	if err := store.validate(); err != nil {
		return Session{}, err
	}
	identity, err := normalizeIdentity(identity)
	if err != nil {
		return Session{}, err
	}
	title = strings.TrimSpace(title)
	if title == "" {
		return Session{}, fmt.Errorf("create session: title is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Session{}, fmt.Errorf("begin session transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	repository, err := store.ensureRepositoryTx(ctx, tx, identity)
	if err != nil {
		return Session{}, err
	}

	sessionID, err := store.newID()
	if err != nil {
		return Session{}, fmt.Errorf("create session ID: %w", err)
	}
	createdAt := store.now().UTC()
	if createdAt.IsZero() {
		return Session{}, fmt.Errorf("create session: clock returned zero time")
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO sessions (id, repository_id, title, created_at)
VALUES (?, ?, ?, ?)`,
		sessionID,
		repository.ID,
		title,
		shared.TimestampString(createdAt),
	); err != nil {
		return Session{}, fmt.Errorf("insert session: %w", err)
	}

	if err := tx.Commit(); err != nil {
		return Session{}, fmt.Errorf("commit session: %w", err)
	}

	return Session{
		ID:                 sessionID,
		RepositoryID:       repository.ID,
		RepositoryName:     repository.DisplayName,
		RepositoryIdentity: repository.CanonicalIdentity,
		Title:              title,
		CreatedAt:          createdAt,
	}, nil
}

// GetSession returns one persisted session and its repository metadata.
func (store *RepositoryStore) GetSession(ctx context.Context, sessionID string) (Session, error) {
	if err := store.validate(); err != nil {
		return Session{}, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return Session{}, fmt.Errorf("get session: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	row := store.database.QueryRowContext(ctx, sessionQuery+` WHERE s.id = ?`, sessionID)
	session, err := scanSession(row)
	if errors.Is(err, sql.ErrNoRows) {
		return Session{}, fmt.Errorf("%w: %q", ErrSessionNotFound, sessionID)
	}
	if err != nil {
		return Session{}, fmt.Errorf("get session %q: %w", sessionID, err)
	}
	return session, nil
}

// GetRepositoryForSession returns the immutable repository metadata owning a
// session. It is used by audit and export projections that must retain the
// discovered Git directory without consulting a live worktree.
func (store *RepositoryStore) GetRepositoryForSession(ctx context.Context, sessionID string) (Repository, error) {
	if err := store.validate(); err != nil {
		return Repository{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var repository Repository
	var created string
	err := store.database.QueryRowContext(ctx, `
SELECT r.id, r.canonical_identity, r.display_name, r.discovered_git_dir, r.created_at
FROM repositories r JOIN sessions s ON s.repository_id = r.id WHERE s.id = ?`, strings.TrimSpace(sessionID)).Scan(
		&repository.ID, &repository.CanonicalIdentity, &repository.DisplayName, &repository.DiscoveredGitDir, &created)
	if errors.Is(err, sql.ErrNoRows) {
		return Repository{}, fmt.Errorf("%w for session: %q", ErrRepositoryNotFound, sessionID)
	}
	if err != nil {
		return Repository{}, fmt.Errorf("get repository for session %q: %w", sessionID, err)
	}
	parsed, parseErr := shared.ParseTimestamp(created)
	if parseErr != nil {
		return Repository{}, fmt.Errorf("parse repository creation time: %w", parseErr)
	}
	repository.CreatedAt = parsed
	return repository, nil
}

// ListSessions returns all persisted session metadata in deterministic order.
func (store *RepositoryStore) ListSessions(ctx context.Context) ([]Session, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}

	rows, err := store.database.QueryContext(ctx, sessionQuery+` ORDER BY s.created_at ASC, s.id ASC`)
	if err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	defer rows.Close()

	sessions := make([]Session, 0)
	for rows.Next() {
		session, err := scanSession(rows)
		if err != nil {
			return nil, fmt.Errorf("list sessions: %w", err)
		}
		sessions = append(sessions, session)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list sessions: %w", err)
	}
	return sessions, nil
}

// ListSessionsForRepository returns persisted sessions belonging to one
// canonical repository identity.
func (store *RepositoryStore) ListSessionsForRepository(ctx context.Context, canonicalIdentity string) ([]Session, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	canonicalIdentity = strings.TrimSpace(canonicalIdentity)
	if canonicalIdentity == "" {
		return nil, fmt.Errorf("list sessions: repository identity is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	rows, err := store.database.QueryContext(ctx, sessionQuery+` WHERE r.canonical_identity = ? ORDER BY s.created_at ASC, s.id ASC`, canonicalIdentity)
	if err != nil {
		return nil, fmt.Errorf("list sessions for repository: %w", err)
	}
	defer rows.Close()

	sessions := make([]Session, 0)
	for rows.Next() {
		session, err := scanSession(rows)
		if err != nil {
			return nil, fmt.Errorf("list sessions for repository: %w", err)
		}
		sessions = append(sessions, session)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list sessions for repository: %w", err)
	}
	return sessions, nil
}

// DeleteSession explicitly deletes one session. An unknown ID is an error and
// leaves all persisted state untouched.
func (store *RepositoryStore) DeleteSession(ctx context.Context, sessionID string) error {
	if err := store.validate(); err != nil {
		return err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return fmt.Errorf("delete session: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin delete session transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	result, err := tx.ExecContext(ctx, `DELETE FROM sessions WHERE id = ?`, sessionID)
	if err != nil {
		return fmt.Errorf("delete session %q: %w", sessionID, err)
	}
	deleted, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check deleted session %q: %w", sessionID, err)
	}
	if deleted == 0 {
		return fmt.Errorf("%w: %q", ErrSessionNotFound, sessionID)
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit delete session: %w", err)
	}
	return nil
}

// DeleteSessionForRepository deletes one session only when it belongs to the
// supplied repository identity.
func (store *RepositoryStore) DeleteSessionForRepository(ctx context.Context, canonicalIdentity, sessionID string) error {
	if err := store.validate(); err != nil {
		return err
	}
	canonicalIdentity = strings.TrimSpace(canonicalIdentity)
	sessionID = strings.TrimSpace(sessionID)
	if canonicalIdentity == "" {
		return fmt.Errorf("delete session: repository identity is empty")
	}
	if sessionID == "" {
		return fmt.Errorf("delete session: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin delete session transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	result, err := tx.ExecContext(ctx, `
DELETE FROM sessions
WHERE id = ?
  AND repository_id = (
      SELECT id FROM repositories WHERE canonical_identity = ?
  )`, sessionID, canonicalIdentity)
	if err != nil {
		return fmt.Errorf("delete session %q: %w", sessionID, err)
	}
	deleted, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check deleted session %q: %w", sessionID, err)
	}
	if deleted == 0 {
		return fmt.Errorf("%w: %q", ErrSessionNotFound, sessionID)
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit delete session: %w", err)
	}
	return nil
}

// NewID returns a UUIDv4-style identifier without adding another dependency to
// the persistence layer.
func NewID() (string, error) {
	var bytes [16]byte
	if _, err := rand.Read(bytes[:]); err != nil {
		return "", err
	}
	bytes[6] = (bytes[6] & 0x0f) | 0x40
	bytes[8] = (bytes[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		bytes[0:4], bytes[4:6], bytes[6:8], bytes[8:10], bytes[10:16]), nil
}

func (store *RepositoryStore) validate() error {
	if store == nil || store.database == nil || store.database.DB == nil {
		return fmt.Errorf("repository store is nil")
	}
	if store.now == nil || store.newID == nil {
		return fmt.Errorf("repository store is not configured")
	}
	return nil
}

func normalizeIdentity(identity RepositoryIdentity) (RepositoryIdentity, error) {
	identity.CanonicalIdentity = strings.TrimSpace(identity.CanonicalIdentity)
	identity.DisplayName = strings.TrimSpace(identity.DisplayName)
	identity.DiscoveredGitDir = strings.TrimSpace(identity.DiscoveredGitDir)
	if identity.CanonicalIdentity == "" {
		return RepositoryIdentity{}, fmt.Errorf("%w: canonical identity is empty", ErrInvalidRepositoryIdentity)
	}
	if identity.DiscoveredGitDir == "" {
		return RepositoryIdentity{}, fmt.Errorf("%w: discovered Git directory is empty", ErrInvalidRepositoryIdentity)
	}
	if identity.DisplayName == "" {
		identity.DisplayName = filepath.Base(filepath.Clean(identity.CanonicalIdentity))
		if identity.DisplayName == "." || identity.DisplayName == string(filepath.Separator) || identity.DisplayName == "" {
			identity.DisplayName = identity.CanonicalIdentity
		}
	}
	return identity, nil
}

func (store *RepositoryStore) ensureRepositoryTx(ctx context.Context, tx *sql.Tx, identity RepositoryIdentity) (Repository, error) {
	repositoryID, err := store.newID()
	if err != nil {
		return Repository{}, fmt.Errorf("create repository ID: %w", err)
	}
	createdAt := store.now().UTC()
	if createdAt.IsZero() {
		return Repository{}, fmt.Errorf("create repository: clock returned zero time")
	}
	_, err = tx.ExecContext(ctx, `
INSERT INTO repositories (id, canonical_identity, display_name, discovered_git_dir, created_at)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT (canonical_identity) DO UPDATE SET
    display_name = excluded.display_name,
    discovered_git_dir = excluded.discovered_git_dir`,
		repositoryID,
		identity.CanonicalIdentity,
		identity.DisplayName,
		identity.DiscoveredGitDir,
		shared.TimestampString(createdAt),
	)
	if err != nil {
		return Repository{}, fmt.Errorf("persist repository identity: %w", err)
	}

	var repository Repository
	var createdAtRaw string
	if err := tx.QueryRowContext(ctx, `
SELECT id, canonical_identity, display_name, discovered_git_dir, created_at
FROM repositories
WHERE canonical_identity = ?`, identity.CanonicalIdentity).Scan(
		&repository.ID,
		&repository.CanonicalIdentity,
		&repository.DisplayName,
		&repository.DiscoveredGitDir,
		&createdAtRaw,
	); err != nil {
		return Repository{}, fmt.Errorf("read repository identity: %w", err)
	}
	repository.CreatedAt, err = shared.ParseTimestamp(createdAtRaw)
	if err != nil {
		return Repository{}, fmt.Errorf("read repository creation time: %w", err)
	}
	return repository, nil
}

const sessionQuery = `
SELECT s.id, s.repository_id, r.display_name, r.canonical_identity,
       s.title, s.created_at, COALESCE(s.current_round_id, '')
FROM sessions s
JOIN repositories r ON r.id = s.repository_id`

type scanner interface {
	Scan(dest ...any) error
}

func scanSession(row scanner) (Session, error) {
	var session Session
	var createdAtRaw string
	if err := row.Scan(
		&session.ID,
		&session.RepositoryID,
		&session.RepositoryName,
		&session.RepositoryIdentity,
		&session.Title,
		&createdAtRaw,
		&session.CurrentRoundID,
	); err != nil {
		return Session{}, err
	}
	var err error
	session.CreatedAt, err = shared.ParseTimestamp(createdAtRaw)
	if err != nil {
		return Session{}, fmt.Errorf("parse session creation time: %w", err)
	}
	return session, nil
}
