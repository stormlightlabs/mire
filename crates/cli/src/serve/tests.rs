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

/// A review file in its own temporary directory, with the server state that
/// serves it. Dropping the fixture removes the directory, so the review, its
/// lock file and any leftover temporary write go with it.
struct Fixture {
    directory: PathBuf,
    state: AppState,
}

impl Fixture {
    /// An empty review at revision 1.
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "mire-serve-{}-{}",
            std::process::id(),
            TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).expect("temporary directory");
        let review_path = directory.join("review.json");
        let review = Review::new(
            ReviewRevision::new(1).expect("positive revision"),
            Changeset::new(ChangesetSource::DirectFiles, Vec::new(), Fingerprint::new([0; 32])),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid review");
        crate::write_review_atomic(&review_path, &review).expect("review is written");
        let state = AppState {
            review_identity: "identity".to_owned(),
            review_path,
            session_secret: "secret".into(),
            origin: ORIGIN_VALUE.into(),
            request_ids: Arc::new(AtomicU64::new(1)),
            events: broadcast::channel(16).0,
            watch_status: Arc::new(Mutex::new(WatchStatus::Unavailable)),
            announced_viewed: Arc::new(Mutex::new(ViewedMap::new())),
            held_blobs: Arc::default(),
        };
        Self { directory, state }
    }

    /// A one-file review at revision 2 with one open finding on `file.rs`.
    fn with_note() -> Self {
        let fixture = Self::new();
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
        crate::write_review_atomic(fixture.path(), &review).expect("review is written");
        fixture
    }

    /// A clone of the server state, for one `app` call.
    fn state(&self) -> AppState {
        self.state.clone()
    }

    fn path(&self) -> &Path {
        &self.state.review_path
    }

    /// The identifier of the first finding in the review.
    fn note_id(&self) -> String {
        read_review(self.path()).expect("review").notes()[0]
            .id()
            .as_str()
            .to_owned()
    }

    fn revision(&self) -> u64 {
        read_review(self.path()).expect("review").revision().get()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

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
#[tokio::test]
async fn every_api_route_requires_the_exact_bearer_secret() {
    let routes = API_GET_ROUTES.iter().map(|path| (Method::GET, *path)).chain([
        (Method::GET, "/api/v1/openapi.json"),
        (Method::PATCH, "/api/v1/findings/missing"),
        (Method::POST, "/api/v1/findings/missing/decision"),
        (Method::POST, "/api/v1/refresh"),
        (Method::PUT, "/api/v1/viewed?path=66696c652e7273"),
        (Method::DELETE, "/api/v1/viewed?path=66696c652e7273"),
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
            let (status, problem) =
                send_json(Fixture::new().state(), builder.body(Body::empty()).expect("request")).await;
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
            // Without a valid secret, an API request that passed the host
            // check would fail authentication with 401 instead.
            for authorization in [None, Some("Bearer wrong")] {
                let mut builder = Request::builder().uri(path);
                if let Some(host) = host {
                    builder = builder.header(HOST, host);
                }
                if let Some(value) = authorization {
                    builder = builder.header(AUTHORIZATION, value);
                }
                let (status, problem) =
                    send_json(Fixture::new().state(), builder.body(Body::empty()).expect("request")).await;
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "{path} with host {host:?} and {authorization:?}"
                );
                assert_eq!(problem["code"], "unexpected_host");
            }
        }
    }
}

#[tokio::test]
async fn writes_require_the_page_origin_and_reads_do_not() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
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
        let (status, problem) = send_json(fixture.state(), request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "origin {origin:?}");
        assert_eq!(problem["code"], "unexpected_origin");
    }
    assert_eq!(fixture.revision(), 2);

    let mut read = api_request(Method::GET, "/api/v1/review", None);
    read.headers_mut().remove(ORIGIN);
    let (status, _) = send_json(fixture.state(), read).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn request_bodies_are_bounded_and_strictly_typed() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
    let edit = format!("/api/v1/findings/{id}");
    let oversized = format!(
        r#"{{"expectedRevision":2,"body":"{}","severity":"low","annotationKind":"comment"}}"#,
        "x".repeat(MAX_REQUEST_BODY_BYTES)
    );
    let (status, _, _) = send(fixture.state(), api_request(Method::PATCH, &edit, Some(&oversized))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    for body in [
        "not json",
        r#"{"expectedRevision":2,"body":"x","severity":"urgent","annotationKind":"comment"}"#,
        r#"{"expectedRevision":2,"body":"x","severity":"low","annotationKind":"comment","path":"/etc/passwd"}"#,
        r#"{"body":"x","severity":"low","annotationKind":"comment"}"#,
    ] {
        let (status, _, _) = send(fixture.state(), api_request(Method::PATCH, &edit, Some(body))).await;
        assert!(status.is_client_error(), "{body} returned {status}");
    }

    let mut untyped = api_request(
        Method::PATCH,
        &edit,
        Some(r#"{"expectedRevision":2,"body":"x","severity":"low","annotationKind":"comment"}"#),
    );
    untyped.headers_mut().remove(CONTENT_TYPE);
    let (status, _, _) = send(fixture.state(), untyped).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(fixture.revision(), 2);
}

#[tokio::test]
async fn overview_file_and_finding_resources_describe_the_review() {
    let fixture = Fixture::with_note();
    let (status, headers, bytes) = send(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    assert_eq!(status, StatusCode::OK);
    for (name, value) in [
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
    ] {
        assert_eq!(
            headers.get(name).and_then(|value| value.to_str().ok()),
            Some(value),
            "{name}"
        );
    }
    let overview: serde_json::Value = serde_json::from_slice(&bytes).expect("overview JSON");
    assert_eq!(overview["reviewIdentity"], "identity");
    assert_eq!(overview["revision"], 2);
    assert_eq!(overview["watch"], "unavailable");
    assert_eq!(overview["totals"]["files"], 1);
    assert_eq!(overview["totals"]["open"], 1);
    assert_eq!(overview["changes"]["additions"], 1);
    assert_eq!(overview["changes"]["deletions"], 1);
    assert_eq!(overview["readiness"]["ready"], false);
    assert_eq!(overview["files"][0]["path"]["display"], "file.rs");
    assert_eq!(overview["files"][0]["path"]["raw"], "66696c652e7273");
    assert_eq!(overview["files"][0]["viewed"], serde_json::Value::Null);
    assert_eq!(overview["files"][0]["openFindings"], 1);
    assert_eq!(overview["findings"][0]["body"], "A finding");

    let file_id = overview["files"][0]["id"].as_str().expect("file id").to_owned();
    let (status, file) = send_json(
        fixture.state(),
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

    let id = fixture.note_id();
    let (status, finding) = send_json(
        fixture.state(),
        api_request(Method::GET, &format!("/api/v1/findings/{id}"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finding["id"], id.as_str());
    assert_eq!(finding["author"]["id"], "agent");
    assert_eq!(finding["severity"], "low");
    assert_eq!(finding["annotationKind"], "comment");
    assert_eq!(finding["status"], "open");
}

#[tokio::test]
async fn finding_edits_and_decisions_over_http_persist_and_advance_the_revision() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
    let (status, mutation) = send_json(
        fixture.state(),
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
    assert_eq!(mutation["finding"]["severity"], "high");
    assert_eq!(mutation["finding"]["annotationKind"], "question");

    let mut revision = 3;
    for (decision, status_name, note_status) in [
        ("resolve", "resolved", NoteStatus::Resolved),
        ("reopen", "open", NoteStatus::Open),
        ("dismiss", "dismissed", NoteStatus::Dismissed),
        ("accept-risk", "accepted-risk", NoteStatus::AcceptedRisk),
    ] {
        let (status, mutation) = send_json(
            fixture.state(),
            api_request(
                Method::POST,
                &format!("/api/v1/findings/{id}/decision"),
                Some(&format!(r#"{{"expectedRevision":{revision},"decision":"{decision}"}}"#)),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{decision}");
        revision += 1;
        assert_eq!(mutation["revision"], revision);
        assert_eq!(mutation["finding"]["status"], status_name);

        let review = read_review(fixture.path()).expect("review");
        assert_eq!(review.revision().get(), revision);
        assert_eq!(review.notes()[0].body(), "Edited in the browser");
        assert_eq!(review.notes()[0].status(), note_status);
    }
    assert_eq!(revision, 7);

    let (status, problem) = send_json(
        fixture.state(),
        api_request(
            Method::POST,
            "/api/v1/findings/missing/decision",
            Some(r#"{"expectedRevision":7,"decision":"resolve"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "finding_not_found");
}

#[tokio::test]
async fn stale_revisions_conflict_without_changing_the_review_file() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
    let before = std::fs::read(fixture.path()).expect("review bytes");
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
        (
            Method::PATCH,
            "/api/v1/findings/missing".to_owned(),
            r#"{"expectedRevision":1,"body":"Lost update","severity":"high","annotationKind":"defect"}"#,
        ),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(method, &path, Some(body))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}");
        assert_eq!(problem["code"], "revision_conflict");
        assert_eq!(problem["actualRevision"], 2);
    }
    assert_eq!(std::fs::read(fixture.path()).expect("review bytes"), before);
}

#[tokio::test]
async fn a_locked_review_reports_locked_instead_of_writing() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
    let file_name = fixture
        .path()
        .file_name()
        .expect("file name")
        .to_string_lossy()
        .into_owned();
    let lock = fixture.path().with_file_name(format!(".{file_name}.mire-lock"));
    std::fs::write(&lock, b"").expect("lock file");
    let (status, problem) = send_json(
        fixture.state(),
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
    assert_eq!(fixture.revision(), 2);
}

#[tokio::test]
async fn exports_and_the_openapi_document_are_typed_downloads() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
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
        let (status, headers, bytes) = send(fixture.state(), api_request(Method::GET, path, None)).await;
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

    let (status, document) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/openapi.json", None)).await;
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
    let history = &paths["/api/v1/history"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
    assert_eq!(history["$ref"], "#/components/schemas/HistoryResult");
    let variants = document["components"]["schemas"]["HistoryResult"]["oneOf"]
        .as_array()
        .expect("the history body is one of two shapes")
        .iter()
        .map(|variant| variant["$ref"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        variants,
        [
            "#/components/schemas/HistoryResponse",
            "#/components/schemas/CheckpointResponse"
        ]
    );
}

#[tokio::test]
async fn unknown_api_paths_are_not_found_rather_than_the_application() {
    let (status, problem) = send_json(
        Fixture::new().state(),
        api_request(Method::GET, "/api/v1/unknown", None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "not_found");
}

#[tokio::test]
async fn missing_files_and_findings_are_not_found() {
    let fixture = Fixture::with_note();
    for (path, code) in [
        ("/api/v1/files/missing", "file_not_found"),
        ("/api/v1/findings/missing", "finding_not_found"),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::GET, path, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(problem["code"], code, "{path}");
    }
}

#[tokio::test]
async fn the_overview_reports_the_review_title_and_description() {
    let fixture = Fixture::new();
    let (status, overview) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["title"], serde_json::Value::Null);
    assert_eq!(overview["description"], serde_json::Value::Null);

    let review = read_review(fixture.path())
        .expect("review")
        .with_metadata(
            Some("Add caching".to_owned()),
            Some("First line\n\n<b>not html</b>".to_owned()),
        )
        .expect("valid metadata");
    crate::write_review_atomic(fixture.path(), &review).expect("review is written");
    let (status, overview) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["title"], "Add caching");
    assert_eq!(overview["description"], "First line\n\n<b>not html</b>");
}

#[tokio::test]
async fn refresh_without_a_source_binding_is_unprocessable() {
    let fixture = Fixture::with_note();
    let (status, problem) = send_json(fixture.state(), api_request(Method::POST, "/api/v1/refresh", None)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["code"], "source_unavailable");
    assert_eq!(fixture.revision(), 2);
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
        let fixture = Fixture::new();
        let review = read_review(fixture.path())
            .expect("review")
            .with_metadata(Some(title.clone()), Some(description.clone()))
            .expect("valid metadata");
        crate::write_review_atomic(fixture.path(), &review).expect("review is written");
        let (status, context) = send_json(
            fixture.state(),
            api_request(Method::GET, "/api/v1/exports/context.json", None),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(context["payload"]["description"], description.as_str());
    }
}

#[tokio::test]
async fn the_static_application_is_public_with_a_script_hash_policy() {
    let fixture = Fixture::new();
    let mut request = api_request(Method::GET, "/", None);
    request.headers_mut().remove(AUTHORIZATION);
    request.headers_mut().remove(ORIGIN);
    let (status, headers, _) = send(fixture.state(), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(CACHE_CONTROL), Some(&HeaderValue::from_static("no-cache")));
    assert!(
        headers
            .get(CONTENT_SECURITY_POLICY)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|policy| policy.contains("script-src 'self' 'sha256-"))
    );
}

#[tokio::test]
async fn the_event_stream_delivers_invalidations_after_a_write() {
    let fixture = Fixture::with_note();
    let id = fixture.note_id();
    let response = app(fixture.state())
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
        fixture.state(),
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

#[test]
fn the_watcher_announces_every_new_revision_it_learns() {
    let fixture = Fixture::new();
    let state = fixture.state();
    let mut events = state.events.subscribe();
    let mut announced = Some(2);

    // A refresh that found nothing to do can still return a revision that
    // another process wrote since the last announcement.
    announce_revision(&state, &mut announced, 3, false);
    let event = events.try_recv().expect("an event for the unannounced write");
    assert_eq!(
        (event.kind, event.revision),
        (ServerEventKind::ReviewInvalidated, Some(3))
    );
    assert_eq!(announced, Some(3));

    announce_revision(&state, &mut announced, 3, false);
    assert!(events.try_recv().is_err(), "a known revision is not announced again");

    announce_revision(&state, &mut announced, 4, true);
    let event = events.try_recv().expect("an event for the refresh");
    assert_eq!((event.kind, event.revision), (ServerEventKind::Refresh, Some(4)));
    assert_eq!(announced, Some(4));
}

#[tokio::test]
async fn the_watcher_announces_a_write_made_before_it_started() {
    let fixture = Fixture::with_note();
    let state = fixture.state();
    let mut events = state.events.subscribe();
    let (shutdown, shutdown_receiver) = watch::channel(false);

    // The review is at revision 2, but the server read revision 1 before it
    // printed its URL, so the watcher must report the write it never saw.
    let watcher = start_watcher(state, 1, shutdown_receiver);
    let received = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(event) if event.kind == ServerEventKind::WatchStatus => {}
                other => break other,
            }
        }
    })
    .await;
    // Stop the watcher before any assertion, so a failure cannot leave its
    // blocking loop holding the runtime open.
    shutdown.send(true).expect("the watcher is listening");
    watcher.await.expect("the watcher stops");
    let event = received.expect("an event arrives").expect("the channel stays open");
    assert_eq!(
        (event.kind, event.revision),
        (ServerEventKind::ReviewInvalidated, Some(2))
    );
}

/// The file id and path hex of the review's only file.
async fn first_file(fixture: &Fixture) -> (String, String) {
    let (_, overview) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    let file = &overview["files"][0];
    (
        file["id"].as_str().expect("file id").to_owned(),
        file["path"]["raw"].as_str().expect("raw path").to_owned(),
    )
}

#[tokio::test]
async fn viewed_marks_persist_without_advancing_the_revision() {
    let fixture = Fixture::with_note();
    let (id, raw) = first_file(&fixture).await;
    let route = format!("/api/v1/viewed?path={raw}");
    let body = format!(r#"{{"fingerprint":"{id}","revision":2}}"#);
    let (status, marked) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
    assert_eq!(status, StatusCode::OK, "{marked}");
    assert_eq!(marked["revision"], 2);
    assert_eq!(marked["path"]["display"], "file.rs");
    assert_eq!(marked["viewed"]["fingerprint"], id.as_str());
    assert_eq!(marked["viewed"]["revision"], 2);
    assert_eq!(fixture.revision(), 2);
    let review = read_review(fixture.path()).expect("review");
    assert!(review.is_viewed(local_author().id(), &review.changeset().files()[0]));

    let (_, overview) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    assert_eq!(overview["files"][0]["viewed"]["fingerprint"], id.as_str());

    let (status, cleared) = send_json(fixture.state(), api_request(Method::DELETE, &route, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["viewed"], serde_json::Value::Null);
    assert_eq!(fixture.revision(), 2);
    assert!(read_review(fixture.path()).expect("review").viewed().is_empty());
}

#[tokio::test]
async fn the_delete_route_clears_a_mark_on_a_path_that_left_the_changeset() {
    let fixture = Fixture::with_note();
    let (id, raw) = first_file(&fixture).await;
    let route = format!("/api/v1/viewed?path={raw}");
    let body = format!(r#"{{"fingerprint":"{id}","revision":2}}"#);
    let (status, _) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
    assert_eq!(status, StatusCode::OK);
    let review = read_review(fixture.path()).expect("review");
    let other = parse_patch(
        b"--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-a\n+b\n",
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("changeset");
    let refreshed = review.reanchor(other).expect("reanchored");
    crate::write_review_atomic(fixture.path(), &refreshed).expect("review is written");
    let path = BytePath::new(b"file.rs".to_vec()).expect("path");
    assert!(
        read_review(fixture.path())
            .expect("review")
            .viewed_mark(local_author().id(), &path)
            .is_some()
    );

    let (status, cleared) = send_json(fixture.state(), api_request(Method::DELETE, &route, None)).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["viewed"], serde_json::Value::Null);
    assert!(read_review(fixture.path()).expect("review").viewed().is_empty());
}

#[tokio::test]
async fn viewed_marks_reject_bad_paths_and_changed_fingerprints() {
    let fixture = Fixture::with_note();
    let (id, raw) = first_file(&fixture).await;
    let before = std::fs::read(fixture.path()).expect("review bytes");
    let body = format!(r#"{{"fingerprint":"{id}","revision":2}}"#);
    for route in [
        "/api/v1/viewed".to_owned(),
        "/api/v1/viewed?path=".to_owned(),
        "/api/v1/viewed?path=zz".to_owned(),
        "/api/v1/viewed?path=6".to_owned(),
        "/api/v1/viewed?path=+f".to_owned(),
        "/api/v1/viewed?path=%66%69".to_owned(),
        format!("/api/v1/viewed?path=2e2e2f{raw}"),
        format!("/api/v1/viewed?path={raw}&path={raw}"),
        format!("/api/v1/viewed?path={}", raw.to_uppercase()),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{route}");
        assert_eq!(problem["code"], "invalid_path", "{route}");
    }

    let route = format!("/api/v1/viewed?path={raw}");
    for body in [
        r#"{"fingerprint":"not hex","revision":2}"#.to_owned(),
        format!(r#"{{"fingerprint":"{id}","revision":0}}"#),
        r#"{"revision":2}"#.to_owned(),
        "not json".to_owned(),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(problem["code"], "invalid_viewed_mark", "{body}");
    }
    // Revision 3 is after the current revision, so the lookup has no answer.
    for body in [
        format!(r#"{{"fingerprint":"{id}","revision":3}}"#),
        format!(r#"{{"fingerprint":"{}","revision":2}}"#, "0".repeat(64)),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(problem["code"], "viewed_mismatch", "{body}");
    }
    let missing = format!("/api/v1/viewed?path={}", viewed::encode_hex(b"other.rs"));
    let (status, problem) = send_json(fixture.state(), api_request(Method::PUT, &missing, Some(&body))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "viewed_mismatch");
    assert_eq!(std::fs::read(fixture.path()).expect("review bytes"), before);
}

#[tokio::test]
async fn a_viewed_mark_sends_a_viewed_event() {
    let fixture = Fixture::with_note();
    let (id, raw) = first_file(&fixture).await;
    let response = app(fixture.state())
        .oneshot(api_request(Method::GET, "/api/v1/events", None))
        .await
        .expect("response");
    let mut stream = response.into_body().into_data_stream();
    let body = format!(r#"{{"fingerprint":"{id}","revision":2}}"#);
    let (status, _) = send_json(
        fixture.state(),
        api_request(Method::PUT, &format!("/api/v1/viewed?path={raw}"), Some(&body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("an event arrives")
        .expect("the stream stays open")
        .expect("frame bytes");
    let frame = String::from_utf8(frame.to_vec()).expect("UTF-8 event");
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("event data");
    let event: serde_json::Value = serde_json::from_str(data).expect("event JSON");
    assert_eq!(event["kind"], "viewed");
    assert_eq!(event["revision"], 2);
}

#[test]
fn the_watcher_announces_marks_that_change_without_a_new_revision() {
    let fixture = Fixture::with_note();
    let state = fixture.state();
    let mut events = state.events.subscribe();
    let review = read_review(fixture.path()).expect("review");
    announce_viewed(&state, &review);
    assert!(events.try_recv().is_err(), "unchanged marks are not announced");

    let file = &review.changeset().files()[0];
    let marked = review
        .mark_viewed(
            "ana",
            file.canonical_path().clone(),
            file.fingerprint(),
            review.revision(),
        )
        .expect("mark");
    announce_viewed(&state, &marked);
    let event = events.try_recv().expect("an event for the new mark");
    assert_eq!((event.kind, event.revision), (ServerEventKind::Viewed, Some(2)));
    announce_viewed(&state, &marked);
    assert!(events.try_recv().is_err(), "a known mark is not announced again");
}

#[tokio::test]
async fn a_viewed_route_and_the_watcher_announce_one_change_once() {
    let fixture = Fixture::with_note();
    let (id, raw) = first_file(&fixture).await;
    let mut events = fixture.state.events.subscribe();
    let route = format!("/api/v1/viewed?path={raw}");
    let body = format!(r#"{{"fingerprint":"{id}","revision":2}}"#);
    let (status, _) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
    assert_eq!(status, StatusCode::OK);
    let event = events.try_recv().expect("the route announces the mark");
    assert_eq!(event.kind, ServerEventKind::Viewed);

    // The watcher then reloads the file the route wrote.
    announce_viewed(&fixture.state, &read_review(fixture.path()).expect("review"));
    assert!(
        events.try_recv().is_err(),
        "the watcher does not announce the mark again"
    );

    // A mark the watcher saw first is not announced again by the route.
    let review = read_review(fixture.path()).expect("review");
    let cleared = review
        .unmark_viewed(local_author().id(), &BytePath::new(b"file.rs".to_vec()).expect("path"))
        .expect("clear");
    announce_viewed(&fixture.state, &cleared);
    assert_eq!(
        events.try_recv().expect("the watcher announces").kind,
        ServerEventKind::Viewed
    );
    let (status, _) = send_json(fixture.state(), api_request(Method::DELETE, &route, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        events.try_recv().is_err(),
        "the route does not announce the clear again"
    );
}

/// Refreshes the one-note fixture to a changed `file.rs` at revision 3, and
/// returns the file's raw path.
async fn refreshed_note_fixture() -> (Fixture, String) {
    let fixture = Fixture::with_note();
    let (_, raw) = first_file(&fixture).await;
    let review = read_review(fixture.path()).expect("review");
    let changed = parse_patch(
        b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 3;\n",
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("changeset");
    let refreshed = review.reanchor(changed).expect("reanchored");
    crate::write_review_atomic(fixture.path(), &refreshed).expect("review is written");
    (fixture, raw)
}

#[test]
fn held_blobs_are_checked_once_per_revision_and_a_failure_is_not_kept() {
    let cache = history::HeldBlobCache::default();
    let id = mire_core::ObjectId::new(&"a".repeat(40)).expect("object id");
    let objects = BTreeSet::from([id.clone()]);
    let revision = |value| ReviewRevision::new(value).expect("positive revision");
    let checks = std::cell::Cell::new(0);
    let check = |objects: &BTreeSet<mire_core::ObjectId>| {
        checks.set(checks.get() + 1);
        Some(objects.clone())
    };

    let first = cache.held_with(revision(2), objects.clone(), check);
    assert_eq!(first.as_deref(), Some(&objects));
    let again = cache.held_with(revision(2), objects.clone(), check);
    assert_eq!(again.as_deref(), Some(&objects));
    assert_eq!(checks.get(), 1, "the same revision reuses the result");
    cache.held_with(revision(3), objects.clone(), check);
    assert_eq!(checks.get(), 2, "a new revision checks again");

    // A failed check reports unknown and leaves nothing to reuse.
    assert!(cache.held_with(revision(4), objects.clone(), |_| None).is_none());
    cache.held_with(revision(4), objects.clone(), check);
    assert_eq!(checks.get(), 3, "a failed check is tried again");
}

#[tokio::test]
async fn the_history_route_lists_a_files_checkpoints_and_reads_one() {
    let (fixture, raw) = refreshed_note_fixture().await;
    let (_, overview) = send_json(fixture.state(), api_request(Method::GET, "/api/v1/review", None)).await;
    let history = overview["files"][0]["history"].as_array().expect("history");
    let revisions = history
        .iter()
        .map(|entry| entry["revision"].clone())
        .collect::<Vec<_>>();
    assert_eq!(revisions, [serde_json::json!(3), serde_json::json!(1)]);
    // A patch review has no repository and kept no copy: only the current
    // capture can be shown.
    assert_eq!(history[0]["available"], true);
    assert_eq!(history[1]["available"], false);
    assert_eq!(history[1]["copy"], false);

    let (status, listed) = send_json(
        fixture.state(),
        api_request(Method::GET, &format!("/api/v1/history?path={raw}"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["history"], overview["files"][0]["history"]);
    assert_eq!(listed["path"]["raw"], raw);

    let (status, current) = send_json(
        fixture.state(),
        api_request(Method::GET, &format!("/api/v1/history?path={raw}&revision=3"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["source"], "capture");
    assert_eq!(current["checkpointRevision"], 3);
    assert_eq!(
        current["content"]["hunks"][0]["lines"][1]["content"]["display"],
        "let new = 3;"
    );

    let (status, problem) = send_json(
        fixture.state(),
        api_request(Method::GET, &format!("/api/v1/history?path={raw}&revision=2"), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "checkpoint_unavailable");
    for route in [
        format!("/api/v1/history?path={raw}&revision=4"),
        format!("/api/v1/history?path={}&revision=1", viewed::encode_hex(b"other.rs")),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::GET, &route, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{route}");
        assert_eq!(problem["code"], "checkpoint_not_found", "{route}");
    }
    for route in [
        format!("/api/v1/history?path={raw}&revision=0"),
        format!("/api/v1/history?path={raw}&revision=x"),
        format!("/api/v1/history?path={raw}&revision=1&revision=2"),
    ] {
        let (status, problem) = send_json(fixture.state(), api_request(Method::GET, &route, None)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{route}");
        assert_eq!(problem["code"], "invalid_revision", "{route}");
    }
    let (status, problem) = send_json(
        fixture.state(),
        api_request(Method::GET, "/api/v1/history?path=zz", None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["code"], "invalid_path");
}

#[tokio::test]
async fn a_finding_names_and_serves_the_checkpoint_it_was_written_on() {
    let fixture = Fixture::with_note();
    let note_id = fixture.note_id();
    let (_, finding) = send_json(
        fixture.state(),
        api_request(Method::GET, &format!("/api/v1/findings/{note_id}"), None),
    )
    .await;
    assert_eq!(finding["checkpoint"]["revision"], 1);
    assert_eq!(finding["checkpoint"]["path"]["display"], "file.rs");

    let route = format!("/api/v1/findings/{note_id}/checkpoint");
    let (status, written_on) = send_json(fixture.state(), api_request(Method::GET, &route, None)).await;
    assert_eq!(status, StatusCode::OK, "{written_on}");
    assert_eq!(written_on["source"], "capture");
    assert_eq!(written_on["checkpointRevision"], 1);

    // After the file changes, a patch review keeps no copy of the old text.
    let review = read_review(fixture.path()).expect("review");
    let changed = parse_patch(
        b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 3;\n",
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("changeset");
    let refreshed = review.reanchor(changed).expect("reanchored");
    crate::write_review_atomic(fixture.path(), &refreshed).expect("review is written");
    let (status, problem) = send_json(fixture.state(), api_request(Method::GET, &route, None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "checkpoint_unavailable");

    // With a copy kept, the old text comes from the copy.
    let kept = read_review(fixture.path()).expect("review");
    let mut json = serde_json::to_value(&kept).expect("json");
    json["checkpoints"][0]["files"][0]["copy"] = serde_json::json!({
        "hunks": match review.changeset().files()[0].content() {
            mire_core::FileContent::Text { hunks } => serde_json::to_value(hunks).expect("hunks"),
            mire_core::FileContent::Binary => unreachable!(),
        }
    });
    std::fs::write(fixture.path(), serde_json::to_vec(&json).expect("bytes")).expect("review is written");
    let (status, copied) = send_json(fixture.state(), api_request(Method::GET, &route, None)).await;
    assert_eq!(status, StatusCode::OK, "{copied}");
    assert_eq!(copied["source"], "copy");
    assert_eq!(
        copied["content"]["hunks"][0]["lines"][1]["content"]["display"],
        "let new = 2;"
    );

    let (status, problem) = send_json(
        fixture.state(),
        api_request(Method::GET, "/api/v1/findings/missing/checkpoint", None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "finding_not_found");
}

#[tokio::test]
async fn a_viewed_mark_at_an_older_revision_checks_the_file_as_it_was() {
    let fixture = Fixture::with_note();
    let (old, raw) = first_file(&fixture).await;
    let review = read_review(fixture.path()).expect("review");
    let changed = parse_patch(
        b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 3;\n",
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("changeset");
    crate::write_review_atomic(fixture.path(), &review.reanchor(changed).expect("reanchored"))
        .expect("review is written");
    let (new, _) = first_file(&fixture).await;
    let route = format!("/api/v1/viewed?path={raw}");

    let body = format!(r#"{{"fingerprint":"{old}","revision":2}}"#);
    let (status, marked) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
    assert_eq!(status, StatusCode::OK, "{marked}");
    assert_eq!(marked["viewed"]["revision"], 2);
    let body = format!(r#"{{"fingerprint":"{new}","revision":2}}"#);
    let (status, problem) = send_json(fixture.state(), api_request(Method::PUT, &route, Some(&body))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "viewed_mismatch");
}
