//! Tests for the loopback review server: request security, every API resource,
//! revision conflicts, and the event stream.

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

async fn overview_json(state: AppState) -> serde_json::Value {
    let mut request = request("/api/v1/review");
    request
        .headers_mut()
        .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
    let response = app(state).oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), MAX_REQUEST_BODY_BYTES)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("overview json")
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
async fn api_overview_reports_the_review_title_and_description() {
    let state = state();
    let overview = overview_json(state.clone()).await;
    assert_eq!(overview["title"], serde_json::Value::Null);
    assert_eq!(overview["description"], serde_json::Value::Null);

    let review = read_review(&state.review_path)
        .expect("review")
        .with_metadata(
            Some("Add caching".to_owned()),
            Some("First line\n\n<b>not html</b>".to_owned()),
        )
        .expect("valid metadata");
    crate::write_review_atomic(&state.review_path, &review).expect("review is written");
    let overview = overview_json(state).await;
    assert_eq!(overview["title"], "Add caching");
    assert_eq!(overview["description"], "First line\n\n<b>not html</b>");
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
    *request.body_mut() =
        Body::from(r#"{"expectedRevision":2,"body":"Updated finding","severity":"high","annotationKind":"defect"}"#);

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
async fn context_export_fits_a_maximum_worst_case_description() {
    let chars = mire_core::MAX_REVIEW_DESCRIPTION_CHARS;
    let title = "\"".repeat(mire_core::MAX_REVIEW_TITLE_BYTES);
    for description in [
        "\u{1F600}".repeat(chars),
        "\"".repeat(chars),
        "\\\n\t".repeat(chars / 3),
    ] {
        let state = state();
        let review = read_review(&state.review_path)
            .expect("review")
            .with_metadata(Some(title.clone()), Some(description.clone()))
            .expect("valid metadata");
        crate::write_review_atomic(&state.review_path, &review).expect("review is written");
        let mut export = request("/api/v1/exports/context.json");
        export
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        let response = app(state).oneshot(export).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), MAX_CONTEXT_EXPORT_BYTES)
            .await
            .expect("body");
        let context: serde_json::Value = serde_json::from_slice(&bytes).expect("context JSON");
        assert_eq!(context["payload"]["description"], description.as_str());
    }
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

const ORIGIN_VALUE: &str = "http://127.0.0.1:3737";
const API_GET_ROUTES: [&str; 7] = [
    "/api/v1/review",
    "/api/v1/files/missing",
    "/api/v1/findings/missing",
    "/api/v1/events",
    "/api/v1/exports/notes.json",
    "/api/v1/exports/notes.md",
    "/api/v1/exports/context.json",
];

/// Builds an authenticated API request with the page's own origin.
fn api_request(method: Method, path: &str, body: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(HOST, "127.0.0.1:3737")
        .header(AUTHORIZATION, "Bearer secret")
        .header(ORIGIN, ORIGIN_VALUE);
    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_owned())))
        .expect("request")
}

async fn send(state: AppState, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app(state).oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), MAX_CONTEXT_EXPORT_BYTES)
        .await
        .expect("body");
    (status, headers, bytes.to_vec())
}

async fn send_json(state: AppState, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let (status, _, bytes) = send(state, request).await;
    (status, serde_json::from_slice(&bytes).expect("JSON body"))
}

fn note_id(state: &AppState) -> String {
    read_review(&state.review_path).expect("review").notes()[0]
        .id()
        .as_str()
        .to_owned()
}

#[tokio::test]
async fn every_api_route_requires_the_exact_bearer_secret() {
    let routes = API_GET_ROUTES.iter().map(|path| (Method::GET, *path)).chain([
        (Method::GET, "/api/v1/openapi.json"),
        (Method::PATCH, "/api/v1/findings/missing"),
        (Method::POST, "/api/v1/findings/missing/decision"),
        (Method::POST, "/api/v1/refresh"),
    ]);
    for (method, path) in routes {
        for authorization in [
            None,
            Some("Bearer wrong"),
            Some("bearer secret"),
            Some("Basic secret"),
            Some("Bearer  secret"),
            Some("Bearer secret "),
        ] {
            let mut builder = Request::builder()
                .method(method.clone())
                .uri(path)
                .header(HOST, "127.0.0.1:3737")
                .header(ORIGIN, ORIGIN_VALUE);
            if let Some(value) = authorization {
                builder = builder.header(AUTHORIZATION, value);
            }
            let (status, problem) = send_json(state(), builder.body(Body::empty()).expect("request")).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} with {authorization:?}"
            );
            assert_eq!(problem["code"], "unauthorized");
        }
    }
}

#[tokio::test]
async fn unexpected_hosts_are_rejected_before_authentication() {
    for host in [
        None,
        Some("localhost:3737"),
        Some("127.0.0.1:3738"),
        Some("127.0.0.1"),
        Some("evil.example"),
    ] {
        for path in ["/api/v1/review", "/"] {
            let mut builder = Request::builder().uri(path).header(AUTHORIZATION, "Bearer secret");
            if let Some(host) = host {
                builder = builder.header(HOST, host);
            }
            let (status, problem) = send_json(state(), builder.body(Body::empty()).expect("request")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path} with host {host:?}");
            assert_eq!(problem["code"], "unexpected_host");
        }
    }
}

#[tokio::test]
async fn writes_require_the_page_origin_and_reads_do_not() {
    let state = state_with_note();
    let id = note_id(&state);
    let decision = format!("/api/v1/findings/{id}/decision");
    for origin in [
        None,
        Some("null"),
        Some("http://localhost:3737"),
        Some("http://127.0.0.1:3737/"),
        Some("https://127.0.0.1:3737"),
    ] {
        let mut request = api_request(
            Method::POST,
            &decision,
            Some(r#"{"expectedRevision":2,"decision":"resolve"}"#),
        );
        request.headers_mut().remove(ORIGIN);
        if let Some(origin) = origin {
            request.headers_mut().insert(ORIGIN, HeaderValue::from_static(origin));
        }
        let (status, problem) = send_json(state.clone(), request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "origin {origin:?}");
        assert_eq!(problem["code"], "unexpected_origin");
    }
    assert_eq!(read_review(&state.review_path).expect("review").revision().get(), 2);

    let mut read = api_request(Method::GET, "/api/v1/review", None);
    read.headers_mut().remove(ORIGIN);
    let (status, _) = send_json(state, read).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn request_bodies_are_bounded_and_strictly_typed() {
    let state = state_with_note();
    let id = note_id(&state);
    let edit = format!("/api/v1/findings/{id}");
    let oversized = format!(
        r#"{{"expectedRevision":2,"body":"{}","severity":"low","annotationKind":"comment"}}"#,
        "x".repeat(MAX_REQUEST_BODY_BYTES)
    );
    let (status, _, _) = send(state.clone(), api_request(Method::PATCH, &edit, Some(&oversized))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    for body in [
        "not json",
        r#"{"expectedRevision":2,"body":"x","severity":"urgent","annotationKind":"comment"}"#,
        r#"{"expectedRevision":2,"body":"x","severity":"low","annotationKind":"comment","path":"/etc/passwd"}"#,
        r#"{"body":"x","severity":"low","annotationKind":"comment"}"#,
    ] {
        let (status, _, _) = send(state.clone(), api_request(Method::PATCH, &edit, Some(body))).await;
        assert!(status.is_client_error(), "{body} returned {status}");
    }

    let mut untyped = api_request(
        Method::PATCH,
        &edit,
        Some(r#"{"expectedRevision":2,"body":"x","severity":"low","annotationKind":"comment"}"#),
    );
    untyped.headers_mut().remove(CONTENT_TYPE);
    let (status, _, _) = send(state.clone(), untyped).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(read_review(&state.review_path).expect("review").revision().get(), 2);
}

#[tokio::test]
async fn overview_file_and_finding_resources_describe_the_review() {
    let state = state_with_note();
    let (status, overview) = send_json(state.clone(), api_request(Method::GET, "/api/v1/review", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["reviewIdentity"], "identity");
    assert_eq!(overview["revision"], 2);
    assert_eq!(overview["watch"], "unavailable");
    assert_eq!(overview["totals"]["files"], 1);
    assert_eq!(overview["totals"]["open"], 1);
    assert_eq!(overview["changes"]["additions"], 1);
    assert_eq!(overview["changes"]["deletions"], 1);
    assert_eq!(overview["readiness"]["ready"], false);
    assert_eq!(overview["files"][0]["path"]["display"], "file.rs");
    assert_eq!(overview["files"][0]["openFindings"], 1);
    assert_eq!(overview["findings"][0]["body"], "A finding");

    let file_id = overview["files"][0]["id"].as_str().expect("file id").to_owned();
    let (status, file) = send_json(
        state.clone(),
        api_request(Method::GET, &format!("/api/v1/files/{file_id}"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(file["id"], file_id.as_str());
    assert_eq!(file["content"]["kind"], "text");
    let lines = &file["content"]["hunks"][0]["lines"];
    assert_eq!(lines[0]["kind"], "deletion");
    assert_eq!(lines[0]["content"]["display"], "let old = 1;");
    assert_eq!(lines[1]["kind"], "addition");
    assert_eq!(lines[1]["newLine"], 1);
    assert_eq!(file["findings"][0]["anchorState"], "captured");

    let id = note_id(&state);
    let (status, finding) = send_json(state, api_request(Method::GET, &format!("/api/v1/findings/{id}"), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finding["id"], id.as_str());
    assert_eq!(finding["author"]["id"], "agent");
    assert_eq!(finding["severity"], "low");
    assert_eq!(finding["annotationKind"], "comment");
    assert_eq!(finding["status"], "open");
}

#[tokio::test]
async fn finding_edits_and_decisions_over_http_persist_and_advance_the_revision() {
    let state = state_with_note();
    let id = note_id(&state);
    let (status, mutation) = send_json(
        state.clone(),
        api_request(
            Method::PATCH,
            &format!("/api/v1/findings/{id}"),
            Some(r#"{"expectedRevision":2,"body":"Edited in the browser","severity":"high","annotationKind":"question"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mutation["revision"], 3);
    assert_eq!(mutation["finding"]["body"], "Edited in the browser");

    let (status, mutation) = send_json(
        state.clone(),
        api_request(
            Method::POST,
            &format!("/api/v1/findings/{id}/decision"),
            Some(r#"{"expectedRevision":3,"decision":"accept-risk"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mutation["revision"], 4);
    assert_eq!(mutation["finding"]["status"], "accepted-risk");

    let review = read_review(&state.review_path).expect("review");
    assert_eq!(review.revision().get(), 4);
    assert_eq!(review.notes()[0].body(), "Edited in the browser");
    assert_eq!(review.notes()[0].status(), NoteStatus::AcceptedRisk);

    let (status, problem) = send_json(
        state,
        api_request(
            Method::POST,
            "/api/v1/findings/missing/decision",
            Some(r#"{"expectedRevision":4,"decision":"resolve"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "finding_not_found");
}

#[tokio::test]
async fn stale_revisions_conflict_without_changing_the_review_file() {
    let state = state_with_note();
    let id = note_id(&state);
    let before = std::fs::read(&state.review_path).expect("review bytes");
    for (method, path, body) in [
        (
            Method::PATCH,
            format!("/api/v1/findings/{id}"),
            r#"{"expectedRevision":1,"body":"Lost update","severity":"high","annotationKind":"defect"}"#,
        ),
        (
            Method::POST,
            format!("/api/v1/findings/{id}/decision"),
            r#"{"expectedRevision":3,"decision":"dismiss"}"#,
        ),
    ] {
        let (status, problem) = send_json(state.clone(), api_request(method, &path, Some(body))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}");
        assert_eq!(problem["code"], "revision_conflict");
        assert_eq!(problem["actualRevision"], 2);
    }
    assert_eq!(std::fs::read(&state.review_path).expect("review bytes"), before);
}

#[tokio::test]
async fn a_locked_review_reports_locked_instead_of_writing() {
    let state = state_with_note();
    let id = note_id(&state);
    let file_name = state
        .review_path
        .file_name()
        .expect("file name")
        .to_string_lossy()
        .into_owned();
    let lock = state.review_path.with_file_name(format!(".{file_name}.mire-lock"));
    std::fs::write(&lock, b"").expect("lock file");
    let (status, problem) = send_json(
        state.clone(),
        api_request(
            Method::POST,
            &format!("/api/v1/findings/{id}/decision"),
            Some(r#"{"expectedRevision":2,"decision":"resolve"}"#),
        ),
    )
    .await;
    std::fs::remove_file(&lock).expect("lock removed");
    assert_eq!(status, StatusCode::LOCKED);
    assert_eq!(problem["code"], "review_locked");
    assert_eq!(read_review(&state.review_path).expect("review").revision().get(), 2);
}

#[tokio::test]
async fn exports_and_the_openapi_document_are_typed_downloads() {
    let state = state_with_note();
    let id = note_id(&state);
    for (path, content_type, file_name) in [
        (
            "/api/v1/exports/notes.json",
            "application/json; charset=utf-8",
            "mire-notes.json",
        ),
        (
            "/api/v1/exports/notes.md",
            "text/markdown; charset=utf-8",
            "mire-notes.md",
        ),
        (
            "/api/v1/exports/context.json",
            "application/json; charset=utf-8",
            "mire-context.json",
        ),
    ] {
        let (status, headers, bytes) = send(state.clone(), api_request(Method::GET, path, None)).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(
            headers.get(CONTENT_TYPE).and_then(|value| value.to_str().ok()),
            Some(content_type)
        );
        assert_eq!(
            headers.get(CONTENT_DISPOSITION).and_then(|value| value.to_str().ok()),
            Some(format!("attachment; filename=\"{file_name}\"").as_str())
        );
        assert!(
            String::from_utf8_lossy(&bytes).contains(&id),
            "{path} carries the finding"
        );
    }

    let (status, document) = send_json(state, api_request(Method::GET, "/api/v1/openapi.json", None)).await;
    assert_eq!(status, StatusCode::OK);
    let paths = document["paths"].as_object().expect("paths");
    for path in [
        "/api/v1/review",
        "/api/v1/files/{file_id}",
        "/api/v1/findings/{note_id}",
        "/api/v1/findings/{note_id}/decision",
        "/api/v1/refresh",
        "/api/v1/events",
        "/api/v1/exports/notes.json",
        "/api/v1/exports/notes.md",
        "/api/v1/exports/context.json",
    ] {
        assert!(paths.contains_key(path), "the OpenAPI document lists {path}");
    }
}

#[tokio::test]
async fn unknown_api_paths_are_not_found_rather_than_the_application() {
    let (status, problem) = send_json(state(), api_request(Method::GET, "/api/v1/unknown", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "not_found");
}

#[tokio::test]
async fn the_event_stream_delivers_invalidations_after_a_write() {
    let state = state_with_note();
    let id = note_id(&state);
    let response = app(state.clone())
        .oneshot(api_request(Method::GET, "/api/v1/events", None))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE),
        Some(&HeaderValue::from_static("text/event-stream"))
    );
    let mut stream = response.into_body().into_data_stream();

    let (status, _) = send_json(
        state,
        api_request(
            Method::POST,
            &format!("/api/v1/findings/{id}/decision"),
            Some(r#"{"expectedRevision":2,"decision":"resolve"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("an event arrives")
        .expect("the stream stays open")
        .expect("frame bytes");
    let frame = String::from_utf8(frame.to_vec()).expect("UTF-8 event");
    assert!(frame.starts_with("event: mire\n"), "{frame}");
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("event data");
    let event: serde_json::Value = serde_json::from_str(data).expect("event JSON");
    assert_eq!(event["kind"], "review_invalidated");
    assert_eq!(event["revision"], 3);
    assert_eq!(event["watch"], "unavailable");
}
