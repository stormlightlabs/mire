use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use mire::{read_review, write_review_atomic};
use mire_core::{ChangesetSource, NoteStatus, PatchLimits, Review, ReviewRevision, parse_patch};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const PATCH: &[u8] = b"--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n";

#[test]
fn pty_notes_create_disposition_save_and_reload_without_touching_source_files() {
    let directory = test_directory();
    let review_path = directory.join("review.json");
    let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
    let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
    write_review_atomic(&review_path, &review).unwrap();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: 14, cols: 120, pixel_width: 0, pixel_height: 0 })
        .expect("a pseudo-terminal can be allocated");
    let mut command = CommandBuilder::new(binary());
    command.arg("review");
    command.arg(&review_path);
    command.env("NO_COLOR", "1");
    command.env("USER", "pty-reviewer");

    let mut reader = pair.master.try_clone_reader().expect("PTY output can be read");
    let reader_thread = thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output).expect("PTY output can be captured");
        output
    });
    let mut writer = pair.master.take_writer().expect("PTY input can be written");
    let mut child = pair.slave.spawn_command(command).expect("Mire can start in the PTY");
    drop(pair.slave);

    thread::sleep(Duration::from_secs(1));
    writer.write_all(b"jjcNeeds a guard\r").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(200));
    writer.write_all(b"pr").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(150));
    writer.write_all(b"\x1bq").unwrap();
    writer.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().expect("terminal review state can be inspected") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stuck terminal review can be stopped");
            break child.wait().expect("stopped terminal review exits");
        }
        thread::sleep(Duration::from_millis(25));
    };
    drop(writer);
    drop(pair.master);
    let output = reader_thread.join().expect("PTY reader does not panic");

    assert!(
        status.success(),
        "PTY review failed: {}",
        String::from_utf8_lossy(&output)
    );
    let saved = read_review(&review_path).expect("saved review reloads");
    assert_eq!(saved.revision().get(), 3);
    assert_eq!(saved.notes().len(), 1);
    assert_eq!(saved.notes()[0].body(), "Needs a guard");
    assert_eq!(saved.notes()[0].status(), NoteStatus::Resolved);
    assert_eq!(saved.notes()[0].author().id(), "pty-reviewer");

    let normalized = Command::new(binary())
        .arg("review")
        .arg(&review_path)
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(normalized.status.success());
    let reloaded: Review = serde_json::from_slice(&normalized.stdout).unwrap();
    assert_eq!(reloaded, saved);
    fs::remove_dir_all(directory).unwrap();
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}

fn test_directory() -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mire-pty-notes-{}-{id}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}
