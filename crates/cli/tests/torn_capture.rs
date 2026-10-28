use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use mire::{load_bound_diff, read_review};
use mire_core::{FileContent, LineKind, ObjectId, blob_matches, rebuild_new_side};

/// Captures taken while the file changes. Without the check, more than one
/// in ten was torn.
const CAPTURES: usize = 200;

struct Fixture {
    directory: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn a_file_replaced_during_capture_never_yields_a_torn_capture() {
    let directory = std::env::temp_dir().join(format!("mire-torn-capture-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    let fixture = Fixture { directory };
    let root = fixture.directory.join("repository");
    let review = fixture.directory.join("review.json");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--quiet"]);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    git(&root, &["add", "file.txt"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Mire",
            "-c",
            "user.email=mire@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "base",
        ],
    );

    let alpha = "alpha\n".repeat(200);
    let beta = "beta\n".repeat(200);
    fs::write(root.join("file.txt"), &alpha).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(["review", "init"])
        .arg(&review)
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let binding = read_review(&review).unwrap().source_binding().unwrap().clone();
    let alpha_id = hash_object(&root, &alpha);
    let beta_id = hash_object(&root, &beta);

    // Replace the file by rename, so each read sees one whole content.
    let stop = Arc::new(AtomicBool::new(false));
    let racer = {
        let stop = Arc::clone(&stop);
        let root = root.clone();
        thread::spawn(move || {
            let staging = root.join(".git").join("staging");
            while !stop.load(Ordering::Relaxed) {
                for content in [&alpha, &beta] {
                    fs::write(&staging, content).unwrap();
                    fs::rename(&staging, root.join("file.txt")).unwrap();
                }
            }
        })
    };

    let (mut torn, mut binary, mut cleared) = (0, 0, 0);
    for _ in 0..CAPTURES {
        let changeset = load_bound_diff(&binding).unwrap();
        let file = &changeset.files()[0];
        let object = file.new_side().unwrap().object.as_ref();
        // A read cut short pads the file with zeros, and Git sees it as
        // binary. Mire cannot check a binary file, so it keeps no id.
        let FileContent::Text { hunks } = file.content() else {
            binary += 1;
            assert_eq!(object, None, "a torn binary capture kept its blob id");
            continue;
        };
        let Some(object) = object else {
            cleared += 1;
            continue;
        };
        let shows_alpha = hunks
            .iter()
            .flat_map(|hunk| hunk.lines())
            .any(|line| line.kind() == LineKind::Addition && line.content() == b"alpha");
        let expected = if shows_alpha { &alpha_id } else { &beta_id };
        if object != expected {
            torn += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    let kept = CAPTURES - binary - cleared;
    eprintln!("{kept} captures kept their id, {cleared} text and {binary} binary captures did not");
    assert_eq!(torn, 0, "{torn} of {kept} stored blob ids were torn");
    assert!(kept > CAPTURES / 2, "only {kept} of {CAPTURES} captures kept their id");
}

#[test]
fn a_file_appended_to_during_capture_always_captures() {
    let directory = std::env::temp_dir().join(format!("mire-append-capture-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    let fixture = Fixture { directory };
    let root = fixture.directory.join("repository");
    let review = fixture.directory.join("review.json");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--quiet"]);
    let base = "line\n".repeat(100);
    fs::write(root.join("app.log"), &base).unwrap();
    git(&root, &["add", "app.log"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Mire",
            "-c",
            "user.email=mire@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "base",
        ],
    );
    fs::write(root.join("app.log"), format!("{base}first\n")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(["review", "init"])
        .arg(&review)
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let binding = read_review(&review).unwrap().source_binding().unwrap().clone();

    // Append in small writes, as a logger does.
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let stop = Arc::clone(&stop);
        let path = root.join("app.log");
        thread::spawn(move || {
            let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
            let mut line = 0u64;
            while !stop.load(Ordering::Relaxed) {
                writeln!(file, "entry {line}").unwrap();
                line += 1;
                thread::sleep(Duration::from_millis(1));
            }
        })
    };

    let mut kept = 0;
    for _ in 0..CAPTURES / 5 {
        let changeset = load_bound_diff(&binding).unwrap();
        let file = &changeset.files()[0];
        let Some(object) = file.new_side().unwrap().object.as_ref() else {
            continue;
        };
        let FileContent::Text { hunks } = file.content() else {
            panic!("a binary capture kept its blob id");
        };
        let rebuilt = rebuild_new_side(base.as_bytes(), hunks).unwrap();
        assert!(
            blob_matches(&rebuilt, object),
            "a stored blob id does not match its hunks"
        );
        kept += 1;
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    eprintln!("{kept} of {} captures kept their id", CAPTURES / 5);
}

fn hash_object(root: &Path, content: &str) -> ObjectId {
    let path = root.join(".git").join("hashed");
    fs::write(&path, content).unwrap();
    let output = Command::new("git")
        .arg("hash-object")
        .arg(&path)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    ObjectId::new(String::from_utf8(output.stdout).unwrap().trim()).unwrap()
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git").args(arguments).current_dir(root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
