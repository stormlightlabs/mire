use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::read_review;
use serde_json::Value;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
    review: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-checkpoints-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        fs::create_dir_all(&root).unwrap();
        git(&root, ["init", "--quiet"]);
        git(&root, ["config", "user.name", "Mire Tests"]);
        git(&root, ["config", "user.email", "mire@example.invalid"]);
        let review = directory.join("review.json");
        Self { directory, root, review }
    }

    fn write(&self, name: &str, content: &str) {
        fs::write(self.root.join(name), content).unwrap();
    }

    fn commit(&self, message: &str) {
        git(&self.root, ["add", "--all"]);
        git(&self.root, ["commit", "--quiet", "-m", message]);
    }

    fn review(&self) -> &str {
        self.review.to_str().unwrap()
    }

    fn mire(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mire"))
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .unwrap()
    }

    fn json(&self, arguments: &[&str]) -> Value {
        let output = self.mire(arguments);
        assert_success(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn rejection(&self, arguments: &[&str]) -> String {
        let output = self.mire(arguments);
        assert_eq!(
            output.status.code(),
            Some(8),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let report: Value = serde_json::from_slice(&output.stderr).unwrap();
        report["failures"][0]["code"].as_str().unwrap().to_owned()
    }

    fn history(&self, file: &str) -> Value {
        self.json(&["file", "history", self.review(), "--file", file])
    }

    fn at(&self, file: &str, revision: &str) -> Value {
        self.json(&["file", "history", self.review(), "--file", file, "--revision", revision])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn worktree_checkpoints_keep_copies_because_git_lacks_the_blobs() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "one\n");
    fixture.commit("base");
    fixture.write("a.txt", "two\n");
    assert_success(&fixture.mire(&["review", "init", fixture.review()]));
    fixture.write("a.txt", "three\n");
    assert_success(&fixture.mire(&["review", "refresh", fixture.review()]));

    let review = read_review(&fixture.review).unwrap();
    assert_eq!(review.revision().get(), 2);
    let objects = review.checkpoints()[0].files()[0].new_object().cloned().unwrap();
    assert_eq!(objects.as_str().len(), 40);

    let history = fixture.history("a.txt");
    let entries = history["history"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["revision"], 2);
    assert_eq!(entries[1]["revision"], 1);
    for entry in entries {
        assert_eq!(entry["copy"], true, "{entry}");
        assert_eq!(entry["available"], true, "{entry}");
    }

    let first = fixture.at("a.txt", "1");
    assert_eq!(first["source"], "copy");
    assert_eq!(first["checkpoint_revision"], 1);
    assert_eq!(first["fingerprint"], entries[1]["fingerprint"]);
    assert!(first["content"].to_string().contains(&byte_array("two")));
    let current = fixture.at("a.txt", "2");
    assert_eq!(current["source"], "capture");
    assert!(current["content"].to_string().contains(&byte_array("three")));

    assert_eq!(
        fixture.rejection(&[
            "file",
            "history",
            fixture.review(),
            "--file",
            "b.txt",
            "--revision",
            "1"
        ]),
        "checkpoint_not_found"
    );
    assert_eq!(
        fixture.rejection(&[
            "file",
            "history",
            fixture.review(),
            "--file",
            "a.txt",
            "--revision",
            "3"
        ]),
        "checkpoint_not_found"
    );

    // A mark at the older revision is checked against the file as it was.
    let old = entries[1]["fingerprint"].as_str().unwrap();
    let new = entries[0]["fingerprint"].as_str().unwrap();
    let mark = |fingerprint: &str| {
        fixture.mire(&[
            "file",
            "viewed",
            fixture.review(),
            "--file",
            "a.txt",
            "--fingerprint",
            fingerprint,
            "--revision",
            "1",
            "--reader",
            "ana",
        ])
    };
    assert_success(&mark(old));
    let output = mark(new);
    assert_eq!(output.status.code(), Some(8));
    assert!(String::from_utf8_lossy(&output.stderr).contains("viewed_mismatch"));

    // Without the copy, and with Git lacking the worktree blob, the old text
    // is gone.
    let mut stored: Value = serde_json::from_slice(&fs::read(&fixture.review).unwrap()).unwrap();
    stored["checkpoints"][0]["files"][0]["copy"] = Value::from("skipped");
    fs::write(&fixture.review, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(fixture.history("a.txt")["history"][1]["available"], false);
    assert_eq!(
        fixture.rejection(&[
            "file",
            "history",
            fixture.review(),
            "--file",
            "a.txt",
            "--revision",
            "1"
        ]),
        "checkpoint_unavailable"
    );
}

#[test]
fn committed_checkpoints_read_the_blobs_from_git_without_copies() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "one\n");
    fixture.commit("base");
    fixture.write("a.txt", "two\n");
    fixture.write("added.txt", "first\nsecond\n");
    fixture.commit("change");
    git(&fixture.root, ["tag", "base", "HEAD~1"]);
    assert_success(&fixture.mire(&["review", "init", fixture.review(), "base", "HEAD"]));
    fixture.write("a.txt", "three\n");
    fixture.write("added.txt", "first\nchanged\n");
    fixture.commit("again");
    assert_success(&fixture.mire(&["review", "refresh", fixture.review()]));

    let review = read_review(&fixture.review).unwrap();
    assert_eq!(review.checkpoints().len(), 2);
    for checkpoint in review.checkpoints() {
        for row in checkpoint.files() {
            assert!(row.copy().hunks().is_none(), "Git holds every committed blob");
        }
    }
    let history = fixture.history("a.txt");
    assert_eq!(history["history"][1]["copy"], false);
    assert_eq!(history["history"][1]["available"], true);

    let first = fixture.at("a.txt", "1");
    assert_eq!(first["source"], "git");
    let content = first["content"].to_string();
    assert!(content.contains(&byte_array("one")) && content.contains(&byte_array("two")));
    assert!(!content.contains(&byte_array("three")));

    let added = fixture.at("added.txt", "1");
    assert_eq!(added["source"], "git");
    let hunks = added["content"]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0]["old_line_count"], 0);
    assert_eq!(hunks[0]["new_line_count"], 2);
    assert!(added["content"].to_string().contains(&byte_array("second")));
}

#[test]
fn staged_checkpoints_need_no_copy() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "one\n");
    fixture.commit("base");
    fixture.write("a.txt", "two\n");
    git(&fixture.root, ["add", "a.txt"]);
    assert_success(&fixture.mire(&["review", "init", fixture.review(), "--staged"]));
    let review = read_review(&fixture.review).unwrap();
    assert!(review.checkpoints()[0].files()[0].copy().hunks().is_none());
    assert_eq!(fixture.history("a.txt")["history"][0]["available"], true);
}

fn byte_array(text: &str) -> String {
    let bytes = text.bytes().map(|byte| byte.to_string()).collect::<Vec<_>>();
    format!("[{}]", bytes.join(","))
}

fn git<I, S>(path: &Path, arguments: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git").args(arguments).current_dir(path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
