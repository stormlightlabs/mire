use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::{read_review, write_review_atomic};
use mire_core::{ChangesetSource, PatchLimits, Review, ReviewRevision, parse_patch};
use serde_json::{Value, json};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] =
    b"--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n context before\n-old value\n+new value\n context after\n";

#[test]
fn location_batches_are_atomic_and_mire_assigns_anchors_and_ids() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.review_path).unwrap();
    let invalid = json!({
        "schema_version": { "major": 1, "minor": 1 },
        "review_revision": 1,
        "notes": [
            request("src/lib.rs", 1, "context only"),
            request("missing.rs", 2, "missing")
        ]
    });
    let rejected = mire_with_stdin(
        &["notes", "apply", fixture.review_str(), "--stdin"],
        serde_json::to_string(&invalid).unwrap().as_bytes(),
    );
    assert_eq!(rejected.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "context_only_location");
    assert_eq!(report["failures"][1]["code"], "location_not_found");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);

    let valid = json!({
        "schema_version": { "major": 1, "minor": 1 },
        "review_revision": 1,
        "notes": [request("src/lib.rs", 2, "A concrete defect")]
    });
    let applied = mire_with_stdin(
        &["notes", "apply", fixture.review_str(), "--stdin"],
        serde_json::to_string(&valid).unwrap().as_bytes(),
    );
    assert_success(&applied);

    let review = read_review(&fixture.review_path).unwrap();
    assert_eq!(review.revision().get(), 2);
    assert_eq!(review.notes()[0].id().as_str(), "note-1");
    assert_eq!(review.notes()[0].anchor().range().start().get(), 2);
    assert!(review.notes()[0].anchor().validate(review.changeset()).is_ok());
}

#[test]
fn hunk_context_is_bounded_and_mutations_require_the_observed_revision() {
    let fixture = Fixture::new();
    let review = read_review(&fixture.review_path).unwrap();
    let hunk = match review.changeset().files()[0].content() {
        mire_core::FileContent::Text { hunks } => &hunks[0],
        mire_core::FileContent::Binary => panic!("fixture is text"),
    };
    let expanded = mire(&[
        "context",
        fixture.review_str(),
        "--hunk",
        &hunk.fingerprint().to_string(),
        "--max-bytes",
        "65536",
    ]);
    assert_success(&expanded);
    let context: Value = serde_json::from_slice(&expanded.stdout).unwrap();
    assert_eq!(context["payload"]["kind"], "hunk");

    let added = mire(&[
        "note",
        "add",
        fixture.review_str(),
        "--revision",
        "1",
        "--file",
        "src/lib.rs",
        "--new-line",
        "2",
        "--author",
        "review-agent",
        "--provenance",
        "agent",
        "--producer",
        "test-agent",
        "--severity",
        "high",
        "--kind",
        "defect",
        "--body",
        "The new value is unsafe.",
    ]);
    assert_success(&added);

    let before_stale = fs::read(&fixture.review_path).unwrap();
    let stale = mire(&[
        "note",
        "resolve",
        fixture.review_str(),
        "note-1",
        "--revision",
        "1",
        "--author",
        "reviewer",
    ]);
    assert_eq!(stale.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&stale.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "revision_conflict");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before_stale);

    let resolved = mire(&[
        "note",
        "resolve",
        fixture.review_str(),
        "note-1",
        "--revision",
        "2",
        "--author",
        "reviewer",
    ]);
    assert_success(&resolved);
    let review = read_review(&fixture.review_path).unwrap();
    assert_eq!(review.revision().get(), 3);
    assert_eq!(review.notes()[0].status(), mire_core::NoteStatus::Resolved);
    assert_eq!(review.events().last().unwrap().author().id(), "reviewer");
}

fn request(file: &str, new_line: u64, body: &str) -> Value {
    json!({
        "file": file,
        "new_line": new_line,
        "author": { "id": "review-agent", "display_name": null },
        "provenance": { "kind": "agent", "producer": "test-agent" },
        "severity": "medium",
        "kind": "defect",
        "body": body
    })
}

struct Fixture {
    directory: PathBuf,
    review_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-note-commands-{}-{id}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let review_path = directory.join("review.json");
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        write_review_atomic(&review_path, &review).unwrap();
        Self { directory, review_path }
    }

    fn review_str(&self) -> &str {
        self.review_path.to_str().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn mire(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mire")).args(args).output().unwrap()
}

fn mire_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "mire failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
