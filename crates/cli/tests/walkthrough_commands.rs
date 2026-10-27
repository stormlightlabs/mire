use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::{read_review, write_review_atomic};
use mire_core::{ChangesetSource, PatchLimits, Review, ReviewRevision, parse_patch};
use serde_json::{Value, json};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n context before\n-old value\n+new value\n context after\ndiff --git a/src/other.rs b/src/other.rs\n--- a/src/other.rs\n+++ b/src/other.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";

#[test]
fn apply_replaces_the_walkthrough_and_mire_assigns_ids_and_anchors() {
    let fixture = Fixture::new();
    let applied = fixture.apply(&batch(1, vec![contract_step(), implementation_step()]));
    assert_success(&applied);
    let result: Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["review_revision"], 2);
    assert_eq!(result["steps"], json!(["contract", "step-2"]));

    let review = read_review(&fixture.review_path).unwrap();
    let walkthrough = review.walkthrough().unwrap();
    assert_eq!(walkthrough.overview(), Some("Swap the value."));
    let anchor = walkthrough.steps()[0].anchors()[0].anchor();
    assert_eq!(anchor.range().start().get(), 2);
    assert!(anchor.validate(review.changeset()).is_ok());
    assert_eq!(walkthrough.steps()[1].depends_on()[0].as_str(), "contract");

    let replaced = fixture.apply(&batch(2, vec![contract_step()]));
    assert_success(&replaced);
    let review = read_review(&fixture.review_path).unwrap();
    assert_eq!(review.revision().get(), 3);
    assert_eq!(review.walkthrough().unwrap().steps().len(), 1);
}

#[test]
fn invalid_anchors_are_rejected_with_stable_codes_and_write_nothing() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.review_path).unwrap();
    let cases = [
        (
            step(Some("a"), "Missing", json!({ "file": "missing.rs", "new_line": 1 })),
            "location_not_found",
        ),
        (
            step(Some("a"), "Context", json!({ "file": "src/lib.rs", "new_line": 1 })),
            "context_only_location",
        ),
        (
            step(Some("a"), "Outside", json!({ "file": "src/lib.rs", "new_line": 9 })),
            "location_not_found",
        ),
    ];
    for (bad, code) in cases {
        let rejected = fixture.apply(&batch(1, vec![contract_step(), bad]));
        assert_eq!(rejected.status.code(), Some(8));
        let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
        assert_eq!(report["status"], "rejected");
        assert_eq!(report["failures"][0]["code"], code);
        assert_eq!(report["failures"][0]["step_index"], 1);
        assert_eq!(fs::read(&fixture.review_path).unwrap(), before);
    }

    let without_anchors = json!({ "title": "Empty", "narrative": "Nothing.", "locations": [] });
    let rejected = fixture.apply(&batch(1, vec![without_anchors]));
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "step_without_anchors");

    let malformed = step(
        None,
        "Both sides",
        json!({ "file": "src/lib.rs", "old_line": 2, "new_line": 2 }),
    );
    let rejected = fixture.apply(&batch(1, vec![malformed]));
    assert_eq!(rejected.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "invalid_input");

    let mut human = batch(1, vec![contract_step()]);
    human["provenance"] = json!({ "kind": "human" });
    let rejected = fixture.apply(&human);
    let report: Value = serde_json::from_slice(&rejected.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "human_provenance_forbidden");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);
}

#[test]
fn stale_revisions_are_rejected_for_apply_and_clear() {
    let fixture = Fixture::new();
    assert_success(&fixture.apply(&batch(1, vec![contract_step()])));
    let before = fs::read(&fixture.review_path).unwrap();

    let stale_apply = fixture.apply(&batch(1, vec![implementation_step()]));
    assert_eq!(stale_apply.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&stale_apply.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "revision_conflict");
    assert_eq!(report["failures"][0]["expected"], 1);
    assert_eq!(report["failures"][0]["actual"], 2);

    let stale_clear = mire(&["walkthrough", "clear", fixture.review_str(), "--revision", "1"]);
    assert_eq!(stale_clear.status.code(), Some(8));
    let report: Value = serde_json::from_slice(&stale_clear.stderr).unwrap();
    assert_eq!(report["failures"][0]["code"], "revision_conflict");
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);
}

#[test]
fn clear_removes_the_walkthrough_and_context_reports_it() {
    let fixture = Fixture::new();
    let context = fixture.context();
    assert_eq!(context["payload"]["has_walkthrough"], false);
    assert_eq!(context["review_revision"], 1);

    assert_success(&fixture.apply(&batch(1, vec![contract_step()])));
    let context = fixture.context();
    assert_eq!(context["payload"]["has_walkthrough"], true);
    assert_eq!(context["review_revision"], 2);

    let cleared = mire(&["walkthrough", "clear", fixture.review_str(), "--revision", "2"]);
    assert_success(&cleared);
    let result: Value = serde_json::from_slice(&cleared.stdout).unwrap();
    assert_eq!(result["status"], "cleared");
    assert_eq!(result["review_revision"], 3);
    let review = read_review(&fixture.review_path).unwrap();
    assert!(review.walkthrough().is_none());
    assert_eq!(fixture.context()["payload"]["has_walkthrough"], false);

    let before = fs::read(&fixture.review_path).unwrap();
    let again = mire(&["walkthrough", "clear", fixture.review_str(), "--revision", "3"]);
    assert_success(&again);
    let result: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(result["status"], "unchanged");
    assert_eq!(result["review_revision"], 3);
    assert_eq!(fs::read(&fixture.review_path).unwrap(), before);
}

#[test]
fn show_prints_steps_anchor_states_and_coverage_as_json() {
    let fixture = Fixture::new();
    let empty = mire(&["walkthrough", "show", fixture.review_str(), "--format", "json"]);
    assert_success(&empty);
    let document: Value = serde_json::from_slice(&empty.stdout).unwrap();
    assert_eq!(document["walkthrough"], Value::Null);
    assert_eq!(document["coverage"], Value::Null);

    assert_success(&fixture.apply(&batch(1, vec![contract_step(), implementation_step()])));
    let shown = mire(&["walkthrough", "show", fixture.review_str(), "--format", "json"]);
    assert_success(&shown);
    let document: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(document["review_revision"], 2);
    let walkthrough = &document["walkthrough"];
    assert_eq!(walkthrough["overview"], "Swap the value.");
    assert_eq!(walkthrough["author"]["id"], "review-agent");
    assert_eq!(walkthrough["steps"][0]["id"], "contract");
    assert_eq!(walkthrough["steps"][1]["id"], "step-2");
    assert_eq!(walkthrough["steps"][1]["depends_on"], json!(["contract"]));
    let anchor = &walkthrough["steps"][0]["anchors"][0];
    assert_eq!(anchor["state"], "original");
    assert_eq!(anchor["anchor"]["side"], "new");
    assert_eq!(anchor["current_anchor"], anchor["anchor"]);
    assert_eq!(anchor["reanchor_outcome"], Value::Null);

    let uncovered_hunks = document["coverage"]["uncovered_hunks"].as_array().unwrap();
    assert_eq!(uncovered_hunks.len(), 1);
    assert_eq!(uncovered_hunks[0]["new_path"], json!(b"src/other.rs".to_vec()));
    let uncovered_files = document["coverage"]["uncovered_files"].as_array().unwrap();
    assert_eq!(uncovered_files.len(), 1);
    assert_eq!(uncovered_files[0]["new_path"], json!(b"run.sh".to_vec()));
}

#[test]
fn show_prints_plain_text() {
    let fixture = Fixture::new();
    let empty = mire(&["walkthrough", "show", fixture.review_str()]);
    assert_success(&empty);
    assert_eq!(
        String::from_utf8(empty.stdout).unwrap(),
        "review revision: 1\nwalkthrough: none\n"
    );

    assert_success(&fixture.apply(&batch(1, vec![contract_step(), implementation_step()])));
    let shown = mire(&["walkthrough", "show", fixture.review_str()]);
    assert_success(&shown);
    assert_eq!(
        String::from_utf8(shown.stdout).unwrap(),
        "review revision: 2
walkthrough: 2 steps by review-agent, agent (test-agent)

overview:
  Swap the value.

1. Contract [contract]
   src/lib.rs, new line 2 (original)

   Why Contract changed.

2. Implementation [step-2]
   depends on: contract
   src/lib.rs, old line 2 (original)

   Why Implementation changed.

   diagram:
     flowchart LR
       old --> new

uncovered hunks:
  src/other.rs @@ -1,1 +1,1 @@

files with no hunks:
  run.sh (modified)
"
    );
}

#[test]
fn refresh_reports_anchor_states_in_show() {
    let directory = temp_directory("refresh");
    let root = directory.join("repository");
    let review = directory.join("review.json");
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
    fs::write(root.join("file.txt"), b"bug\n").unwrap();
    let review_str = review.to_str().unwrap();
    assert_success(&mire_in(&root, &["review", "init", review_str]));

    let input = json!({
        "schema_version": { "major": 1, "minor": 4 },
        "review_revision": 1,
        "author": { "id": "review-agent", "display_name": null },
        "provenance": { "kind": "agent", "producer": "test-agent" },
        "steps": [step(None, "Fix", json!({ "path": "file.txt", "side": "new", "start": 1 }))]
    });
    assert_success(&mire_with_stdin(
        &root,
        &["walkthrough", "apply", review_str, "--stdin"],
        &serde_json::to_vec(&input).unwrap(),
    ));

    fs::write(root.join("file.txt"), b"inserted\nbug\n").unwrap();
    assert_success(&mire_in(&root, &["review", "refresh", review_str]));
    let shown = mire_in(&root, &["walkthrough", "show", review_str, "--format", "json"]);
    assert_success(&shown);
    let document: Value = serde_json::from_slice(&shown.stdout).unwrap();
    let anchor = &document["walkthrough"]["steps"][0]["anchors"][0];
    assert_eq!(anchor["state"], "moved");
    assert_eq!(anchor["current_anchor"]["range"]["start"], 2);
    assert_eq!(anchor["reanchor_outcome"]["kind"], "moved");

    fs::write(root.join("file.txt"), b"different\n").unwrap();
    assert_success(&mire_in(&root, &["review", "refresh", review_str]));
    let shown = mire_in(&root, &["walkthrough", "show", review_str]);
    assert_success(&shown);
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("file.txt, new line 1 (stale)"), "{text}");
    assert!(text.contains("uncovered hunks:\n  file.txt"), "{text}");

    fs::remove_dir_all(directory).unwrap();
}

fn batch(revision: u64, steps: Vec<Value>) -> Value {
    json!({
        "schema_version": { "major": 1, "minor": 4 },
        "review_revision": revision,
        "overview": "Swap the value.",
        "author": { "id": "review-agent", "display_name": null },
        "provenance": { "kind": "agent", "producer": "test-agent" },
        "steps": steps
    })
}

fn contract_step() -> Value {
    step(
        Some("contract"),
        "Contract",
        json!({ "file": "src/lib.rs", "new_line": 2 }),
    )
}

fn implementation_step() -> Value {
    let mut step = step(
        None,
        "Implementation",
        json!({ "file": "src/lib.rs", "old_line": 2, "end": 2 }),
    );
    step["depends_on"] = json!(["contract"]);
    step["diagram"] = json!("flowchart LR\n  old --> new");
    step
}

fn step(id: Option<&str>, title: &str, location: Value) -> Value {
    let mut step = json!({
        "title": title,
        "narrative": format!("Why {title} changed."),
        "locations": [location]
    });
    if let Some(id) = id {
        step["id"] = json!(id);
    }
    step
}

struct Fixture {
    directory: PathBuf,
    review_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = temp_directory("patch");
        let review_path = directory.join("review.json");
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        write_review_atomic(&review_path, &review).unwrap();
        Self { directory, review_path }
    }

    fn review_str(&self) -> &str {
        self.review_path.to_str().unwrap()
    }

    fn apply(&self, batch: &Value) -> Output {
        mire_with_stdin(
            &self.directory,
            &["walkthrough", "apply", self.review_str(), "--stdin"],
            &serde_json::to_vec(batch).unwrap(),
        )
    }

    fn context(&self) -> Value {
        let output = mire(&["context", self.review_str()]);
        assert_success(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn temp_directory(kind: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!("mire-walkthrough-{kind}-{}-{id}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn mire(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mire")).args(args).output().unwrap()
}

fn mire_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap()
}

fn mire_with_stdin(directory: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn git(directory: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "mire failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
