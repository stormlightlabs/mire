use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use mire::write_review_atomic;
use mire_core::{
    AnchorSide, AnnotationKind, Author, BytePath, ChangesetSource, LineNumber, LineRange, NoteInput, NoteSeverity,
    PatchLimits, Provenance, Review, ReviewRevision, parse_patch,
};
use serde_json::Value;

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n";

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mire-live-session-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TuiProcess {
    child: Child,
    reader: thread::JoinHandle<Vec<u8>>,
    writer: ChildStdin,
}

impl TuiProcess {
    fn review(directory: &Path, review: &Path, watch: bool) -> Self {
        let mut arguments = vec!["review", review.to_str().unwrap()];
        if watch {
            arguments.push("--watch");
        }
        Self::spawn(directory, &arguments)
    }

    fn patch_watch(directory: &Path, patch: &Path) -> Self {
        Self::spawn(directory, &["patch", patch.to_str().unwrap(), "--watch"])
    }

    fn spawn(directory: &Path, arguments: &[&str]) -> Self {
        let mut child = Command::new("script")
            .args(["-q", "/dev/null", binary()])
            .args(arguments)
            .env("NO_COLOR", "1")
            .env("XDG_RUNTIME_DIR", directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Mire can start in a PTY");
        let mut reader_source = child.stdout.take().expect("PTY output can be read");
        let reader = thread::spawn(move || {
            let mut output = Vec::new();
            reader_source
                .read_to_end(&mut output)
                .expect("PTY output can be captured");
            output
        });
        let writer = child.stdin.take().expect("PTY input can be written");
        Self { child, reader, writer }
    }

    fn finish(mut self) {
        self.writer.write_all(b"q").unwrap();
        self.writer.flush().unwrap();
        let status = self.child.wait().unwrap();
        drop(self.writer);
        let output = self.reader.join().unwrap();
        assert!(status.success(), "TUI failed: {}", String::from_utf8_lossy(&output));
    }
}

#[test]
fn live_session_inspects_and_navigates_an_open_review_without_writing_it() {
    let directory = TempDirectory::new();
    let review_path = directory.0.join("review.json");
    write_review_atomic(&review_path, &review_with_note()).unwrap();
    let mut process = TuiProcess::review(&directory.0, &review_path, false);
    thread::sleep(Duration::from_secs(1));

    assert!(
        process.child.try_wait().unwrap().is_none(),
        "TUI exited before live discovery"
    );
    let session = listed_session(&directory.0);
    assert_rejects_invalid_capability(&directory.0);
    let inspected = session_command(&directory.0, &["inspect", &session]);
    if inspected["status"] != "ok" {
        panic!("inspect failed: {inspected}");
    }
    assert_eq!(inspected["result"]["review_revision"], 2);

    let focused = session_command(&directory.0, &["focus", &session, "--note", "note-1"]);
    assert_eq!(focused["status"], "ok");
    assert_eq!(focused["result"]["selected_note_id"], "note-1");

    let location = session_command(
        &directory.0,
        &[
            "focus",
            &session,
            "--file",
            "file.rs",
            "--side",
            "new",
            "--start-line",
            "1",
        ],
    );
    assert_eq!(location["status"], "ok");

    let next = session_command(&directory.0, &["next", &session]);
    assert_eq!(next["status"], "ok");

    let walkthrough = session_command(&directory.0, &["walkthrough", "start", &session]);
    assert_eq!(walkthrough["status"], "ok");
    assert_eq!(walkthrough["result"]["walkthrough_active"], true);
    let previous = session_command(&directory.0, &["walkthrough", "previous", &session]);
    assert_eq!(previous["status"], "ok");
    let stopped = session_command(&directory.0, &["walkthrough", "stop", &session]);
    assert_eq!(stopped["result"]["walkthrough_active"], false);

    let reload = session_command(&directory.0, &["reload", &session]);
    assert_eq!(reload["status"], "error");
    assert_eq!(reload["error"]["code"], "reload_unavailable");
    process.finish();

    let sessions = session_command(&directory.0, &["list"]);
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 0);
}

#[test]
fn live_session_requests_a_watched_reload() {
    let directory = TempDirectory::new();
    let patch_path = directory.0.join("review.patch");
    fs::write(&patch_path, PATCH).unwrap();
    let mut process = TuiProcess::patch_watch(&directory.0, &patch_path);
    thread::sleep(Duration::from_secs(1));

    assert!(
        process.child.try_wait().unwrap().is_none(),
        "TUI exited before live discovery"
    );
    let session = listed_session(&directory.0);
    let reload = session_command(&directory.0, &["reload", &session]);
    assert_eq!(reload["status"], "ok");
    process.finish();
}

fn review_with_note() -> Review {
    let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
    let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
    let input = NoteInput::new(
        BytePath::new(b"file.rs".to_vec()).unwrap(),
        AnchorSide::New,
        LineRange::new(LineNumber::new(1).unwrap(), LineNumber::new(1).unwrap()).unwrap(),
        Author::new("agent", None).unwrap(),
        Provenance::Agent { producer: "test".to_owned() },
        NoteSeverity::Low,
        AnnotationKind::Comment,
        "A finding".to_owned(),
    )
    .unwrap();
    review.apply_notes(vec![input]).unwrap()
}

fn assert_rejects_invalid_capability(directory: &Path) {
    let descriptor_path = fs::read_dir(directory.join("mire"))
        .unwrap()
        .find_map(|entry| {
            let path = entry.unwrap().path();
            (path.extension().is_some_and(|extension| extension == "json")).then_some(path)
        })
        .unwrap();
    let descriptor: Value = serde_json::from_slice(&fs::read(descriptor_path).unwrap()).unwrap();
    let endpoint = descriptor["endpoint"].as_str().unwrap();
    let mut stream = UnixStream::connect(endpoint).unwrap();
    stream
        .write_all(br#"{"schema_version":{"major":1,"minor":0},"token":"invalid","operation":"inspect"}"#)
        .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let response: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["status"], "error");
    assert_eq!(response["error"]["code"], "authentication_failed");
}

fn listed_session(directory: &Path) -> String {
    let sessions = session_command(directory, &["list"]);
    let entries = sessions["sessions"].as_array().unwrap();
    assert!(!entries.is_empty(), "no session listed: {sessions}");
    entries[0]["id"].as_str().unwrap().to_owned()
}

fn session_command(directory: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(binary())
        .arg("session")
        .args(arguments)
        .env("XDG_RUNTIME_DIR", directory)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
