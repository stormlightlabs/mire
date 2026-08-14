use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use mire::read_review;
use mire_core::{NoteStatus, ReanchorOutcome, Review, ReviewNote};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde_json::{Value, json};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const INITIAL_FILES: [(&str, &str); 5] = [
    ("exact.txt", "exact-value\n"),
    ("moved.txt", "move-value\n"),
    ("stale.txt", "stale-value\n"),
    ("ambiguous.txt", "duplicate-value\n"),
    ("later.txt", "later-value\n"),
];

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
    review: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-review-loop-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        let review = directory.join("review.json");
        fs::create_dir_all(&root).expect("fixture repository directory can be created");
        git(&root, ["init", "--quiet"]);
        git(&root, ["config", "user.name", "Mire Tests"]);
        git(&root, ["config", "user.email", "mire@example.invalid"]);
        for (path, _) in INITIAL_FILES {
            fs::write(root.join(path), b"old\n").expect("fixture source can be written");
        }
        git(&root, ["add", "."]);
        git(&root, ["commit", "--quiet", "-m", "base"]);
        for (path, contents) in INITIAL_FILES {
            fs::write(root.join(path), contents).expect("initial diff can be written");
        }
        Self { directory, root, review }
    }

    fn first_source_edit(&self) {
        fs::write(self.root.join("moved.txt"), b"padding-one\nmove-value\n").expect("moved source can be written");
        fs::write(self.root.join("stale.txt"), b"replacement-value\n").expect("stale source can be written");
        fs::write(self.root.join("ambiguous.txt"), b"duplicate-value\nduplicate-value\n")
            .expect("ambiguous source can be written");
    }

    fn second_source_edit(&self) {
        fs::write(self.root.join("later.txt"), b"later-value-v2\n").expect("later source can be written");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn complete_review_loop_preserves_history_and_reanchor_results_in_exports() {
    let fixture = Fixture::new();
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("init"),
            fixture.review.as_os_str().to_owned(),
        ],
    ));

    let manifest = json_output(run_mire(
        &fixture.root,
        [OsString::from("context"), fixture.review.as_os_str().to_owned()],
    ));
    assert_eq!(manifest["review_revision"], 1);
    assert_eq!(manifest["payload"]["kind"], "manifest");
    assert_eq!(
        manifest["payload"]["files"].as_array().unwrap().len(),
        INITIAL_FILES.len()
    );

    let file_context = json_output(run_mire(
        &fixture.root,
        [
            OsString::from("context"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("exact.txt"),
            OsString::from("--max-bytes"),
            OsString::from("4096"),
        ],
    ));
    assert_eq!(file_context["payload"]["kind"], "file");

    assert_success(&run_mire_with_stdin(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("apply"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--stdin"),
        ],
        &serde_json::to_vec(&json!({
            "schema_version": { "major": 1, "minor": 1 },
            "review_revision": 1,
            "notes": [
                request("exact.txt", "The exact finding remains in place."),
                request("moved.txt", "The moved finding retains its selected content."),
                request("stale.txt", "The stale finding's selected content changed."),
                request("ambiguous.txt", "The ambiguous finding has duplicate content.")
            ]
        }))
        .unwrap(),
    ));
    open_watched_review(&fixture.root, &fixture.review);

    fixture.first_source_edit();
    refresh(&fixture);
    let review = read_review(&fixture.review).expect("refreshed review can be read");
    assert_eq!(review.revision().get(), 3);
    assert_outcomes(&review);
    assert_exported_outcomes(&fixture);

    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("note"),
            OsString::from("resolve"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("note-1"),
            OsString::from("--revision"),
            OsString::from("3"),
            OsString::from("--author"),
            OsString::from("reviewer"),
        ],
    ));

    fixture.second_source_edit();
    refresh(&fixture);
    let review = read_review(&fixture.review).expect("second refreshed review can be read");
    assert_eq!(review.revision().get(), 5);
    assert_refreshed_statuses(&review);
    assert_eq!(note(&review, "note-1").status(), NoteStatus::Resolved);

    let context = json_output(run_mire(
        &fixture.root,
        [OsString::from("context"), fixture.review.as_os_str().to_owned()],
    ));
    assert_eq!(context["review_revision"], 5);
    assert_success(&run_mire_with_stdin(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("apply"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--stdin"),
        ],
        &serde_json::to_vec(&json!({
            "schema_version": { "major": 1, "minor": 1 },
            "review_revision": 5,
            "notes": [request("later.txt", "The follow-up finding was added after refresh.")]
        }))
        .unwrap(),
    ));

    let final_review = read_review(&fixture.review).expect("final review can be read");
    assert_eq!(final_review.revision().get(), 6);
    assert_eq!(note(&final_review, "note-1").status(), NoteStatus::Resolved);
    assert_eq!(final_review.events().len(), 6);
    assert_refreshed_statuses(&final_review);

    let exported_json = json_output(run_mire(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("json"),
        ],
    ));
    assert_eq!(exported_json["review_revision"], 6);
    assert_eq!(exported_json["notes"][0]["author"]["id"], "review-agent");
    assert_eq!(exported_json["notes"][0]["provenance"]["kind"], "agent");
    assert_eq!(exported_json["notes"][1]["reanchor_outcome"]["kind"], "exact");
    assert!(exported_json["notes"][1]["reanchor_outcome"]["original_anchor"].is_object());
    assert_eq!(exported_json["events"].as_array().unwrap().len(), 6);
    assert_eq!(exported_json["events"][4]["author"]["id"], "reviewer");

    let markdown = run_mire(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("markdown"),
        ],
    );
    assert_success(&markdown);
    let markdown = String::from_utf8(markdown.stdout).expect("Markdown export is UTF-8");
    assert!(markdown.contains("- Original location:"));
    assert!(markdown.contains("- Re-anchor: `exact`"));
    assert!(markdown.contains("- Re-anchor: `stale`"));
    assert!(markdown.contains("- Events:"));
    assert!(markdown.contains("reviewer changed the status from `open` to `resolved`"));
}

fn request(file: &str, body: &str) -> Value {
    json!({
        "file": file,
        "new_line": 1,
        "author": { "id": "review-agent", "display_name": "Review agent" },
        "provenance": { "kind": "agent", "producer": "mire-review" },
        "severity": "high",
        "kind": "defect",
        "body": body
    })
}

fn refresh(fixture: &Fixture) {
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("refresh"),
            fixture.review.as_os_str().to_owned(),
        ],
    ));
}

fn assert_outcomes(review: &Review) {
    for identifier in ["note-1", "note-2", "note-3", "note-4"] {
        let note = note(review, identifier);
        assert_eq!(note.reanchor_outcome().unwrap().original_anchor(), note.anchor());
    }
    assert!(matches!(
        note(review, "note-1").reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));
    assert!(matches!(
        note(review, "note-2").reanchor_outcome(),
        Some(ReanchorOutcome::Moved { .. })
    ));
    assert!(matches!(
        note(review, "note-3").reanchor_outcome(),
        Some(ReanchorOutcome::Stale { .. })
    ));
    let ambiguous = note(review, "note-4").reanchor_outcome();
    assert!(
        matches!(ambiguous, Some(ReanchorOutcome::Ambiguous { candidates, .. }) if candidates.len() == 2),
        "unexpected ambiguous result: {ambiguous:?}"
    );
}

fn assert_exported_outcomes(fixture: &Fixture) {
    let exported = json_output(run_mire(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("json"),
        ],
    ));
    let outcomes = exported["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["reanchor_outcome"]["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes, ["exact", "moved", "stale", "ambiguous"]);

    let markdown = run_mire(
        &fixture.root,
        [
            OsString::from("notes"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("markdown"),
        ],
    );
    assert_success(&markdown);
    let markdown = String::from_utf8(markdown.stdout).expect("Markdown export is UTF-8");
    assert!(markdown.contains("- Re-anchor: `exact`"));
    assert!(markdown.contains("- Re-anchor: `moved`"));
    assert!(markdown.contains("- Re-anchor: `stale`"));
    assert!(markdown.contains("- Re-anchor: `ambiguous`"));
}

fn assert_refreshed_statuses(review: &Review) {
    for identifier in ["note-1", "note-2", "note-3", "note-4"] {
        let note = note(review, identifier);
        assert_eq!(note.reanchor_outcome().unwrap().original_anchor(), note.anchor());
    }
    assert!(matches!(
        note(review, "note-1").reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));
    assert!(matches!(
        note(review, "note-2").reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));
    assert!(matches!(
        note(review, "note-3").reanchor_outcome(),
        Some(ReanchorOutcome::Stale { .. })
    ));
    assert!(matches!(
        note(review, "note-4").reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));
}

fn note<'a>(review: &'a Review, identifier: &str) -> &'a ReviewNote {
    review
        .notes()
        .iter()
        .find(|note| note.id().as_str() == identifier)
        .expect("note exists")
}

fn open_watched_review(root: &Path, review: &Path) {
    let pair = native_pty_system()
        .openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("a pseudo-terminal can be allocated");
    let mut command = CommandBuilder::new(binary());
    command.args(["review", review.to_str().expect("fixture path is UTF-8"), "--watch"]);
    command.cwd(root);
    command.env("NO_COLOR", "1");
    let mut output = pair.master.try_clone_reader().expect("PTY output can be read");
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).expect("PTY output can be captured");
        bytes
    });
    let mut writer = pair.master.take_writer().expect("PTY input can be written");
    let mut child = pair.slave.spawn_command(command).expect("Mire can start in the PTY");
    drop(pair.slave);
    thread::sleep(Duration::from_millis(600));
    writer.write_all(b"q").expect("quit key can be sent");
    writer.flush().expect("quit key can be flushed");
    let status = child.wait().expect("watched review exits");
    drop(writer);
    drop(pair.master);
    let output = reader.join().expect("PTY reader does not panic");
    assert!(
        status.success(),
        "watched review failed: {}",
        String::from_utf8_lossy(&output)
    );
}

fn run_mire(path: &Path, arguments: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(binary())
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("mire runs")
}

fn run_mire_with_stdin(path: &Path, arguments: impl IntoIterator<Item = OsString>, input: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .args(arguments)
        .current_dir(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mire starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .expect("batch can be written");
    child.wait_with_output().expect("mire exits")
}

fn git<I, S>(path: &Path, arguments: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("Git is available");
    assert!(
        output.status.success(),
        "Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_output(output: Output) -> Value {
    assert_success(&output);
    serde_json::from_slice(&output.stdout).expect("Mire output is JSON")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "mire failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
