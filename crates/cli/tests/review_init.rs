use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use mire::read_review;

static TEST_ID: AtomicU64 = AtomicU64::new(0);

struct FixtureRepository {
    directory: PathBuf,
    root: PathBuf,
    base: String,
}

impl FixtureRepository {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("mire-review-init-{}-{id}", std::process::id()));
        let root = directory.join("repository");
        fs::create_dir_all(&root).expect("fixture repository directory can be created");
        git(&root, ["init", "--quiet"]);
        git(&root, ["config", "user.name", "Mire Tests"]);
        git(&root, ["config", "user.email", "mire@example.invalid"]);

        fs::write(root.join("tracked.txt"), b"base\n").expect("tracked fixture can be written");
        fs::write(root.join("range.txt"), b"base\n").expect("range fixture can be written");
        git(&root, ["add", "."]);
        git(&root, ["commit", "--quiet", "-m", "base"]);
        let base = git_stdout(&root, ["rev-parse", "HEAD"]);

        fs::write(root.join("range.txt"), b"committed\n").expect("range change can be written");
        git(&root, ["add", "range.txt"]);
        git(&root, ["commit", "--quiet", "-m", "range change"]);
        fs::write(root.join("tracked.txt"), b"worktree\n").expect("worktree change can be written");
        fs::write(root.join("staged.txt"), b"staged\n").expect("staged change can be written");
        git(&root, ["add", "staged.txt"]);

        Self { directory, root, base }
    }

    fn review_path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
}

impl Drop for FixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn worktree_staged_range_and_filtered_initialization_match_diff_json() {
    let repository = FixtureRepository::new();
    let requests = [
        ("worktree.json", Vec::new()),
        ("staged.json", vec![OsString::from("--staged")]),
        ("range.json", vec![OsString::from(format!("{}..HEAD", repository.base))]),
        (
            "filtered.json",
            vec![OsString::from("--"), OsString::from("tracked.txt")],
        ),
    ];

    for (name, request) in requests {
        let diff = run_mire(
            &repository.root,
            [
                OsString::from("diff"),
                OsString::from("--format"),
                OsString::from("json"),
            ]
            .into_iter()
            .chain(request.clone()),
        );
        assert_success(&diff);

        let review_path = repository.review_path(name);
        let initialized = run_mire(
            &repository.root,
            [
                OsString::from("review"),
                OsString::from("init"),
                review_path.as_os_str().to_owned(),
            ]
            .into_iter()
            .chain(request),
        );
        assert_success(&initialized);

        let review = read_review(&review_path).expect("initialized review is valid");
        assert_eq!(review.revision().get(), 1);
        assert!(review.notes().is_empty());
        assert!(review.events().is_empty());
        let mut expected = diff.stdout;
        assert_eq!(expected.pop(), Some(b'\n'));
        assert_eq!(serde_json::to_vec(review.changeset()).unwrap(), expected);

        let output = String::from_utf8(initialized.stdout).expect("success output is UTF-8");
        assert_eq!(
            output,
            format!(
                "review: {:?}\nchangeset: {}\nrevision: 1\n",
                review_path,
                review.changeset().fingerprint()
            )
        );
    }
}

#[test]
fn initialization_refuses_existing_files_and_cleans_up_after_failures() {
    let repository = FixtureRepository::new();
    let existing = repository.review_path("existing.json");
    fs::write(&existing, b"do not replace\n").expect("existing destination can be written");

    let refused = run_mire(
        &repository.root,
        [
            OsString::from("review"),
            OsString::from("init"),
            existing.as_os_str().to_owned(),
        ],
    );
    assert_eq!(refused.status.code(), Some(7));
    assert!(stderr(&refused).contains("already exists"));
    assert_eq!(fs::read(&existing).unwrap(), b"do not replace\n");

    let failed = repository.review_path("failed.json");
    let invalid = run_mire(
        &repository.root,
        [
            OsString::from("review"),
            OsString::from("init"),
            failed.as_os_str().to_owned(),
            OsString::from("revision-that-does-not-exist"),
        ],
    );
    assert_eq!(invalid.status.code(), Some(6));
    assert!(!failed.exists());

    let siblings = fs::read_dir(&repository.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(
        !siblings
            .iter()
            .any(|name| name.to_string_lossy().contains("mire-write"))
    );
}

fn run_mire(path: &Path, arguments: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(binary())
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
        .expect("Git is available for fixture setup");
    assert!(
        output.status.success(),
        "fixture Git command failed: {}",
        stderr(&output)
    );
}

fn git_stdout<I, S>(path: &Path, arguments: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("Git is available for fixture setup");
    assert!(
        output.status.success(),
        "fixture Git command failed: {}",
        stderr(&output)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "mire failed: {}", stderr(output));
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
