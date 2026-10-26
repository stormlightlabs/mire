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
    directory: PathBuf,
    root: PathBuf,
    child: Child,
    address: String,
    secret: String,
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
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.name", "Mire Tests"]);
        git(&root, &["config", "user.email", "mire@example.invalid"]);
        fs::write(root.join("file.txt"), b"old\n").unwrap();
        git(&root, &["add", "file.txt"]);
        git(&root, &["commit", "--quiet", "-m", "base"]);
        fs::write(root.join("file.txt"), b"new\n").unwrap();
        let init = Command::new(binary())
            .args(["review", "init"])
            .arg(&review)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stderr));

        let mut child = Command::new(binary())
            .arg("serve")
            .arg(&review)
            .current_dir(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
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
        Self { directory, root, child, address: address.to_owned(), secret: secret.to_owned() }
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
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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

    fs::write(server.root.join("file.txt"), b"newer\n").unwrap();
    let response = server.request(
        "POST",
        "/api/v1/refresh",
        &[("Host", &host), ("Authorization", &authorization), ("Origin", &origin)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let refresh: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(refresh["revision"], 2, "the watcher or the request refreshed once");

    read_until(&mut events, &mut received, |text| text.contains("\"kind\":\"refresh\""));
    assert!(received.contains("event: mire"), "{received}");
    assert!(received.contains("\"revision\":2"), "{received}");

    let response = server.request(
        "GET",
        "/api/v1/review",
        &[("Host", &host), ("Authorization", &authorization)],
    );
    let overview: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(overview["revision"], 2);
    assert_eq!(overview["changes"]["additions"], 1);
}

#[test]
fn an_external_review_write_streams_a_review_invalidated_event() {
    let server = Server::start();
    let (mut events, mut received) = server.events();

    // A refresh event from the watcher shows its loop is running with a known
    // revision, so the next review-file write is compared against revision 2.
    fs::write(server.root.join("file.txt"), b"newer\n").unwrap();
    read_until(&mut events, &mut received, |text| text.contains("\"kind\":\"refresh\""));
    assert!(received.contains("\"revision\":2"), "{received}");

    let added = Command::new(binary())
        .args(["note", "add"])
        .arg(server.directory.join("review.json"))
        .args([
            "--revision",
            "2",
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
        text.contains("\"kind\":\"review_invalidated\"")
    });
    let event = received
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str::<serde_json::Value>(data).unwrap())
        .find(|event| event["kind"] == "review_invalidated")
        .unwrap();
    assert_eq!(event["revision"], 3, "{received}");
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
