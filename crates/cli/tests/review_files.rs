use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use mire::{read_review, write_review_atomic};
use mire_core::{Changeset, ChangesetSource, Fingerprint, Review, ReviewRevision};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn atomic_review_files_replace_the_previous_revision_and_open_through_the_binary() {
    let directory = test_directory();
    let path = directory.join("review.json");
    write_review_atomic(&path, &review(1)).expect("first review can be written");
    write_review_atomic(&path, &review(2)).expect("next revision atomically replaces the first");

    let loaded = read_review(&path).expect("replaced review remains valid");
    assert_eq!(loaded.revision().get(), 2);
    let output = Command::new(binary())
        .arg("review")
        .arg(&path)
        .args(["--format", "json"])
        .output()
        .expect("mire runs");
    assert!(output.status.success(), "mire failed: {}", stderr(&output.stderr));
    let from_binary: Review = serde_json::from_slice(&output.stdout).expect("binary emits canonical review JSON");
    assert_eq!(from_binary, loaded);
    fs::remove_dir_all(directory).expect("fixture directory can be removed");
}

#[test]
fn unsupported_schema_majors_fail_without_rewriting_the_file() {
    let directory = test_directory();
    let path = directory.join("future-review.json");
    let valid = serde_json::to_string(&review(1)).expect("fixture review serializes");
    let future = valid.replacen("\"major\":1", "\"major\":2", 1);
    fs::write(&path, future.as_bytes()).expect("future fixture can be written");
    let before = fs::read(&path).expect("future fixture can be read");

    let output = Command::new(binary())
        .arg("review")
        .arg(&path)
        .args(["--format", "json"])
        .output()
        .expect("mire runs");
    assert_eq!(output.status.code(), Some(7));
    assert!(stderr(&output.stderr).contains("unsupported review schema major 2"));
    assert_eq!(fs::read(&path).expect("rejected file still exists"), before);
    fs::remove_dir_all(directory).expect("fixture directory can be removed");
}

#[test]
fn invalid_and_interrupted_siblings_leave_the_last_valid_review_recoverable() {
    let directory = test_directory();
    let path = directory.join("review.json");
    let expected = review(1);
    write_review_atomic(&path, &expected).expect("valid review can be written");
    fs::write(directory.join(".review.json.mire-write-interrupted"), b"{invalid json")
        .expect("an interrupted sibling can be simulated");

    assert_eq!(read_review(&path).expect("destination remains recoverable"), expected);
    fs::remove_dir_all(directory).expect("fixture directory can be removed");
}

fn review(revision: u64) -> Review {
    Review::new(
        ReviewRevision::new(revision).expect("fixture revision is positive"),
        Changeset::new(
            ChangesetSource::DirectFiles,
            Vec::new(),
            Fingerprint::new([revision as u8; 32]),
        ),
        Vec::new(),
        Vec::new(),
    )
    .expect("empty fixture review is valid")
}

fn test_directory() -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mire-review-files-{}-{id}", std::process::id()));
    fs::create_dir(&path).expect("fixture directory can be created");
    path
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}

fn stderr(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
