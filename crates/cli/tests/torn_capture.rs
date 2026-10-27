use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use mire::{load_bound_diff, read_review};
use mire_core::{FileContent, LineKind, ObjectId};

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

    let (mut torn, mut binary, mut failed) = (0, 0, 0);
    for _ in 0..CAPTURES {
        // Git can fail with a short read when the file shrinks under it.
        let Ok(changeset) = load_bound_diff(&binding) else {
            failed += 1;
            continue;
        };
        let file = &changeset.files()[0];
        // A read cut short pads the file with zeros, and Git sees it as
        // binary. Mire keeps no binary payload, so it cannot check one.
        let FileContent::Text { hunks } = file.content() else {
            binary += 1;
            continue;
        };
        let shows_alpha = hunks
            .iter()
            .flat_map(|hunk| hunk.lines())
            .any(|line| line.kind() == LineKind::Addition && line.content() == b"alpha");
        let object = file.new_side().unwrap().object.as_ref().unwrap();
        let expected = if shows_alpha { &alpha_id } else { &beta_id };
        if object != expected {
            torn += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    let checked = CAPTURES - binary - failed;
    eprintln!("{checked} text captures, {binary} binary, {failed} failed");
    assert_eq!(torn, 0, "{torn} of {checked} text captures were torn");
    assert!(
        checked > CAPTURES / 2,
        "only {checked} of {CAPTURES} captures were text"
    );
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
