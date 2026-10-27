//! End-to-end tests for `mire serve` over a real loopback socket.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

/// A Git repository with an uncommitted change, a review bound to it, and a
/// running `mire serve` process that stops when the fixture drops.
struct Server {
    cleanup: Cleanup,
    root: PathBuf,
    address: String,
    secret: String,
}

/// Owns the fixture's temporary directory and server process from the moment
/// each exists, so a panic anywhere in setup still stops the process and
/// removes the directory.
struct Cleanup {
    directory: PathBuf,
    child: Option<Child>,
}

struct HttpResponse {
    status: u16,
    body: String,
}

impl Server {
    fn start() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-serve-e2e-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        let review = directory.join("review.json");
        let mut cleanup = Cleanup { directory, child: None };
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.name", "Mire Tests"]);
        git(&root, &["config", "user.email", "mire@example.invalid"]);
        fs::write(root.join("file.txt"), b"old\n").unwrap();
        git(&root, &["add", "file.txt"]);
        git(
            &root,
            &["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "base"],
        );
        fs::write(root.join("file.txt"), b"new\n").unwrap();
        let init = Command::new(binary())
            .args(["review", "init"])
            .arg(&review)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));

        let child = cleanup.child.insert(
            Command::new(binary())
                .arg("serve")
                .arg(&review)
                .current_dir(&root)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let url = line
            .trim()
            .strip_prefix("Mire review server: http://")
            .unwrap_or_else(|| panic!("unexpected server banner: {line:?}"));
        let (address, secret) = url
            .split_once("/#")
            .expect("the URL carries the secret in its fragment");
        let (address, secret) = (address.to_owned(), secret.to_owned());
        Self { cleanup, root, address, secret }
    }

    fn directory(&self) -> &Path {
        &self.cleanup.directory
    }

    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(&self.address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        stream
    }

    fn request(&self, method: &str, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
        let mut stream = self.connect();
        let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: 0\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let status = response
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or_else(|| panic!("malformed response: {response:?}"));
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .unwrap_or_default();
        HttpResponse { status, body }
    }

    /// Opens the event stream and reads past its response headers, which
    /// arrive once the server has subscribed this client. Returns the stream
    /// and any event text that arrived with the headers.
    fn events(&self) -> (TcpStream, String) {
        let mut events = self.connect();
        events
            .write_all(
                format!(
                    "GET /api/v1/events HTTP/1.1\r\nHost: {}\r\nAuthorization: {}\r\n\r\n",
                    self.address,
                    self.authorization()
                )
                .as_bytes(),
            )
            .unwrap();
        let mut headers = String::new();
        read_until(&mut events, &mut headers, |text| text.contains("\r\n\r\n"));
        assert!(headers.starts_with("HTTP/1.1 200"), "{headers}");
        assert!(
            headers.to_ascii_lowercase().contains("content-type: text/event-stream"),
            "{headers}"
        );
        let (_, body) = headers.split_once("\r\n\r\n").unwrap();
        (events, body.to_owned())
    }

    fn authorization(&self) -> String {
        format!("Bearer {}", self.secret)
    }

    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Replaces the tracked file in one step. `fs::write` truncates the file
    /// before writing it, and a watcher refresh that lands between the two
    /// records the empty file as a revision of its own.
    fn edit_source(&self, contents: &[u8]) {
        let staged = self.directory().join("file.txt.next");
        fs::write(&staged, contents).unwrap();
        fs::rename(&staged, self.root.join("file.txt")).unwrap();
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn serve_prints_a_loopback_url_and_guards_its_api_on_the_socket() {
    let server = Server::start();
    assert!(server.address.starts_with("127.0.0.1:"), "{}", server.address);
    assert!(server.secret.len() >= 43, "the secret has 256 bits");
    let host = server.address.clone();
    let authorization = server.authorization();

    let response = server.request("GET", "/api/v1/review", &[("Host", &host)]);
    assert_eq!(response.status, 401);
    assert!(!response.body.contains("file.txt"), "no review data without the secret");

    let response = server.request(
        "GET",
        "/api/v1/review",
        &[("Host", "localhost"), ("Authorization", &authorization)],
    );
    assert_eq!(response.status, 400);

    let response = server.request(
        "POST",
        "/api/v1/refresh",
        &[
            ("Host", &host),
            ("Authorization", &authorization),
            ("Origin", "http://evil.example"),
        ],
    );
    assert_eq!(response.status, 403);

    let response = server.request(
        "GET",
        "/api/v1/review",
        &[("Host", &host), ("Authorization", &authorization)],
    );
    assert_eq!(response.status, 200);
    let overview: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(overview["revision"], 1);
    assert_eq!(overview["files"][0]["path"]["display"], "file.txt");
    assert!(
        response.body.find(&server.secret).is_none(),
        "the API never echoes the session secret"
    );
}

#[test]
fn a_source_change_refreshes_the_review_and_streams_an_event() {
    let server = Server::start();
    let host = server.address.clone();
    let authorization = server.authorization();
    let origin = server.origin();

    let (mut events, mut received) = server.events();

    server.edit_source(b"newer\n");
    let response = server.request(
        "POST",
        "/api/v1/refresh",
        &[("Host", &host), ("Authorization", &authorization), ("Origin", &origin)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let refresh: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    // The watcher and the request race to refresh, so either may write the
    // new revision; the other finds the capture current. The source changes
    // in one step, so there is exactly one new revision.
    let revision = refresh["revision"].as_u64().unwrap();
    assert_eq!(revision, 2, "{refresh}");

    read_until(&mut events, &mut received, |text| {
        mire_events(text).any(|event| event["kind"] == "refresh" && event["revision"] == revision)
    });
    assert!(received.contains("event: mire"), "{received}");

    let response = server.request(
        "GET",
        "/api/v1/review",
        &[("Host", &host), ("Authorization", &authorization)],
    );
    let overview: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(overview["revision"], revision);
    assert_eq!(overview["changes"]["additions"], 1);
}

#[test]
fn an_external_review_write_streams_a_review_invalidated_event() {
    let server = Server::start();
    let (mut events, mut received) = server.events();

    // A refresh event from the watcher shows its loop is running with a known
    // revision, so the next review-file write is compared against it.
    server.edit_source(b"newer\n");
    read_until(&mut events, &mut received, |text| {
        mire_events(text).any(|event| event["kind"] == "refresh")
    });
    let refreshed = mire_events(&received).find(|event| event["kind"] == "refresh").unwrap()["revision"]
        .as_u64()
        .unwrap();

    let added = Command::new(binary())
        .args(["note", "add"])
        .arg(server.directory().join("review.json"))
        .args([
            "--revision",
            &refreshed.to_string(),
            "--file",
            "file.txt",
            "--new-line",
            "1",
            "--author",
            "review-agent",
            "--provenance",
            "agent",
            "--producer",
            "test-agent",
            "--severity",
            "low",
            "--kind",
            "comment",
            "--body",
            "Written by another process.",
        ])
        .current_dir(&server.root)
        .output()
        .unwrap();
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));

    read_until(&mut events, &mut received, |text| {
        mire_events(text).any(|event| event["kind"] == "review_invalidated")
    });
    let event = mire_events(&received)
        .find(|event| event["kind"] == "review_invalidated")
        .unwrap();
    assert_eq!(event["revision"], refreshed + 1, "{received}");
}

/// Parses each complete `data:` line of an event stream, skipping a final
/// line whose newline has not arrived yet, and fails on a complete line that
/// is not JSON.
fn mire_events(text: &str) -> impl Iterator<Item = serde_json::Value> + '_ {
    text.split_inclusive('\n')
        .filter_map(|line| line.strip_suffix('\n'))
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap_or_else(|error| panic!("malformed event data {data:?}: {error}")))
}

/// Reads from `stream` into `text` until `done` holds, failing after ten seconds.
fn read_until(stream: &mut TcpStream, text: &mut String, done: impl Fn(&str) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = [0_u8; 4096];
    while !done(text) {
        assert!(Instant::now() < deadline, "timed out; received: {text}");
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "the stream closed; received: {text}");
        text.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git").args(arguments).current_dir(root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
