//! End-to-end tests for `--watch` sessions in a pseudo-terminal.
//!
//! Each test waits for what the terminal shows, or for the review revision on
//! disk, with a deadline, and fails with the captured output instead of
//! hanging when the session does not get there.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// How long a test waits for the session to show a state or to exit.
const DEADLINE: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

const INITIAL_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+initial-watch-value\n";
const EDITED_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+zzzzzzzzzzzzzzzzzzzz\n";
const RECOVERED_PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+recovered-watch-value\n";

struct TempDirectory(PathBuf);

/// A Mire session in a pseudo-terminal whose output is collected as it arrives.
struct WatchProcess {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: thread::JoinHandle<()>,
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
        let mut source = pair.master.try_clone_reader().expect("PTY output can be read");
        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        let reader = thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            // The read fails once the session exits and its side of the PTY closes.
            while let Ok(read @ 1..) = source.read(&mut buffer) {
                sink.lock().expect("PTY output lock").extend_from_slice(&buffer[..read]);
            }
        });
        let writer = pair.master.take_writer().expect("PTY input can be written");
        let child = pair.slave.spawn_command(command).expect("Mire can start in the PTY");
        drop(pair.slave);
        Self { child, master: pair.master, output, reader, writer }
    }

    /// Waits until the terminal has shown `needle`, failing with the output
    /// after [`DEADLINE`].
    fn wait_for(&mut self, needle: &[u8]) {
        let deadline = Instant::now() + DEADLINE;
        while !contains(&self.output(), needle) {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!(
                    "session exited ({status:?}) before showing {:?}:\n{}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&self.output())
                );
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {:?}:\n{}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(&self.output())
            );
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn output(&self) -> Vec<u8> {
        self.output.lock().expect("PTY output lock").clone()
    }

    /// Sends `q` and waits for the session to exit, killing it and failing
    /// with the output when it outlives [`DEADLINE`].
    fn finish(mut self) -> (bool, Vec<u8>) {
        self.writer.write_all(b"q").expect("the quit key can be sent");
        self.writer.flush().expect("the quit key can be flushed");
        let deadline = Instant::now() + DEADLINE;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("the session status can be read") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!(
                    "session did not exit after `q`:\n{}",
                    String::from_utf8_lossy(&self.output())
                );
            }
            thread::sleep(POLL_INTERVAL);
        };
        drop(self.writer);
        drop(self.master);
        self.reader.join().expect("PTY reader does not panic");
        let output = self.output.lock().expect("PTY output lock").clone();
        (status.success(), output)
    }
}

#[test]
fn patch_watch_reloads_and_recovers_after_rename_delete_and_recreate() {
    let directory = TempDirectory::new();
    let patch = directory.0.join("review.patch");
    let moved = directory.0.join("moved.patch");
    fs::write(&patch, INITIAL_PATCH).expect("initial patch can be written");

    let mut process = WatchProcess::spawn(&directory.0, &["patch", "review.patch", "--watch"]);
    process.wait_for(b"initial-watch-value");
    fs::write(&patch, EDITED_PATCH).expect("patch can be edited");
    process.wait_for(b"zzzzzzzzzzzzzzzzzzzz");
    fs::rename(&patch, &moved).expect("patch can be renamed away");
    // The status line changes from "refreshed" to "refresh failed", and the
    // terminal is sent only the cells that changed.
    process.wait_for(b"failed");
    fs::remove_file(&moved).expect("renamed patch can be deleted");
    fs::write(&patch, RECOVERED_PATCH).expect("patch can be recreated");
    process.wait_for(b"recovered-watch-value");

    let (success, output) = process.finish();

    assert!(success, "watch process failed: {}", String::from_utf8_lossy(&output));
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

    let mut process = WatchProcess::spawn(&directory.0, &["review", "review.json", "--watch"]);
    process.wait_for(b"initial-review-value");
    fs::write(directory.0.join("tracked.txt"), b"refreshed-review-value\n").expect("source can be edited");
    process.wait_for(b"refreshed-review-value");
    let review = mire::read_review(directory.0.join("review.json")).expect("watched review remains valid");
    let (success, output) = process.finish();

    assert!(
        success,
        "review watch process failed: {}",
        String::from_utf8_lossy(&output)
    );
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

    let mut process = WatchProcess::spawn(&directory.0, &["watch"]);
    process.wait_for(b"initial-git-watch");
    fs::write(directory.0.join("tracked.txt"), b"reloaded-git-watch\n").expect("worktree can be edited");
    process.wait_for(b"reloaded-git-watch");
    let (success, output) = process.finish();

    assert!(
        success,
        "Git watch process failed: {}",
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
