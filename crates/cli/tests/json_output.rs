use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

/// A Git repository with one commit and a worktree edit, captured in a review
/// file that holds one finding.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mire-json-output-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture repository directory can be created");
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.name", "Mire Tests"]);
        git(&root, &["config", "user.email", "mire@example.invalid"]);
        fs::write(
            root.join("config.rs"),
            b"const PORT: u16 = 80;\nconst HOST: &str = \"localhost\";\n",
        )
        .expect("base source can be written");
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "base"]);
        fs::write(
            root.join("config.rs"),
            b"const PORT: u16 = 8080;\nconst HOST: &str = \"127.0.0.1\";\nconst RETRIES: u8 = 3;\n",
        )
        .expect("source edit can be written");

        let fixture = Self { root };
        fixture.run(&["review", "init", "review.json"]);
        fixture.run(&[
            "note",
            "add",
            "review.json",
            "--revision",
            "1",
            "--file",
            "config.rs",
            "--new-line",
            "1",
            "--author",
            "reviewer",
            "--provenance",
            "agent",
            "--producer",
            "fixture",
            "--severity",
            "medium",
            "--kind",
            "defect",
            "--body",
            "Port 8080 collides with the dev server.",
        ]);
        fixture
    }

    fn json(&self, arguments: &[&str]) -> Value {
        serde_json::from_slice(&self.run(arguments).stdout).expect("Mire prints JSON")
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_mire"))
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .expect("Mire runs");
        assert!(
            output.status.success(),
            "mire {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn context_manifest_json_is_stable() {
    let fixture = Fixture::new();
    insta::assert_json_snapshot!("context_manifest", fixture.json(&["context", "review.json"]));
}

#[test]
fn review_status_json_is_stable() {
    let fixture = Fixture::new();
    insta::assert_json_snapshot!(
        "review_status",
        fixture.json(&["review", "status", "review.json", "--format", "json"])
    );
}

fn git(path: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
