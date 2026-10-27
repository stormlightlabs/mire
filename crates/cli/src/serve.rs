//! Loopback HTTP server for the browser review surface.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Request, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN,
};
use axum::http::{HeaderMap, HeaderValue, Method, Response, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post, put};
use axum::{Router, extract::MatchedPath};
use base64::Engine;
use include_dir::{Dir, include_dir};
use mire_core::{
    AnnotationKind, Author, ChangesetSource, FileContent, FileDiff, Hunk, NoteId, NoteSeverity, NoteStatus, Review,
    ReviewNote, ReviewRevision, ViewedMap,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::{OpenApi, ToSchema};

use crate::cli::ServeArgs;
use crate::protocol::{ContextSelection, ProtocolError, context_json, notes_json, notes_markdown};
use crate::refresh::{RefreshError, refresh_review};
use crate::review_file::{ReviewFileError, read_review};
use crate::watch::WatchSet;

mod history;
mod viewed;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/web");

const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;
/// Byte limit on the browser's agent context export. A maximum description
/// serializes to at most 256 KiB (65,536 four-byte characters; escaped
/// characters take two bytes) and a maximum title to 2 KiB, so 1 MiB leaves
/// room for the file and note manifests.
const MAX_CONTEXT_EXPORT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("cannot read review: {0}")]
    Review(ReviewFileError),
    #[error("cannot bind loopback server: {0}")]
    Bind(std::io::Error),
    #[error("cannot run loopback server: {0}")]
    Run(std::io::Error),
    #[error("cannot open the session URL: {0}")]
    OpenBrowser(std::io::Error),
}

#[derive(Clone)]
struct AppState {
    review_identity: String,
    review_path: PathBuf,
    session_secret: Arc<str>,
    origin: Arc<str>,
    request_ids: Arc<AtomicU64>,
    events: broadcast::Sender<ServerEvent>,
    watch_status: Arc<Mutex<WatchStatus>>,
    /// Viewed marks as of the last `Viewed` event or revision change. The
    /// watcher and the viewed routes both compare against it, so one change
    /// sends one event.
    announced_viewed: Arc<Mutex<ViewedMap>>,
    /// The blobs the bound repository held at the last check, per revision.
    held_blobs: Arc<history::HeldBlobCache>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewOverview {
    review_identity: String,
    revision: u64,
    source: String,
    /// Caller-supplied one-line title, such as a pull request title.
    title: Option<String>,
    /// Caller-supplied plain-text description, such as a pull request body.
    description: Option<String>,
    files: Vec<FileSummary>,
    findings: Vec<FindingSummary>,
    totals: ReviewTotals,
    changes: ChangeTotals,
    reanchor: ReanchorTotals,
    readiness: ReadinessSummary,
    watch: WatchStatus,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FileSummary {
    id: String,
    path: PathText,
    status: String,
    content_kind: String,
    open_findings: usize,
    /// The local reader's viewed mark for this path, or `null`. The file is
    /// viewed while the mark's fingerprint equals `id`.
    viewed: Option<viewed::ViewedState>,
    /// The revisions that changed this file, newest first.
    history: Vec<history::HistoryEntry>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FileDetail {
    id: String,
    path: PathText,
    old_path: Option<PathText>,
    status: String,
    content: SemanticFileContent,
    findings: Vec<FindingSummary>,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SemanticFileContent {
    Text { hunks: Vec<SemanticHunk> },
    Binary,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SemanticHunk {
    old_start: u64,
    old_line_count: u64,
    new_start: u64,
    new_line_count: u64,
    section: DisplayText,
    lines: Vec<SemanticLine>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct SemanticLine {
    kind: String,
    old_line: Option<u64>,
    new_line: Option<u64>,
    content: DisplayText,
    missing_newline: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FindingSummary {
    id: String,
    path: PathText,
    side: String,
    start_line: u64,
    end_line: u64,
    anchor_state: String,
    navigable: bool,
    severity: String,
    annotation_kind: String,
    status: String,
    body: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FindingDetail {
    id: String,
    path: PathText,
    side: String,
    start_line: u64,
    end_line: u64,
    anchor_state: String,
    navigable: bool,
    severity: String,
    annotation_kind: String,
    status: String,
    body: String,
    author: FindingAuthor,
    provenance: String,
    /// The checkpoint row the finding was written on, or `null` for findings
    /// written before checkpoints.
    checkpoint: Option<history::CheckpointLink>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FindingAuthor {
    id: String,
    display_name: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DisplayText {
    display: String,
    lossy: bool,
}

/// A repository path: display text, and its raw bytes as lowercase hex for
/// routes that take a `path` query parameter.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PathText {
    display: String,
    lossy: bool,
    raw: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewTotals {
    files: usize,
    findings: usize,
    open: usize,
    resolved: usize,
    dismissed: usize,
    accepted_risk: usize,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ChangeTotals {
    additions: usize,
    deletions: usize,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReanchorTotals {
    captured: usize,
    exact: usize,
    moved: usize,
    stale: usize,
    ambiguous: usize,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReadinessSummary {
    ready: bool,
    open_findings: usize,
    unsafe_anchors: usize,
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum WatchStatus {
    Watching,
    Unavailable,
    Degraded,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ServerEvent {
    kind: ServerEventKind,
    revision: Option<u64>,
    watch: WatchStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ServerEventKind {
    ReviewInvalidated,
    Refresh,
    Viewed,
    WatchStatus,
    Shutdown,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RefreshResponse {
    revision: u64,
    status: String,
    reanchor: ReanchorTotals,
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct Problem {
    code: String,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_revision: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditFindingRequest {
    expected_revision: u64,
    body: String,
    severity: WebSeverity,
    annotation_kind: WebAnnotationKind,
}

#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum WebSeverity {
    Note,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum WebAnnotationKind {
    Comment,
    Defect,
    Suggestion,
    Question,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FindingDecisionRequest {
    expected_revision: u64,
    decision: FindingDecision,
}

#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
enum FindingDecision {
    Resolve,
    Reopen,
    Dismiss,
    AcceptRisk,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FindingMutation {
    revision: u64,
    finding: FindingDetail,
}

#[derive(Debug)]
enum FindingMutationError {
    Conflict { actual_revision: u64 },
    NotFound,
    InvalidInput,
    Locked,
    Unavailable,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_review,
        get_file,
        get_finding,
        edit_finding,
        decide_finding,
        refresh,
        get_events,
        export_notes_json,
        export_notes_markdown,
        export_context,
        get_openapi,
        viewed::put_viewed,
        viewed::delete_viewed,
        history::get_history,
        history::get_finding_checkpoint
    ),
    components(schemas(
        ReviewOverview,
        FileSummary,
        FileDetail,
        SemanticFileContent,
        SemanticHunk,
        SemanticLine,
        FindingSummary,
        FindingDetail,
        FindingAuthor,
        DisplayText,
        ReviewTotals,
        ChangeTotals,
        ReanchorTotals,
        ReadinessSummary,
        WatchStatus,
        RefreshResponse,
        EditFindingRequest,
        FindingDecisionRequest,
        FindingMutation,
        WebSeverity,
        WebAnnotationKind,
        FindingDecision,
        PathText,
        viewed::ViewedState,
        viewed::MarkViewedRequest,
        viewed::ViewedResponse,
        history::HistoryEntry,
        history::HistoryResult,
        history::HistoryResponse,
        history::CheckpointResponse,
        history::CheckpointLink,
        Problem
    ))
)]
struct ApiDocument;

/// Starts a loopback-only browser review server until the process receives Ctrl-C.
pub async fn run(arguments: ServeArgs) -> Result<(), ServeError> {
    let review_path = PathBuf::from(arguments.review);
    read_review(&review_path).map_err(ServeError::Review)?;

    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, arguments.port.unwrap_or(0))))
        .await
        .map_err(ServeError::Bind)?;
    let address = listener.local_addr().map_err(ServeError::Bind)?;
    let origin: Arc<str> = format!("http://{address}").into();
    let secret = session_secret().map_err(ServeError::Bind)?;
    let session_url = format!("{origin}/#{secret}");
    let (events, _) = broadcast::channel(64);
    // Read the starting revision before the URL is printed, so the watcher
    // compares any later write against it.
    let initial_review = read_review(&review_path).map_err(ServeError::Review)?;
    let initial_revision = initial_review.revision().get();
    let initial_watch_status = if initial_review.source_binding().is_some() {
        WatchStatus::Watching
    } else {
        WatchStatus::Unavailable
    };
    let state = AppState {
        review_identity: review_identity(&review_path).map_err(ServeError::Bind)?,
        review_path,
        session_secret: secret.into(),
        origin,
        request_ids: Arc::new(AtomicU64::new(1)),
        events,
        watch_status: Arc::new(Mutex::new(initial_watch_status)),
        announced_viewed: Arc::new(Mutex::new(initial_review.viewed().clone())),
        held_blobs: Arc::default(),
    };
    let (shutdown, shutdown_receiver) = watch::channel(false);
    let watcher = start_watcher(state.clone(), initial_revision, shutdown_receiver);

    println!("Mire review server: {session_url}");
    if arguments.open {
        open::that_detached(&session_url).map_err(ServeError::OpenBrowser)?;
    }

    let server = axum::serve(listener, app(state.clone()))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(ServeError::Run);
    let _ = state
        .events
        .send(server_event(ServerEventKind::Shutdown, None, watch_status(&state)));
    let _ = shutdown.send(true);
    if let Err(error) = watcher.await {
        tracing::error!(error = %error, "Mire watcher task stopped unexpectedly");
    }
    server
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/review", get(get_review))
        .route("/api/v1/files/{file_id}", get(get_file))
        .route("/api/v1/findings/{note_id}", get(get_finding).patch(edit_finding))
        .route("/api/v1/findings/{note_id}/decision", post(decide_finding))
        .route(
            "/api/v1/findings/{note_id}/checkpoint",
            get(history::get_finding_checkpoint),
        )
        .route("/api/v1/history", get(history::get_history))
        .route("/api/v1/refresh", post(refresh))
        .route("/api/v1/viewed", put(viewed::put_viewed).delete(viewed::delete_viewed))
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/exports/notes.json", get(export_notes_json))
        .route("/api/v1/exports/notes.md", get(export_notes_markdown))
        .route("/api/v1/exports/context.json", get(export_context))
        .route("/api/v1/openapi.json", get(get_openapi))
        .fallback(get(static_asset))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request| {
                    let route = request
                        .extensions()
                        .get::<MatchedPath>()
                        .map_or("<unmatched>", MatchedPath::as_str);
                    tracing::info_span!("mire.http", method = %request.method(), route)
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(middleware::from_fn_with_state(state.clone(), secure_request))
        .with_state(state)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    eprintln!("Shutting down Mire review server…");
}

fn start_watcher(state: AppState, initial_revision: u64, shutdown: watch::Receiver<bool>) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || watch_review(state, initial_revision, shutdown))
}

fn watch_review(state: AppState, initial_revision: u64, shutdown: watch::Receiver<bool>) {
    let review_parent = watched_file_parent(&state.review_path);
    let mut review_watcher = match WatchSet::new(&review_parent, false) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::warn!(error = %error, "Mire review-file watching is unavailable");
            set_watch_status(&state, WatchStatus::Degraded);
            return;
        }
    };
    let (mut source_watcher, mut source_degraded) = match create_source_watcher(&state) {
        Ok(watcher) => (watcher, false),
        Err(error) => {
            tracing::warn!(error = %error, "Mire source watching is unavailable");
            (None, true)
        }
    };
    set_watch_status(
        &state,
        if source_degraded {
            WatchStatus::Degraded
        } else if source_watcher.is_some() {
            WatchStatus::Watching
        } else {
            WatchStatus::Unavailable
        },
    );
    let mut source_retry = Instant::now() + Duration::from_secs(2);
    let mut revision = Some(initial_revision);
    // The review file is checked once after the watchers exist, which catches
    // a write that landed before they could report it.
    let mut first_pass = true;

    while !*shutdown.borrow() {
        if source_watcher.is_none() && Instant::now() >= source_retry {
            source_retry = Instant::now() + Duration::from_secs(2);
            match create_source_watcher(&state) {
                Ok(Some(watcher)) => {
                    source_watcher = Some(watcher);
                    source_degraded = false;
                    set_watch_status(&state, WatchStatus::Watching);
                }
                Ok(None) if !source_degraded => set_watch_status(&state, WatchStatus::Unavailable),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(error = %error, "Mire source watcher recovery failed");
                    source_degraded = true;
                    set_watch_status(&state, WatchStatus::Degraded);
                }
            }
        }
        let review_due = review_watcher.reload_due() | std::mem::take(&mut first_pass);
        if let Some(error) = review_watcher.take_error() {
            tracing::warn!(error = %error, "Mire review-file watcher failed");
            set_watch_status(&state, WatchStatus::Degraded);
        }
        let source_due = source_watcher.as_mut().is_some_and(WatchSet::reload_due);
        if let Some(error) = source_watcher.as_mut().and_then(WatchSet::take_error) {
            tracing::warn!(error = %error, "Mire source watcher failed");
            source_watcher = None;
            source_degraded = true;
            set_watch_status(&state, WatchStatus::Degraded);
        }

        if review_due {
            match read_review(&state.review_path) {
                Ok(review) if revision != Some(review.revision().get()) => {
                    announce_revision(&state, &mut revision, review.revision().get(), false);
                    remember_viewed(&state, review.viewed());
                }
                Ok(review) => {
                    announce_viewed(&state, &review);
                    if !source_degraded {
                        set_watch_status(
                            &state,
                            if source_watcher.is_some() { WatchStatus::Watching } else { WatchStatus::Unavailable },
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "Mire review-file reload failed");
                    set_watch_status(&state, WatchStatus::Degraded);
                }
            }
        }
        if source_due {
            match refresh_review(&state.review_path) {
                Ok(result) => {
                    announce_revision(
                        &state,
                        &mut revision,
                        result.review().revision().get(),
                        result.changed(),
                    );
                    remember_viewed(&state, result.review().viewed());
                    if source_watcher.is_some() {
                        source_degraded = false;
                        set_watch_status(&state, WatchStatus::Watching);
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "Mire source refresh failed");
                    source_degraded = true;
                    set_watch_status(&state, WatchStatus::Degraded);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Records `current` as the announced revision and publishes it when it is new.
/// A refresh that changed the review publishes `Refresh`; any other new
/// revision, such as a write that landed while a refresh found nothing to do,
/// publishes `ReviewInvalidated`.
fn announce_revision(state: &AppState, announced: &mut Option<u64>, current: u64, refreshed: bool) {
    let kind = if refreshed {
        ServerEventKind::Refresh
    } else if *announced != Some(current) {
        ServerEventKind::ReviewInvalidated
    } else {
        return;
    };
    *announced = Some(current);
    publish(state, kind, Some(current));
}

/// Publishes `Viewed` when the review's marks differ from the ones last
/// announced. Marks never advance the revision, so a mark written by the CLI
/// or the terminal reaches open tabs only this way. A viewed route calls it
/// after its write, and the watcher then finds nothing new.
fn announce_viewed(state: &AppState, review: &Review) {
    let changed = state.announced_viewed.lock().is_ok_and(|mut known| {
        if *known == *review.viewed() {
            false
        } else {
            *known = review.viewed().clone();
            true
        }
    });
    if changed {
        publish(state, ServerEventKind::Viewed, Some(review.revision().get()));
    }
}

/// Records marks that arrived with a new revision. The revision's own event
/// already tells tabs to reload them.
fn remember_viewed(state: &AppState, viewed: &ViewedMap) {
    if let Ok(mut known) = state.announced_viewed.lock() {
        known.clone_from(viewed);
    }
}

fn create_source_watcher(state: &AppState) -> Result<Option<WatchSet>, crate::watch::WatchError> {
    let review = match read_review(&state.review_path) {
        Ok(review) => review,
        Err(_) => return Ok(None),
    };
    let Some(binding) = review.source_binding() else {
        return Ok(None);
    };
    let root = match crate::git::bound_repository_root(binding) {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(error = %error, "Mire source binding is unavailable");
            return Ok(None);
        }
    };
    WatchSet::new(&root, true).map(Some)
}

fn watched_file_parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_owned()
}

fn watch_status(state: &AppState) -> WatchStatus {
    state
        .watch_status
        .lock()
        .map_or(WatchStatus::Degraded, |status| *status)
}

fn set_watch_status(state: &AppState, status: WatchStatus) {
    let changed = state.watch_status.lock().is_ok_and(|mut current| {
        if *current == status {
            false
        } else {
            *current = status;
            true
        }
    });
    if changed {
        publish(state, ServerEventKind::WatchStatus, None);
    }
}

fn publish(state: &AppState, kind: ServerEventKind, revision: Option<u64>) {
    let _ = state.events.send(server_event(kind, revision, watch_status(state)));
}

fn server_event(kind: ServerEventKind, revision: Option<u64>, watch: WatchStatus) -> ServerEvent {
    ServerEvent { kind, revision, watch }
}

async fn secure_request(State(state): State<AppState>, request: Request, next: Next) -> Response<Body> {
    let request_id = state.request_ids.fetch_add(1, Ordering::Relaxed);
    let started = Instant::now();
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("<unmatched>", MatchedPath::as_str)
        .to_owned();

    let response = if !valid_host(request.headers(), &state.origin) {
        problem(
            StatusCode::BAD_REQUEST,
            "unexpected_host",
            "This server only accepts its loopback host.",
        )
    } else if request.uri().path().starts_with("/api/v1/") && !authorized(request.headers(), &state.session_secret) {
        problem(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "This API requires the session secret.",
        )
    } else if is_state_changing(&method) && !valid_origin(request.headers(), &state.origin) {
        problem(
            StatusCode::FORBIDDEN,
            "unexpected_origin",
            "This request must come from the local review page.",
        )
    } else {
        next.run(request).await
    };

    let status = response.status();
    tracing::info!(
        request_id,
        method = %method,
        route,
        status = %status,
        latency_ms = started.elapsed().as_millis(),
        "Mire HTTP request"
    );
    secure_headers(response)
}

#[utoipa::path(
    get,
    path = "/api/v1/review",
    responses(
        (status = 200, description = "Current review overview", body = ReviewOverview),
        (status = 401, description = "Missing or invalid session secret", body = Problem)
    )
)]
async fn get_review(State(state): State<AppState>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => {
            let identity = state.review_identity.clone();
            let cache = state.held_blobs.clone();
            let overview = tokio::task::spawn_blocking(move || {
                let held = cache.held(&review);
                review_overview(&identity, &review, local_author().id(), held.as_deref())
            })
            .await;
            match overview {
                Ok(mut overview) => {
                    overview.watch = watch_status(&state);
                    Json(overview).into_response()
                }
                Err(_) => problem(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "review_unavailable",
                    "The review file could not be read.",
                ),
            }
        }
        Err(response) => *response,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{file_id}",
    params(("file_id" = String, Path, description = "Opaque file fingerprint")),
    responses(
        (status = 200, description = "One semantic file diff and its anchored findings", body = FileDetail),
        (status = 404, description = "No file has this identity", body = Problem)
    )
)]
async fn get_file(State(state): State<AppState>, AxumPath(file_id): AxumPath<String>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => review
            .changeset()
            .files()
            .iter()
            .find(|file| file.fingerprint().to_string() == file_id)
            .map(|file| Json(FileDetail::from_file(file, &review)).into_response())
            .unwrap_or_else(|| {
                problem(
                    StatusCode::NOT_FOUND,
                    "file_not_found",
                    "This file is not part of the review.",
                )
            }),
        Err(response) => *response,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/findings/{note_id}",
    params(("note_id" = String, Path, description = "Stable finding identifier")),
    responses(
        (status = 200, description = "Complete finding detail", body = FindingDetail),
        (status = 404, description = "No finding has this identity", body = Problem)
    )
)]
async fn get_finding(State(state): State<AppState>, AxumPath(note_id): AxumPath<String>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => review
            .notes()
            .iter()
            .find(|note| note.id().as_str() == note_id)
            .map(|note| Json(FindingDetail::from_note(note)).into_response())
            .unwrap_or_else(|| {
                problem(
                    StatusCode::NOT_FOUND,
                    "finding_not_found",
                    "This finding is not part of the review.",
                )
            }),
        Err(response) => *response,
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/findings/{note_id}",
    params(("note_id" = String, Path, description = "Stable finding identifier")),
    request_body = EditFindingRequest,
    responses(
        (status = 200, description = "Updated finding", body = FindingMutation),
        (status = 409, description = "The supplied revision is no longer current", body = Problem)
    )
)]
async fn edit_finding(
    State(state): State<AppState>, AxumPath(note_id): AxumPath<String>,
    axum::Json(request): axum::Json<EditFindingRequest>,
) -> Response<Body> {
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || {
        mutate_finding(&path, &note_id, request.expected_revision, FindingChange::Edit(request))
    })
    .await
    {
        Ok(Ok(mutation)) => {
            publish(&state, ServerEventKind::ReviewInvalidated, Some(mutation.revision));
            Json(mutation).into_response()
        }
        Ok(Err(error)) => finding_mutation_problem(error),
        Err(_) => finding_mutation_problem(FindingMutationError::Unavailable),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/findings/{note_id}/decision",
    params(("note_id" = String, Path, description = "Stable finding identifier")),
    request_body = FindingDecisionRequest,
    responses(
        (status = 200, description = "Finding with its recorded decision", body = FindingMutation),
        (status = 409, description = "The supplied revision is no longer current", body = Problem)
    )
)]
async fn decide_finding(
    State(state): State<AppState>, AxumPath(note_id): AxumPath<String>,
    axum::Json(request): axum::Json<FindingDecisionRequest>,
) -> Response<Body> {
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || {
        mutate_finding(
            &path,
            &note_id,
            request.expected_revision,
            FindingChange::Decision(request.decision),
        )
    })
    .await
    {
        Ok(Ok(mutation)) => {
            publish(&state, ServerEventKind::ReviewInvalidated, Some(mutation.revision));
            Json(mutation).into_response()
        }
        Ok(Err(error)) => finding_mutation_problem(error),
        Err(_) => finding_mutation_problem(FindingMutationError::Unavailable),
    }
}

enum FindingChange {
    Edit(EditFindingRequest),
    Decision(FindingDecision),
}

fn mutate_finding(
    path: &Path, note_id: &str, expected_revision: u64, change: FindingChange,
) -> Result<FindingMutation, FindingMutationError> {
    let review = read_review(path).map_err(|_| FindingMutationError::Unavailable)?;
    if review.revision().get() != expected_revision {
        return Err(FindingMutationError::Conflict { actual_revision: review.revision().get() });
    }
    let expected = ReviewRevision::new(expected_revision).map_err(|_| FindingMutationError::InvalidInput)?;
    let note_id = NoteId::new(note_id.to_owned()).map_err(|_| FindingMutationError::InvalidInput)?;
    let updated = match change {
        FindingChange::Edit(request) => review.edit_note(
            &note_id,
            request.body,
            request.severity.into(),
            request.annotation_kind.into(),
        ),
        FindingChange::Decision(decision) => review.change_note_status(&note_id, decision.into(), local_author()),
    }
    .map_err(|error| match error {
        mire_core::ReviewError::NoteNotFound(_) => FindingMutationError::NotFound,
        _ => FindingMutationError::InvalidInput,
    })?;

    if updated != review {
        match crate::review_file::write_review_atomic_if_revision(path, expected, &updated) {
            Ok(()) => {}
            Err(ReviewFileError::RevisionConflict { actual, .. }) => {
                return Err(FindingMutationError::Conflict { actual_revision: actual });
            }
            Err(ReviewFileError::Locked { .. }) => return Err(FindingMutationError::Locked),
            Err(_) => return Err(FindingMutationError::Unavailable),
        }
    }
    let note = updated
        .notes()
        .iter()
        .find(|note| note.id() == &note_id)
        .expect("a successful note mutation retains the target finding");
    Ok(FindingMutation { revision: updated.revision().get(), finding: FindingDetail::from_note(note) })
}

fn finding_mutation_problem(error: FindingMutationError) -> Response<Body> {
    match error {
        FindingMutationError::Conflict { actual_revision } => revision_conflict(actual_revision),
        FindingMutationError::NotFound => problem(
            StatusCode::NOT_FOUND,
            "finding_not_found",
            "This finding is not part of the review.",
        ),
        FindingMutationError::InvalidInput => problem(
            StatusCode::BAD_REQUEST,
            "invalid_finding",
            "The finding edit or decision is not valid for this review.",
        ),
        FindingMutationError::Locked => problem(
            StatusCode::LOCKED,
            "review_locked",
            "The review is being updated by another process. Try again shortly.",
        ),
        FindingMutationError::Unavailable => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read or updated.",
        ),
    }
}

fn local_author() -> Author {
    let identifier = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .unwrap_or_else(|| "local-human".to_owned());
    Author::new(identifier, None).expect("the fallback author is valid")
}

impl From<WebSeverity> for NoteSeverity {
    fn from(value: WebSeverity) -> Self {
        match value {
            WebSeverity::Note => Self::Note,
            WebSeverity::Low => Self::Low,
            WebSeverity::Medium => Self::Medium,
            WebSeverity::High => Self::High,
            WebSeverity::Critical => Self::Critical,
        }
    }
}

impl From<WebAnnotationKind> for AnnotationKind {
    fn from(value: WebAnnotationKind) -> Self {
        match value {
            WebAnnotationKind::Comment => Self::Comment,
            WebAnnotationKind::Defect => Self::Defect,
            WebAnnotationKind::Suggestion => Self::Suggestion,
            WebAnnotationKind::Question => Self::Question,
        }
    }
}

impl From<FindingDecision> for NoteStatus {
    fn from(value: FindingDecision) -> Self {
        match value {
            FindingDecision::Resolve => Self::Resolved,
            FindingDecision::Reopen => Self::Open,
            FindingDecision::Dismiss => Self::Dismissed,
            FindingDecision::AcceptRisk => Self::AcceptedRisk,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/refresh",
    responses(
        (status = 200, description = "Refreshed review and re-anchor summary", body = RefreshResponse),
        (status = 409, description = "The review was changed repeatedly during refresh", body = Problem),
        (status = 422, description = "The review has no usable source binding", body = Problem)
    )
)]
async fn refresh(State(state): State<AppState>) -> Response<Body> {
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || refresh_review(&path)).await {
        Ok(Ok(result)) => {
            let review = result.review();
            let response = RefreshResponse {
                revision: review.revision().get(),
                status: if result.changed() { "refreshed" } else { "unchanged" }.to_owned(),
                reanchor: reanchor_totals(review),
            };
            publish(&state, ServerEventKind::Refresh, Some(response.revision));
            Json(response).into_response()
        }
        Ok(Err(error)) => refresh_problem(error),
        Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "refresh_unavailable",
            "The source refresh could not finish.",
        ),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    responses((status = 200, description = "Authenticated review invalidation event stream"))
)]
async fn get_events(State(state): State<AppState>) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(|event| {
        event.ok().map(|event| {
            let data = serde_json::to_string(&event).unwrap_or_else(|_| "{\"kind\":\"watch_status\"}".to_owned());
            Ok(Event::default().event("mire").data(data))
        })
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keepalive"))
}

#[utoipa::path(
    get,
    path = "/api/v1/exports/notes.json",
    responses((status = 200, description = "Deterministic notes JSON download"))
)]
async fn export_notes_json(State(state): State<AppState>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => match notes_json(&review) {
            Ok(bytes) => download(bytes, "application/json; charset=utf-8", "mire-notes.json"),
            Err(error) => protocol_problem(error),
        },
        Err(response) => *response,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/exports/notes.md",
    responses((status = 200, description = "Deterministic notes Markdown download"))
)]
async fn export_notes_markdown(State(state): State<AppState>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => download(notes_markdown(&review), "text/markdown; charset=utf-8", "mire-notes.md"),
        Err(response) => *response,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/exports/context.json",
    responses((status = 200, description = "Bounded agent context JSON download"))
)]
async fn export_context(State(state): State<AppState>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => match context_json(
            &review,
            ContextSelection::Manifest { reader: None },
            Some(MAX_CONTEXT_EXPORT_BYTES),
        ) {
            Ok(bytes) => download(bytes, "application/json; charset=utf-8", "mire-context.json"),
            Err(error) => protocol_problem(error),
        },
        Err(response) => *response,
    }
}

async fn load_current_review(state: &AppState) -> Result<Review, Box<Response<Body>>> {
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || read_review(&path)).await {
        Ok(Ok(review)) => Ok(review),
        Ok(Err(_)) | Err(_) => Err(Box::new(problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read.",
        ))),
    }
}

fn refresh_problem(error: RefreshError) -> Response<Body> {
    match error {
        RefreshError::UnavailableSource => problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "source_unavailable",
            "This review has no source binding that can be refreshed.",
        ),
        RefreshError::Review(ReviewFileError::Locked { .. } | ReviewFileError::RevisionConflict { .. }) => problem(
            StatusCode::CONFLICT,
            "revision_conflict",
            "The review changed during refresh. Reload and try again.",
        ),
        RefreshError::Review(_) | RefreshError::Git(_) | RefreshError::Reanchor(_) => problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "refresh_failed",
            "Mire could not refresh the bound source. The current review is still available.",
        ),
    }
}

fn protocol_problem(error: ProtocolError) -> Response<Body> {
    tracing::warn!(error = %error, "Mire export failed");
    problem(
        StatusCode::INTERNAL_SERVER_ERROR,
        "export_unavailable",
        "The requested export could not be created.",
    )
}

fn download(bytes: Vec<u8>, content_type: &'static str, filename: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).expect("static filename is valid"),
    );
    response
}

#[utoipa::path(
    get,
    path = "/api/v1/openapi.json",
    responses((status = 200, description = "OpenAPI document"))
)]
async fn get_openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDocument::openapi())
}

async fn static_asset(uri: Uri) -> Response<Body> {
    if uri.path().starts_with("/api/") {
        return problem(StatusCode::NOT_FOUND, "not_found", "This API resource does not exist.");
    }

    let requested = uri.path().trim_start_matches('/');
    let file = if requested.is_empty() {
        ASSETS.get_file("200.html")
    } else {
        ASSETS.get_file(requested).or_else(|| ASSETS.get_file("200.html"))
    };
    let Some(file) = file else {
        return problem(
            StatusCode::NOT_FOUND,
            "asset_not_found",
            "The browser application is unavailable.",
        );
    };

    let cache_control = if requested.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(
            CONTENT_TYPE,
            content_type(file.path().extension().and_then(|extension| extension.to_str())),
        )
        .header(CACHE_CONTROL, cache_control)
        .body(Body::from(file.contents()))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    if let Some(hash) = inline_script_hash(file.contents()) {
        response.headers_mut().insert(
            "x-mire-inline-script-hash",
            HeaderValue::from_str(&hash).expect("base64 is a valid HTTP header value"),
        );
    }
    response
}

fn review_overview(
    identity: &str, review: &Review, reader: &str, held: Option<&BTreeSet<mire_core::ObjectId>>,
) -> ReviewOverview {
    let findings = review.notes().iter().map(FindingSummary::from_note).collect::<Vec<_>>();
    let files = review
        .changeset()
        .files()
        .iter()
        .map(|file| FileSummary::from_file(file, review, reader, held))
        .collect::<Vec<_>>();
    let mut totals = ReviewTotals {
        files: files.len(),
        findings: findings.len(),
        open: 0,
        resolved: 0,
        dismissed: 0,
        accepted_risk: 0,
    };
    for finding in review.notes() {
        match finding.status() {
            NoteStatus::Open => totals.open += 1,
            NoteStatus::Resolved => totals.resolved += 1,
            NoteStatus::Dismissed => totals.dismissed += 1,
            NoteStatus::AcceptedRisk => totals.accepted_risk += 1,
        }
    }
    let reanchor = reanchor_totals(review);
    let changes = change_totals(review);
    let readiness = ReadinessSummary {
        ready: totals.open == 0 && reanchor.stale == 0 && reanchor.ambiguous == 0,
        open_findings: totals.open,
        unsafe_anchors: reanchor.stale + reanchor.ambiguous,
    };

    ReviewOverview {
        review_identity: identity.to_owned(),
        revision: review.revision().get(),
        source: source_summary(review.changeset().source()),
        title: review.title().map(str::to_owned),
        description: review.description().map(str::to_owned),
        files,
        findings,
        totals,
        changes,
        reanchor,
        readiness,
        watch: WatchStatus::Unavailable,
    }
}

fn change_totals(review: &Review) -> ChangeTotals {
    let mut totals = ChangeTotals { additions: 0, deletions: 0 };
    for file in review.changeset().files() {
        let FileContent::Text { hunks } = file.content() else {
            continue;
        };
        for hunk in hunks {
            for line in hunk.lines() {
                match line.kind() {
                    mire_core::LineKind::Addition => totals.additions += 1,
                    mire_core::LineKind::Deletion => totals.deletions += 1,
                    mire_core::LineKind::Context => {}
                }
            }
        }
    }
    totals
}

fn reanchor_totals(review: &Review) -> ReanchorTotals {
    let mut totals = ReanchorTotals { captured: 0, exact: 0, moved: 0, stale: 0, ambiguous: 0 };
    for note in review.notes() {
        match note.reanchor_outcome() {
            None => totals.captured += 1,
            Some(mire_core::ReanchorOutcome::Exact { .. }) => totals.exact += 1,
            Some(mire_core::ReanchorOutcome::Moved { .. }) => totals.moved += 1,
            Some(mire_core::ReanchorOutcome::Stale { .. }) => totals.stale += 1,
            Some(mire_core::ReanchorOutcome::Ambiguous { .. }) => totals.ambiguous += 1,
        }
    }
    totals
}

impl FileSummary {
    fn from_file(file: &FileDiff, review: &Review, reader: &str, held: Option<&BTreeSet<mire_core::ObjectId>>) -> Self {
        let path = file_path(file);
        let open_findings = review
            .notes()
            .iter()
            .filter(|note| note.status() == NoteStatus::Open && finding_matches_file(note, file))
            .count();
        Self {
            id: file.fingerprint().to_string(),
            path: PathText::from_bytes(path),
            status: format!("{:?}", file.status()).to_lowercase(),
            content_kind: match file.content() {
                FileContent::Text { .. } => "text".to_owned(),
                FileContent::Binary => "binary".to_owned(),
            },
            open_findings,
            viewed: review
                .viewed_mark(reader, file.canonical_path())
                .map(viewed::ViewedState::from_mark),
            history: history::file_history(review, file.canonical_path(), held),
        }
    }
}

impl FileDetail {
    fn from_file(file: &FileDiff, review: &Review) -> Self {
        let content = SemanticFileContent::from_content(file.content());
        Self {
            id: file.fingerprint().to_string(),
            path: PathText::from_bytes(file_path(file)),
            old_path: file.old_side().map(|side| PathText::from_bytes(side.path.as_bytes())),
            status: format!("{:?}", file.status()).to_lowercase(),
            content,
            findings: review
                .notes()
                .iter()
                .filter(|note| finding_matches_file(note, file))
                .map(FindingSummary::from_note)
                .collect(),
        }
    }
}

impl SemanticFileContent {
    fn from_content(content: &FileContent) -> Self {
        match content {
            FileContent::Text { hunks } => Self::Text { hunks: hunks.iter().map(SemanticHunk::from_hunk).collect() },
            FileContent::Binary => Self::Binary,
        }
    }
}

impl SemanticHunk {
    fn from_hunk(hunk: &Hunk) -> Self {
        Self {
            old_start: hunk.old_start(),
            old_line_count: hunk.old_line_count(),
            new_start: hunk.new_start(),
            new_line_count: hunk.new_line_count(),
            section: DisplayText::from_bytes(hunk.section()),
            lines: hunk.lines().iter().map(SemanticLine::from_line).collect(),
        }
    }
}

impl SemanticLine {
    fn from_line(line: &mire_core::DiffLine) -> Self {
        Self {
            kind: format!("{:?}", line.kind()).to_lowercase(),
            old_line: line.old_line().map(|number| number.get()),
            new_line: line.new_line().map(|number| number.get()),
            content: DisplayText::from_bytes(line.content()),
            missing_newline: format!("{:?}", line.missing_newline()).to_lowercase(),
        }
    }
}

impl FindingSummary {
    fn from_note(note: &ReviewNote) -> Self {
        let (anchor, anchor_state, navigable) = finding_location(note);
        Self {
            id: note.id().as_str().to_owned(),
            path: PathText::from_bytes(anchor.path().as_bytes()),
            side: match anchor.side() {
                mire_core::AnchorSide::Old => "old".to_owned(),
                mire_core::AnchorSide::New => "new".to_owned(),
            },
            start_line: anchor.range().start().get(),
            end_line: anchor.range().end().get(),
            anchor_state,
            navigable,
            severity: note.severity().to_string().to_lowercase(),
            annotation_kind: note.annotation_kind().to_string(),
            status: note.status().to_string(),
            body: summarize(note.body()),
        }
    }
}

impl FindingDetail {
    fn from_note(note: &ReviewNote) -> Self {
        let (anchor, anchor_state, navigable) = finding_location(note);
        Self {
            id: note.id().as_str().to_owned(),
            path: PathText::from_bytes(anchor.path().as_bytes()),
            side: match anchor.side() {
                mire_core::AnchorSide::Old => "old".to_owned(),
                mire_core::AnchorSide::New => "new".to_owned(),
            },
            start_line: anchor.range().start().get(),
            end_line: anchor.range().end().get(),
            anchor_state,
            navigable,
            severity: note.severity().to_string().to_lowercase(),
            annotation_kind: note.annotation_kind().to_string(),
            status: note.status().to_string(),
            body: note.body().to_owned(),
            author: FindingAuthor {
                id: note.author().id().to_owned(),
                display_name: note.author().display_name().map(str::to_owned),
            },
            provenance: note.provenance().to_string(),
            checkpoint: note.anchor().checkpoint().map(history::CheckpointLink::from_reference),
        }
    }
}

impl DisplayText {
    fn from_bytes(bytes: &[u8]) -> Self {
        let display = String::from_utf8_lossy(bytes);
        Self { lossy: matches!(display, std::borrow::Cow::Owned(_)), display: display.into_owned() }
    }
}

impl PathText {
    fn from_bytes(bytes: &[u8]) -> Self {
        let DisplayText { display, lossy } = DisplayText::from_bytes(bytes);
        Self { display, lossy, raw: viewed::encode_hex(bytes) }
    }
}

fn file_path(file: &FileDiff) -> &[u8] {
    file.new_side()
        .or(file.old_side())
        .expect("validated file diff has a side")
        .path
        .as_bytes()
}

fn finding_location(note: &ReviewNote) -> (&mire_core::Anchor, String, bool) {
    match note.reanchor_outcome() {
        None => (note.anchor(), "captured".to_owned(), true),
        Some(mire_core::ReanchorOutcome::Exact { candidate, .. }) => (candidate.anchor(), "exact".to_owned(), true),
        Some(mire_core::ReanchorOutcome::Moved { candidate, .. }) => (candidate.anchor(), "moved".to_owned(), true),
        Some(mire_core::ReanchorOutcome::Stale { .. }) => (note.anchor(), "stale".to_owned(), false),
        Some(mire_core::ReanchorOutcome::Ambiguous { .. }) => (note.anchor(), "ambiguous".to_owned(), false),
    }
}

fn finding_matches_file(note: &ReviewNote, file: &FileDiff) -> bool {
    let (anchor, _, _) = finding_location(note);
    match anchor.side() {
        mire_core::AnchorSide::Old => file.old_side().is_some_and(|side| side.path == *anchor.path()),
        mire_core::AnchorSide::New => file.new_side().is_some_and(|side| side.path == *anchor.path()),
    }
}

fn summarize(body: &str) -> String {
    const LIMIT: usize = 180;
    let mut summary = body.lines().next().unwrap_or_default().trim().to_owned();
    if summary.chars().count() > LIMIT {
        summary = summary.chars().take(LIMIT.saturating_sub(1)).collect::<String>();
        summary.push('…');
    }
    summary
}

fn source_summary(source: &ChangesetSource) -> String {
    match source {
        ChangesetSource::Patch { .. } => "patch".to_owned(),
        ChangesetSource::Git { .. } => "Git comparison".to_owned(),
        ChangesetSource::DirectFiles => "direct files".to_owned(),
    }
}

fn valid_host(headers: &HeaderMap, origin: &str) -> bool {
    headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| origin.strip_prefix("http://") == Some(host))
}

fn authorized(headers: &HeaderMap, secret: &str) -> bool {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| candidate == secret)
}

fn is_state_changing(method: &Method) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
}

fn valid_origin(headers: &HeaderMap, origin: &str) -> bool {
    headers.get(ORIGIN).and_then(|value| value.to_str().ok()) == Some(origin)
}

fn secure_headers(mut response: Response<Body>) -> Response<Body> {
    let headers = response.headers_mut();
    let script_source = headers
        .remove("x-mire-inline-script-hash")
        .and_then(|value| value.to_str().ok().map(|hash| format!(" 'sha256-{hash}'")))
        .unwrap_or_default();
    let policy = format!(
        "default-src 'self'; base-uri 'none'; connect-src 'self'; form-action 'none'; frame-ancestors 'none'; img-src 'self'; script-src 'self'{script_source}; style-src 'self' 'unsafe-inline'"
    );
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&policy).expect("content security policy is a valid HTTP header value"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    response
}

fn inline_script_hash(bytes: &[u8]) -> Option<String> {
    let document = std::str::from_utf8(bytes).ok()?;
    let start = document.find("<script>")? + "<script>".len();
    let end = start + document[start..].find("</script>")?;
    Some(base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&document.as_bytes()[start..end])))
}

fn content_type(extension: Option<&str>) -> &'static str {
    match extension {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn review_identity(path: &Path) -> std::io::Result<String> {
    let canonical = std::fs::canonicalize(path)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_os_str().as_encoded_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn session_secret() -> std::io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn revision_conflict(actual_revision: u64) -> Response<Body> {
    (
        StatusCode::CONFLICT,
        Json(Problem {
            code: "revision_conflict".to_owned(),
            detail: "The review changed before this update could be saved. Reload and try again.".to_owned(),
            actual_revision: Some(actual_revision),
        }),
    )
        .into_response()
}

fn problem(status: StatusCode, code: &str, detail: &str) -> Response<Body> {
    (
        status,
        Json(Problem { code: code.to_owned(), detail: detail.to_owned(), actual_revision: None }),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
