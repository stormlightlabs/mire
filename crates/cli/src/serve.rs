//! Loopback HTTP server for the browser review surface.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, Method, Response, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::{Router, extract::MatchedPath};
use base64::Engine;
use include_dir::{Dir, include_dir};
use mire_core::{ChangesetSource, FileDiff, NoteStatus, Review};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::{OpenApi, ToSchema};

use crate::cli::ServeArgs;
use crate::review_file::{ReviewFileError, read_review};

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
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FileSummary {
    id: String,
    path: DisplayPath,
    status: String,
    open_findings: usize,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FindingSummary {
    id: String,
    path: DisplayPath,
    severity: String,
    status: String,
    body: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DisplayPath {
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
struct Problem {
    code: String,
    detail: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(get_review, get_openapi),
    components(schemas(ReviewOverview, FileSummary, FindingSummary, DisplayPath, ReviewTotals, Problem))
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
    let state = AppState {
        review_identity: review_identity(&review_path).map_err(ServeError::Bind)?,
        review_path,
        session_secret: secret.into(),
        origin,
        request_ids: Arc::new(AtomicU64::new(1)),
    };

    println!("Mire review server: {session_url}");
    if arguments.open {
        open::that_detached(&session_url).map_err(ServeError::OpenBrowser)?;
    }

    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(ServeError::Run)
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/review", get(get_review))
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
    let path = state.review_path.clone();
    match tokio::task::spawn_blocking(move || read_review(&path)).await {
        Ok(Ok(review)) => Json(review_overview(&state.review_identity, &review)).into_response(),
        Ok(Err(_)) | Err(_) => problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "review_unavailable",
            "The review file could not be read.",
        ),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/openapi.json",
    responses(
        (status = 200, description = "OpenAPI document")
    )
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

    ReviewOverview {
        review_identity: identity.to_owned(),
        revision: review.revision().get(),
        source: source_summary(review.changeset().source()),
        files,
        findings,
        totals,
    }
}

impl FileSummary {
    fn from_file(file: &FileDiff, review: &Review) -> Self {
        let path = file
            .new_side()
            .or(file.old_side())
            .expect("validated file diff has a side")
            .path
            .as_bytes();
        let open_findings = review
            .notes()
            .iter()
            .filter(|note| note.status() == NoteStatus::Open && note.anchor().path().as_bytes() == path)
            .count();
        Self {
            id: file.fingerprint().to_string(),
            path: DisplayPath::from_bytes(path),
            status: format!("{:?}", file.status()).to_lowercase(),
            open_findings,
        }
    }
}

impl FindingSummary {
    fn from_note(note: &mire_core::ReviewNote) -> Self {
        Self {
            id: note.id().as_str().to_owned(),
            path: DisplayPath::from_bytes(note.anchor().path().as_bytes()),
            severity: note.severity().to_string().to_lowercase(),
            status: note.status().to_string(),
            body: note.body().to_owned(),
        }
    }
}

impl DisplayPath {
    fn from_bytes(bytes: &[u8]) -> Self {
        let display = String::from_utf8_lossy(bytes);
        Self { lossy: matches!(display, std::borrow::Cow::Owned(_)), display: display.into_owned() }
    }
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

fn problem(status: StatusCode, code: &str, detail: &str) -> Response<Body> {
    (
        status,
        Json(Problem { code: code.to_owned(), detail: detail.to_owned() }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use mire_core::{Changeset, ChangesetSource, Fingerprint, ReviewRevision};
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
        }
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
