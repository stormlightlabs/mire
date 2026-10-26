use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n";
const DESCRIPTION: &str = "Fixes the parser.\n\nIt also trims spaces.\n";

/// A Git repository with one uncommitted edit, removed on drop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mire-metadata-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture directory can be created");
        let fixture = Self { root };
        fixture.git(&["init", "--quiet"]);
        fs::write(fixture.root.join("file.rs"), b"let old = 1;\n").unwrap();
        fixture.git(&["add", "."]);
        fixture.git(&[
            "-c",
            "user.name=Mire Tests",
            "-c",
            "user.email=mire@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "base",
        ]);
        fs::write(fixture.root.join("file.rs"), b"let new = 2;\n").unwrap();
        fs::write(fixture.root.join("change.patch"), PATCH).unwrap();
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn mire(&self, arguments: &[&str], stdin: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mire"))
            .args(arguments)
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("mire runs");
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        child.wait_with_output().unwrap()
    }

    fn git(&self, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(&self.root)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {arguments:?} failed");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn review_init_stores_title_and_description_and_context_reports_them() {
    let fixture = Fixture::new();
    fs::write(fixture.path("body.md"), DESCRIPTION).unwrap();
    let output = fixture.mire(
        &[
            "review",
            "init",
            "review.json",
            "--title",
            "Fix the parser",
            "--description-file",
            "body.md",
        ],
        b"",
    );
    assert_success(&output);

    let review: Value = serde_json::from_slice(&fs::read(fixture.path("review.json")).unwrap()).unwrap();
    assert_eq!(review["schema_version"]["minor"], 4);
    assert_eq!(review["title"], "Fix the parser");
    assert_eq!(review["description"], "Fixes the parser.\n\nIt also trims spaces.");

    let context = json(&fixture.mire(&["context", "review.json"], b""));
    assert_eq!(context["schema_version"]["minor"], 3);
    assert_eq!(context["payload"]["title"], "Fix the parser");
    assert_eq!(
        context["payload"]["description"],
        "Fixes the parser.\n\nIt also trims spaces."
    );

    let bounded = fixture.mire(&["context", "review.json", "--max-bytes", "64"], b"");
    assert!(!bounded.status.success(), "the description counts against --max-bytes");
}

#[test]
fn description_can_come_from_standard_input() {
    let fixture = Fixture::new();
    let output = fixture.mire(
        &["review", "init", "review.json", "--description-file", "-"],
        DESCRIPTION.as_bytes(),
    );
    assert_success(&output);
    let review: Value = serde_json::from_slice(&fs::read(fixture.path("review.json")).unwrap()).unwrap();
    assert!(review.get("title").is_none());
    assert_eq!(review["description"], "Fixes the parser.\n\nIt also trims spaces.");
}

#[test]
fn reviews_without_title_or_description_omit_them() {
    let fixture = Fixture::new();
    fs::write(fixture.path("blank.md"), " \n\n").unwrap();
    assert_success(&fixture.mire(
        &["review", "init", "review.json", "--description-file", "blank.md"],
        b"",
    ));
    let review: Value = serde_json::from_slice(&fs::read(fixture.path("review.json")).unwrap()).unwrap();
    assert!(review.get("title").is_none());
    assert!(
        review.get("description").is_none(),
        "a blank description counts as none"
    );
    let context = json(&fixture.mire(&["context", "review.json"], b""));
    assert!(context["payload"].get("title").is_none());
    assert!(context["payload"].get("description").is_none());
}

#[test]
fn invalid_title_and_description_are_rejected_before_writing() {
    let fixture = Fixture::new();
    fs::write(fixture.path("huge.md"), "d".repeat(64 * 1024 + 1)).unwrap();
    for (arguments, message) in [
        (
            vec!["review", "init", "review.json", "--title", "two\nlines"],
            "single line",
        ),
        (
            vec!["review", "init", "review.json", "--title", ""],
            "review title must contain",
        ),
        (
            vec!["review", "init", "review.json", "--description-file", "huge.md"],
            "exceeds the 65536-byte limit",
        ),
        (
            vec!["review", "init", "review.json", "--description-file", "missing.md"],
            "cannot read description",
        ),
    ] {
        let output = fixture.mire(&arguments, b"");
        assert_eq!(output.status.code(), Some(3), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{arguments:?}: {stderr}");
        assert!(!fixture.path("review.json").exists(), "{arguments:?}");
    }
}

#[test]
fn patch_accepts_a_description_file_but_not_standard_input() {
    let fixture = Fixture::new();
    fs::write(fixture.path("body.md"), DESCRIPTION).unwrap();
    let output = fixture.mire(
        &[
            "patch",
            "change.patch",
            "--format",
            "json",
            "--title",
            "Fix the parser",
            "--description-file",
            "body.md",
        ],
        b"",
    );
    assert_success(&output);

    for input in ["-", "change.patch"] {
        let output = fixture.mire(&["patch", input, "--description-file", "-"], PATCH);
        assert_eq!(output.status.code(), Some(3), "input {input}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("cannot read the description from standard input"),
            "{stderr}"
        );
        assert!(stderr.contains("process substitution"), "{stderr}");
    }
}

fn json(output: &Output) -> Value {
    assert_success(output);
    serde_json::from_slice(&output.stdout).expect("mire prints JSON")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "mire failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
