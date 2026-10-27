use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::{read_review, write_review_atomic, write_review_atomic_if_revision};
use mire_core::{BytePath, ChangesetSource, PatchLimits, Review, ReviewRevision, parse_patch};
use serde_json::Value;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
const CHANGED: &[u8] = b"--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+newer\n";

#[test]
fn marking_and_unmarking_keep_the_revision_and_report_the_mark() {
    let fixture = Fixture::new();
    let fingerprint = fixture.fingerprint();
    let marked = mire(&[
        "file",
        "viewed",
        fixture.review_str(),
        "--file",
        "src/lib.rs",
        "--fingerprint",
        &fingerprint,
        "--revision",
        "1",
        "--reader",
        "ana",
    ]);
    assert_success(&marked);
    let result: Value = serde_json::from_slice(&marked.stdout).unwrap();
    assert_eq!(result["status"], "viewed");
    assert_eq!(result["review_revision"], 1);
    assert_eq!(result["reader"], "ana");
    assert_eq!(result["mark"]["fingerprint"], fingerprint);
    assert_eq!(result["mark"]["revision"], 1);
    let review = read_review(&fixture.review_path).unwrap();
    assert_eq!(review.revision().get(), 1);
    assert!(review.is_viewed("ana", &review.changeset().files()[0]));

    let context = mire(&["context", fixture.review_str(), "--reader", "ana"]);
    assert_success(&context);
    let context: Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(context["payload"]["viewed"]["reader"], "ana");
    assert_eq!(context["payload"]["viewed"]["marks"][0]["fingerprint"], fingerprint);
    let plain = mire(&["context", fixture.review_str()]);
    let plain: Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert!(plain["payload"].get("viewed").is_none());

    let cleared = mire(&[
        "file",
        "unviewed",
        fixture.review_str(),
        "--file",
        "src/lib.rs",
        "--reader",
        "ana",
    ]);
    assert_success(&cleared);
    let result: Value = serde_json::from_slice(&cleared.stdout).unwrap();
    assert_eq!(result["status"], "unviewed");
    assert_eq!(result["mark"], Value::Null);
    let review = read_review(&fixture.review_path).unwrap();
    assert_eq!(review.revision().get(), 1);
    assert!(review.viewed().is_empty());
}

#[test]
fn a_mark_for_a_changed_fingerprint_is_rejected() {
    let fixture = Fixture::new();
    let old_fingerprint = fixture.fingerprint();
    let mark = |fingerprint: &str, revision: &str| {
        mire(&[
            "file",
            "viewed",
            fixture.review_str(),
            "--file",
            "src/lib.rs",
            "--fingerprint",
            fingerprint,
            "--revision",
            revision,
            "--reader",
            "ana",
        ])
    };
    assert_success(&mark(&old_fingerprint, "1"));

    let review = read_review(&fixture.review_path).unwrap();
    let changed = parse_patch(CHANGED, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
    let refreshed = review.reanchor(changed).unwrap();
    write_review_atomic_if_revision(&fixture.review_path, review.revision(), &refreshed).unwrap();
    let refreshed = read_review(&fixture.review_path).unwrap();
    let file = &refreshed.changeset().files()[0];
    assert!(!refreshed.is_viewed("ana", file));
    let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
    assert_eq!(
        refreshed.viewed_mark("ana", &path).unwrap().fingerprint().to_string(),
        old_fingerprint
    );

    let before = fs::read(&fixture.review_path).unwrap();
    let rejected = mark(&old_fingerprint, "2");
    assert_eq!(rejected.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "viewed_mismatch");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);

    let new_fingerprint = file.fingerprint().to_string();
    assert_success(&mark(&new_fingerprint, "2"));
    let review = read_review(&fixture.review_path).unwrap();
    assert!(review.is_viewed("ana", &review.changeset().files()[0]));
}

#[test]
fn an_unsafe_path_is_rejected() {
    let fixture = Fixture::new();
    let rejected = mire(&["file", "unviewed", fixture.review_str(), "--file", "../escape"]);
    assert_eq!(rejected.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "invalid_path");
}

struct Fixture {
    directory: PathBuf,
    review_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-viewed-commands-{}-{id}", std::process::id()));
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

    fn fingerprint(&self) -> String {
        read_review(&self.review_path).unwrap().changeset().files()[0]
            .fingerprint()
            .to_string()
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

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
