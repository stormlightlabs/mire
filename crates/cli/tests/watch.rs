use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

const INITIAL_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+initial-watch-value\n";
const EDITED_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+zzzzzzzzzzzzzzzzzzzz\n";
const RECOVERED_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+recovered-watch-value\n";

struct TempDirectory(PathBuf);

struct WatchProcess {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    reader: thread::JoinHandle<Vec<u8>>,
    writer: Box<dyn Write + Send>,
}

impl TempDirectory {
    fn new() -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mire-watch-{}-{id}", std::process::id()));
        fs::create_dir(&path).expect("watch fixture directory can be created");
        Self(path)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl WatchProcess {
    fn spawn(directory: &Path, arguments: &[&str]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 })
            .expect("a pseudo-terminal can be allocated");
        let mut command = CommandBuilder::new(binary());
        command.args(arguments);
        command.cwd(directory);
        command.env("NO_COLOR", "1");
        let mut output = pair.master.try_clone_reader().expect("PTY output can be read");
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            output.read_to_end(&mut bytes).expect("PTY output can be captured");
            bytes
        });
        let writer = pair.master.take_writer().expect("PTY input can be written");
        let child = pair.slave.spawn_command(command).expect("Mire can start in the PTY");
        drop(pair.slave);
        Self { child, master: pair.master, reader, writer }
    }

    fn finish(mut self) -> (bool, Vec<u8>) {
        let _ = self.writer.write_all(b"q");
        let _ = self.writer.flush();
        let status = self.child.wait().expect("watch process exits");
        drop(self.writer);
        drop(self.master);
        let output = self.reader.join().expect("PTY reader does not panic");
        (status.success(), output)
    }
}

#[test]
fn patch_watch_reloads_and_recovers_after_rename_delete_and_recreate() {
    let directory = TempDirectory::new();
    let patch = directory.0.join("review.patch");
    let moved = directory.0.join("moved.patch");
    fs::write(&patch, INITIAL_PATCH).expect("initial patch can be written");

    let process = WatchProcess::spawn(&directory.0, &["patch", "review.patch", "--watch"]);
    thread::sleep(Duration::from_millis(600));
    fs::write(&patch, EDITED_PATCH).expect("patch can be edited");
    thread::sleep(Duration::from_millis(2_500));
    fs::rename(&patch, &moved).expect("patch can be renamed away");
    thread::sleep(Duration::from_millis(2_500));
    fs::remove_file(&moved).expect("renamed patch can be deleted");
    fs::write(&patch, RECOVERED_PATCH).expect("patch can be recreated");
    thread::sleep(Duration::from_millis(2_500));

    let (success, output) = process.finish();

    assert!(success, "watch process failed");
    assert!(
        contains(&output, b"zzzzzzzzzzzzzzzzzzzz"),
        "{}",
        String::from_utf8_lossy(&output)
    );
    assert!(
        contains(&output, b"recovered-watch-value"),
        "{}",
        String::from_utf8_lossy(&output)
    );
    assert!(contains(&output, b"\x1b[?1049l"), "alternate screen was not restored");
}

#[test]
fn source_backed_review_watch_refreshes_the_review_file() {
    let directory = TempDirectory::new();
    git(&directory.0, ["init", "--quiet"]);
    git(&directory.0, ["config", "user.name", "Mire Tests"]);
    git(&directory.0, ["config", "user.email", "mire@example.invalid"]);
    fs::write(directory.0.join("tracked.txt"), b"base\n").expect("base file can be written");
    git(&directory.0, ["add", "tracked.txt"]);
    git(&directory.0, ["commit", "--quiet", "-m", "base"]);
    fs::write(directory.0.join("tracked.txt"), b"initial-review-value\n").expect("initial edit can be written");
    let initialized = Command::new(binary())
        .args(["review", "init", "review.json"])
        .current_dir(&directory.0)
        .output()
        .expect("review initialization runs");
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let process = WatchProcess::spawn(&directory.0, &["review", "review.json", "--watch"]);
    thread::sleep(Duration::from_millis(600));
    fs::write(directory.0.join("tracked.txt"), b"refreshed-review-value\n").expect("source can be edited");
    thread::sleep(Duration::from_millis(2_500));
    let (success, output) = process.finish();

    assert!(
        success,
        "review watch process failed: {}",
        String::from_utf8_lossy(&output)
    );
    assert!(
        contains(&output, b"refreshed-review-value"),
        "{}",
        String::from_utf8_lossy(&output)
    );
    let review = mire::read_review(directory.0.join("review.json")).expect("watched review remains valid");
    assert_eq!(review.revision().get(), 2);
}

#[test]
fn git_watch_reloads_a_changed_worktree() {
    let directory = TempDirectory::new();
    git(&directory.0, ["init", "--quiet"]);
    git(&directory.0, ["config", "user.name", "Mire Tests"]);
    git(&directory.0, ["config", "user.email", "mire@example.invalid"]);
    fs::write(directory.0.join("tracked.txt"), b"base\n").expect("base file can be written");
    git(&directory.0, ["add", "tracked.txt"]);
    git(&directory.0, ["commit", "--quiet", "-m", "base"]);
    fs::write(directory.0.join("tracked.txt"), b"initial-git-watch\n").expect("initial edit can be written");

    let process = WatchProcess::spawn(&directory.0, &["watch"]);
    thread::sleep(Duration::from_millis(600));
    fs::write(directory.0.join("tracked.txt"), b"reloaded-git-watch\n").expect("worktree can be edited");
    thread::sleep(Duration::from_millis(2_500));
    let (success, output) = process.finish();

    assert!(success, "Git watch process failed");
    assert!(
        contains(&output, b"reloaded-git-watch"),
        "{}",
        String::from_utf8_lossy(&output)
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn git<const N: usize>(path: &Path, arguments: [&str; N]) {
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

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}
