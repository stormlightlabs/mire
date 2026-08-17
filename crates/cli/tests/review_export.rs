use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::write_review_atomic;
use mire_core::{ChangesetSource, FileContent, PatchLimits, Review, ReviewRevision, parse_patch};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct FixtureRepository {
    directory: PathBuf,
    root: PathBuf,
    review: PathBuf,
}

impl FixtureRepository {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-review-export-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        let review = directory.join("review.json");
        fs::create_dir_all(&root).unwrap();
        git(&root, ["init", "--quiet"]);
        git(&root, ["config", "user.name", "Mire Tests"]);
        git(&root, ["config", "user.email", "mire@example.invalid"]);

        fs::write(root.join("modified.txt"), b"before\r\nlast\n").unwrap();
        fs::write(root.join("deleted.txt"), b"delete me\n").unwrap();
        fs::write(root.join("old name.txt"), b"rename me\n").unwrap();
        fs::write(root.join("script.sh"), b"#!/bin/sh\necho before\n").unwrap();
        fs::write(root.join("copy-source.txt"), b"copy source\n").unwrap();
        fs::write(root.join("empty-deleted.txt"), b"").unwrap();
        git(&root, ["add", "."]);
        git(&root, ["commit", "--quiet", "-m", "base"]);

        fs::write(root.join("modified.txt"), b"after\r\nlast").unwrap();
        fs::remove_file(root.join("deleted.txt")).unwrap();
        fs::rename(root.join("old name.txt"), root.join("new name.txt")).unwrap();
        fs::write(root.join("script.sh"), b"#!/bin/sh\necho after\n").unwrap();
        set_executable(&root.join("script.sh"));
        fs::write(root.join("added file.txt"), b"added\n").unwrap();
        fs::write(root.join("empty-added.txt"), b"").unwrap();
        fs::remove_file(root.join("empty-deleted.txt")).unwrap();
        git(&root, ["add", "--all"]);

        Self { directory, root, review }
    }
}

impl Drop for FixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn patch_export_round_trips_and_passes_git_apply_check() {
    let fixture = FixtureRepository::new();
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("init"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--staged"),
        ],
    ));

    let stdout = run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("patch"),
        ],
    );
    assert_success(&stdout);
    assert!(stdout.stdout.starts_with(b"diff --git "));
    assert!(!stdout.stdout.windows(64).any(|window| window == [b'0'; 64]));
    let reparsed = parse_patch(
        &stdout.stdout,
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("exported patch is accepted by Mire's parser");
    assert!(
        reparsed
            .files()
            .iter()
            .any(|file| matches!(file.content(), FileContent::Text { hunks } if hunks.is_empty()))
    );

    let output = fixture.directory.join("review.patch");
    fs::write(&output, b"previous output\n").unwrap();
    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("patch"),
            OsString::from("--output"),
            output.as_os_str().to_owned(),
        ],
    ));
    assert_eq!(fs::read(&output).unwrap(), stdout.stdout);

    git(&fixture.root, ["reset", "--hard", "--quiet"]);
    git(&fixture.root, ["clean", "--force", "--quiet"]);
    git(&fixture.root, ["apply", "--check", output.to_str().unwrap()]);
}

#[test]
fn copy_export_passes_git_apply_check() {
    let fixture = FixtureRepository::new();
    let changeset = parse_patch(
        b"diff --git a/copy-source.txt b/copy target.txt\nsimilarity index 100%\ncopy from copy-source.txt\ncopy to copy target.txt\n",
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .unwrap();
    let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
    let review_path = fixture.directory.join("copy-review.json");
    write_review_atomic(&review_path, &review).unwrap();
    let output = fixture.directory.join("copy.patch");

    assert_success(&run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("export"),
            review_path.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("patch"),
            OsString::from("--output"),
            output.as_os_str().to_owned(),
        ],
    ));
    git(&fixture.root, ["apply", "--check", output.to_str().unwrap()]);
}

#[test]
fn binary_exports_fail_without_writing_stdout_or_replacing_output() {
    let fixture = FixtureRepository::new();
    let changeset = parse_patch(
        include_bytes!("../../core/tests/fixtures/patches/git_metadata.patch"),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .unwrap();
    let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
    write_review_atomic(&fixture.review, &review).unwrap();
    let output = fixture.directory.join("review.patch");
    fs::write(&output, b"previous output\n").unwrap();

    let exported = run_mire(
        &fixture.root,
        [
            OsString::from("review"),
            OsString::from("export"),
            fixture.review.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("patch"),
            OsString::from("--output"),
            output.as_os_str().to_owned(),
        ],
    );

    assert_eq!(exported.status.code(), Some(5));
    assert!(exported.stdout.is_empty());
    assert!(String::from_utf8_lossy(&exported.stderr).contains("image.bin"));
    assert_eq!(fs::read(&output).unwrap(), b"previous output\n");
}

fn run_mire(path: &Path, arguments: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mire"))
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("mire runs")
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
        "Git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "mire failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
