//! Loopback HTTP server for the browser review surface.

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
use axum::routing::{get, post};
use axum::{Router, extract::MatchedPath};
use base64::Engine;
use include_dir::{Dir, include_dir};
use mire_core::{
    AnnotationKind, Author, ChangesetSource, FileContent, FileDiff, Hunk, NoteId, NoteSeverity, NoteStatus, Review,
    ReviewNote, ReviewRevision,
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

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/web");

const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

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
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewOverview {
    review_identity: String,
    revision: u64,
    source: String,
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
    path: DisplayText,
    status: String,
    content_kind: String,
    open_findings: usize,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FileDetail {
    id: String,
    path: DisplayText,
    old_path: Option<DisplayText>,
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
    path: DisplayText,
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
    path: DisplayText,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ServerEventKind {
    ReviewInvalidated,
    Refresh,
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
        get_openapi
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
    let initial_watch_status = if read_review(&review_path)
        .map_err(ServeError::Review)?
        .source_binding()
        .is_some()
    {
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
    };
    let (shutdown, shutdown_receiver) = watch::channel(false);
    let watcher = start_watcher(state.clone(), shutdown_receiver);

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
        .route("/api/v1/refresh", post(refresh))
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
}

fn start_watcher(state: AppState, shutdown: watch::Receiver<bool>) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || watch_review(state, shutdown))
}

fn watch_review(state: AppState, shutdown: watch::Receiver<bool>) {
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
    let mut revision = read_review(&state.review_path)
        .ok()
        .map(|review| review.revision().get());

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
        let review_due = review_watcher.reload_due();
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
                    revision = Some(review.revision().get());
                    publish(&state, ServerEventKind::ReviewInvalidated, revision);
                }
                Ok(_) => {
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
                    revision = Some(result.review().revision().get());
                    if result.changed() {
                        publish(&state, ServerEventKind::Refresh, revision);
                    }
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
            let mut overview = review_overview(&state.review_identity, &review);
            overview.watch = watch_status(&state);
            Json(overview).into_response()
        }
        Err(response) => response,
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
        Err(response) => response,
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
        Err(response) => response,
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
        Err(response) => response,
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
        Err(response) => response,
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/exports/context.json",
    responses((status = 200, description = "Bounded agent context JSON download"))
)]
async fn export_context(State(state): State<AppState>) -> Response<Body> {
    match load_current_review(&state).await {
        Ok(review) => match context_json(&review, ContextSelection::Manifest, Some(256 * 1024)) {
            Ok(bytes) => download(bytes, "application/json; charset=utf-8", "mire-context.json"),
            Err(error) => protocol_problem(error),
        },
        Err(response) => response,
    }
}

async fn load_current_review(state: &AppState) -> Result<Review, Response<Body>> {
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || read_review(&path)).await {
        Ok(Ok(review)) => Ok(review),
        Ok(Err(_)) | Err(_) => Err(problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read.",
        )),
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

fn review_overview(identity: &str, review: &Review) -> ReviewOverview {
    let findings = review.notes().iter().map(FindingSummary::from_note).collect::<Vec<_>>();
    let files = review
        .changeset()
        .files()
        .iter()
        .map(|file| FileSummary::from_file(file, review))
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
    fn from_file(file: &FileDiff, review: &Review) -> Self {
        let path = file_path(file);
        let open_findings = review
            .notes()
            .iter()
            .filter(|note| note.status() == NoteStatus::Open && finding_matches_file(note, file))
            .count();
        Self {
            id: file.fingerprint().to_string(),
            path: DisplayText::from_bytes(path),
            status: format!("{:?}", file.status()).to_lowercase(),
            content_kind: match file.content() {
                FileContent::Text { .. } => "text".to_owned(),
                FileContent::Binary => "binary".to_owned(),
            },
            open_findings,
        }
    }
}

impl FileDetail {
    fn from_file(file: &FileDiff, review: &Review) -> Self {
        let content = match file.content() {
            FileContent::Text { hunks } => {
                SemanticFileContent::Text { hunks: hunks.iter().map(SemanticHunk::from_hunk).collect() }
            }
            FileContent::Binary => SemanticFileContent::Binary,
        };
        Self {
            id: file.fingerprint().to_string(),
            path: DisplayText::from_bytes(file_path(file)),
            old_path: file
                .old_side()
                .map(|side| DisplayText::from_bytes(side.path.as_bytes())),
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
            path: DisplayText::from_bytes(anchor.path().as_bytes()),
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
            path: DisplayText::from_bytes(anchor.path().as_bytes()),
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
        }
    }
}

impl DisplayText {
    fn from_bytes(bytes: &[u8]) -> Self {
        let display = String::from_utf8_lossy(bytes);
        Self { lossy: matches!(display, std::borrow::Cow::Owned(_)), display: display.into_owned() }
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
mod tests {
    use super::*;
    use axum::http::Request;
    use mire_core::{
        AnchorSide, AnnotationKind, Author, BytePath, Changeset, ChangesetSource, Fingerprint, LineNumber, LineRange,
        NoteInput, NoteSeverity, PatchLimits, Provenance, ReviewRevision, parse_patch,
    };
    use tower::ServiceExt;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn state() -> AppState {
        let review_path = std::env::temp_dir().join(format!(
            "mire-serve-{}-{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let review = Review::new(
            ReviewRevision::new(1).expect("positive revision"),
            Changeset::new(ChangesetSource::DirectFiles, Vec::new(), Fingerprint::new([0; 32])),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid review");
        crate::write_review_atomic(&review_path, &review).expect("review is written");
        AppState {
            review_identity: "identity".to_owned(),
            review_path,
            session_secret: "secret".into(),
            origin: "http://127.0.0.1:3737".into(),
            request_ids: Arc::new(AtomicU64::new(1)),
            events: broadcast::channel(16).0,
            watch_status: Arc::new(Mutex::new(WatchStatus::Unavailable)),
        }
    }

    fn state_with_note() -> AppState {
        let state = state();
        let changeset = parse_patch(
            b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n",
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .expect("changeset");
        let review = Review::new(
            ReviewRevision::new(1).expect("positive revision"),
            changeset,
            Vec::new(),
            Vec::new(),
        )
        .expect("review");
        let input = NoteInput::new(
            BytePath::new(b"file.rs".to_vec()).expect("path"),
            AnchorSide::New,
            LineRange::new(LineNumber::new(1).expect("line"), LineNumber::new(1).expect("line")).expect("range"),
            Author::new("agent", None).expect("author"),
            Provenance::Agent { producer: "test".to_owned() },
            NoteSeverity::Low,
            AnnotationKind::Comment,
            "A finding".to_owned(),
        )
        .expect("note input");
        let review = review.apply_notes(vec![input]).expect("note applied");
        crate::write_review_atomic(&state.review_path, &review).expect("review is written");
        state
    }

    fn request(path: &str) -> Request<Body> {
        Request::builder()
            .uri(path)
            .header(HOST, "127.0.0.1:3737")
            .body(Body::empty())
            .expect("request")
    }

    #[tokio::test]
    async fn api_rejects_missing_session_secrets() {
        let response = app(state()).oneshot(request("/api/v1/review")).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_rejects_unexpected_hosts() {
        let response = app(state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/review")
                    .header(HOST, "example.com")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_returns_an_authenticated_overview_with_security_headers() {
        let mut request = request("/api/v1/review");
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state()).oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("referrer-policy"),
            Some(&HeaderValue::from_static("no-referrer"))
        );
    }

    #[tokio::test]
    async fn api_reports_missing_file_and_finding_resources() {
        let mut file_request = request("/api/v1/files/missing");
        file_request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state()).oneshot(file_request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let mut finding_request = request("/api/v1/findings/missing");
        finding_request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state()).oneshot(finding_request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn finding_mutations_edit_and_record_every_decision() {
        let state = state_with_note();
        let review = read_review(&state.review_path).expect("review");
        let note_id = review.notes()[0].id().as_str().to_owned();
        let edit = mutate_finding(
            &state.review_path,
            &note_id,
            review.revision().get(),
            FindingChange::Edit(EditFindingRequest {
                expected_revision: review.revision().get(),
                body: "Updated finding".to_owned(),
                severity: WebSeverity::High,
                annotation_kind: WebAnnotationKind::Defect,
            }),
        )
        .expect("edit succeeds");
        assert_eq!(edit.revision, 3);
        assert_eq!(edit.finding.body, "Updated finding");
        assert_eq!(edit.finding.severity, "high");
        assert_eq!(edit.finding.annotation_kind, "defect");

        let mut revision = edit.revision;
        for (decision, status) in [
            (FindingDecision::Resolve, "resolved"),
            (FindingDecision::Reopen, "open"),
            (FindingDecision::Dismiss, "dismissed"),
            (FindingDecision::AcceptRisk, "accepted-risk"),
        ] {
            let result = mutate_finding(
                &state.review_path,
                &note_id,
                revision,
                FindingChange::Decision(decision),
            )
            .expect("decision succeeds");
            revision = result.revision;
            assert_eq!(result.finding.status, status);
        }
        assert_eq!(revision, 7);
    }

    #[tokio::test]
    async fn finding_writes_require_the_revision_that_was_read() {
        let mut request = request("/api/v1/findings/missing");
        *request.method_mut() = Method::PATCH;
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:3737"));
        request
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        *request.body_mut() = Body::from(
            r#"{"expectedRevision":2,"body":"Updated finding","severity":"high","annotationKind":"defect"}"#,
        );

        let response = app(state()).oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(response.into_body(), MAX_REQUEST_BODY_BYTES)
            .await
            .expect("response body");
        let problem: Problem = serde_json::from_slice(&bytes).expect("problem response");
        assert_eq!(problem.code, "revision_conflict");
        assert_eq!(problem.actual_revision, Some(1));
    }

    #[tokio::test]
    async fn api_rejects_cross_origin_writes() {
        let mut request = request("/api/v1/review");
        *request.method_mut() = Method::POST;
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state()).oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn refresh_requires_a_source_binding_and_exports_are_downloads() {
        let mut refresh = request("/api/v1/refresh");
        *refresh.method_mut() = Method::POST;
        refresh
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        refresh
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:3737"));
        let response = app(state()).oneshot(refresh).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let mut export = request("/api/v1/exports/notes.json");
        export
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state()).oneshot(export).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_DISPOSITION),
            Some(&HeaderValue::from_static("attachment; filename=\"mire-notes.json\""))
        );
    }

    #[tokio::test]
    async fn static_application_is_public_but_host_checked() {
        let response = app(state()).oneshot(request("/")).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-cache"))
        );
        assert!(
            response
                .headers()
                .get(CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|policy| policy.contains("script-src 'self' 'sha256-"))
        );
    }
}
