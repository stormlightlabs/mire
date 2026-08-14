use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::read_review;
use mire_core::{NoteStatus, ReanchorOutcome};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
    review: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-review-refresh-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        let review = directory.join("review.json");
        fs::create_dir_all(&root).unwrap();
        git(&root, ["init", "--quiet"]);
        git(&root, ["config", "user.name", "Mire Tests"]);
        git(&root, ["config", "user.email", "mire@example.invalid"]);
        fs::write(root.join("file.txt"), b"old\n").unwrap();
        git(&root, ["add", "file.txt"]);
        git(&root, ["commit", "--quiet", "-m", "base"]);
        fs::write(root.join("file.txt"), b"bug\n").unwrap();

        assert_success(&run_mire(
            &root,
            [
                OsString::from("review"),
                OsString::from("init"),
                review.as_os_str().to_owned(),
            ],
        ));
        assert_success(&run_mire(
            &root,
            [
                OsString::from("note"),
                OsString::from("add"),
                review.as_os_str().to_owned(),
                OsString::from("--revision"),
                OsString::from("1"),
                OsString::from("--file"),
                OsString::from("file.txt"),
                OsString::from("--new-line"),
                OsString::from("1"),
                OsString::from("--author"),
                OsString::from("agent"),
                OsString::from("--provenance"),
                OsString::from("agent"),
                OsString::from("--producer"),
                OsString::from("fixture"),
                OsString::from("--severity"),
                OsString::from("high"),
                OsString::from("--kind"),
                OsString::from("defect"),
                OsString::from("--body"),
                OsString::from("concrete evidence"),
            ],
        ));
        Self { directory, root, review }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn refresh_reanchors_notes_and_does_not_rewrite_unchanged_captures() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.review).unwrap();
    let unchanged = run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("refresh"),
            fixture.review.as_os_str().to_owned(),
        ],
    );
    assert_success(&unchanged);
    assert!(String::from_utf8_lossy(&unchanged.stdout).contains("status: unchanged"));
    assert_eq!(fs::read(&fixture.review).unwrap(), before);

    fs::write(fixture.root.join("file.txt"), b"inserted\nbug\n").unwrap();
    let refreshed = run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("refresh"),
            fixture.review.as_os_str().to_owned(),
        ],
    );
    assert_success(&refreshed);
    let review = read_review(&fixture.review).unwrap();
    assert_eq!(review.revision().get(), 3);
    let Some(ReanchorOutcome::Moved { candidate, .. }) = review.notes()[0].reanchor_outcome() else {
        panic!("the finding should move with its selected content");
    };
    assert_eq!(candidate.anchor().range().start().get(), 2);

    let context = run_mire(
        &fixture.root,
        [OsString::from("context"), fixture.review.as_os_str().to_owned()],
    );
    assert_success(&context);
    let document: serde_json::Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(document["review_revision"], 3);
    assert_eq!(document["payload"]["notes"][0]["reanchor_outcome"]["kind"], "moved");
}

#[test]
fn refresh_preserves_external_note_decisions_and_rejects_unbound_reviews() {
    let fixture = Fixture::new();
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("note"),
            OsString::from("resolve"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("note-1"),
            OsString::from("--revision"),
            OsString::from("2"),
            OsString::from("--author"),
            OsString::from("reviewer"),
        ],
    ));
    fs::write(fixture.root.join("file.txt"), b"inserted\nbug\n").unwrap();
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("refresh"),
            fixture.review.as_os_str().to_owned(),
        ],
    ));
    let review = read_review(&fixture.review).unwrap();
    assert_eq!(review.notes()[0].status(), NoteStatus::Resolved);
    assert_eq!(review.events().len(), 2);

    let unbound = fixture.directory.join("unbound.json");
    let value = serde_json::json!({
        "schema_version": { "major": 1, "minor": 0 },
        "revision": 1,
        "changeset": review.changeset(),
        "notes": [],
        "events": []
    });
    fs::write(&unbound, serde_json::to_vec(&value).unwrap()).unwrap();
    let output = run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("refresh"),
            unbound.into_os_string(),
        ],
    );
    assert_eq!(output.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no reloadable source binding"));
}

fn run_mire(path: &Path, arguments: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(binary())
        .args(arguments)
        .current_dir(path)
        .output()
        .unwrap()
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

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
