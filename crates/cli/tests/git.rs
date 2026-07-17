use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEMP_REPOSITORY_ID: AtomicU64 = AtomicU64::new(0);

struct FixtureRepository {
    path: PathBuf,
    base: String,
    topic: String,
}

impl FixtureRepository {
    fn new() -> Self {
        let id = TEMP_REPOSITORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mire-git-{}-{id}", std::process::id()));
        fs::create_dir(&path).expect("fixture repository directory can be created");
        git(&path, ["init", "--quiet"]);
        git(&path, ["config", "user.name", "Mire Tests"]);
        git(&path, ["config", "user.email", "mire@example.invalid"]);

        write(&path, "tracked.txt", b"base\n");
        write(&path, "delete.txt", b"delete me\n");
        write(&path, "rename-old.txt", b"rename me\n");
        write(&path, "script.sh", b"#!/bin/sh\necho base\n");
        write(&path, "binary.bin", b"\0base\xff");
        let first_gitlink = detached_commit(&path, None, "first gitlink");
        git(&path, ["add", "."]);
        update_gitlink(&path, &first_gitlink);
        git(&path, ["commit", "--quiet", "-m", "base"]);
        let base = git_stdout(&path, ["rev-parse", "HEAD"]);

        git(&path, ["switch", "--quiet", "-c", "topic"]);
        write(&path, "tracked.txt", b"topic\n");
        write(&path, "commit.txt", b"topic commit\n");
        write(&path, "binary.bin", b"\0topic\xfe");
        fs::remove_file(path.join("delete.txt")).expect("tracked deletion can be created");
        git(&path, ["mv", "rename-old.txt", "rename-new.txt"]);
        make_executable(&path.join("script.sh"));
        let second_gitlink = detached_commit(&path, Some(&first_gitlink), "second gitlink");
        git(&path, ["add", "--all"]);
        git(&path, ["update-index", "--chmod=+x", "script.sh"]);
        update_gitlink(&path, &second_gitlink);
        git(&path, ["commit", "--quiet", "-m", "topic"]);
        let topic = git_stdout(&path, ["rev-parse", "HEAD"]);

        git(&path, ["switch", "--quiet", "-"]);
        write(&path, "main-only.txt", b"main branch\n");
        git(&path, ["add", "main-only.txt"]);
        git(&path, ["commit", "--quiet", "-m", "main"]);

        write(&path, "tracked.txt", b"worktree\n");
        write(&path, "staged.txt", b"staged\n");
        git(&path, ["add", "staged.txt"]);
        write(&path, "untracked.txt", b"untracked\n");
        write(&path, "--output=outside.txt", b"argument-like path\n");

        Self { path, base, topic }
    }
}

impl Drop for FixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn default_staged_and_path_filtered_reviews_match_git_boundaries() {
    let repository = FixtureRepository::new();

    let default = run_mire(&repository.path, [OsString::from("diff")]);
    assert_success(&default);
    let default_files = files(&default);
    assert!(default_files.contains_key("tracked.txt"));
    assert!(default_files.contains_key("untracked.txt"));
    assert!(default_files.contains_key("--output=outside.txt"));
    assert!(!default_files.contains_key("staged.txt"));
    assert!(!repository.path.parent().unwrap().join("outside.txt").exists());

    let staged = run_mire(&repository.path, [OsString::from("diff"), OsString::from("--staged")]);
    assert_success(&staged);
    assert_eq!(files(&staged).keys().collect::<Vec<_>>(), [&"staged.txt"]);

    let tracked_path = run_mire(
        &repository.path,
        [
            OsString::from("diff"),
            OsString::from("--"),
            OsString::from("tracked.txt"),
        ],
    );
    assert_success(&tracked_path);
    assert_eq!(files(&tracked_path).keys().collect::<Vec<_>>(), [&"tracked.txt"]);

    let untracked_path = run_mire(
        &repository.path,
        [
            OsString::from("diff"),
            OsString::from("--"),
            OsString::from("untracked.txt"),
        ],
    );
    assert_success(&untracked_path);
    assert_eq!(files(&untracked_path).keys().collect::<Vec<_>>(), [&"untracked.txt"]);
}

#[test]
fn two_dot_three_dot_and_show_keep_native_revision_semantics() {
    let repository = FixtureRepository::new();
    let main = git_stdout(&repository.path, ["rev-parse", "HEAD"]);
    let two_dot_range = format!("{}..{}", repository.topic, main);

    let two_dot = run_mire(
        &repository.path,
        [OsString::from("diff"), OsString::from(&two_dot_range)],
    );
    assert_success(&two_dot);
    let two_dot_files = files(&two_dot);
    assert!(two_dot_files.contains_key("commit.txt"));
    assert!(two_dot_files.contains_key("main-only.txt"));
    assert_eq!(two_dot_files, native_files(&repository.path, "diff", &two_dot_range));

    let three_dot_range = format!("{main}...{}", repository.topic);
    let three_dot = run_mire(
        &repository.path,
        [OsString::from("diff"), OsString::from(&three_dot_range)],
    );
    assert_success(&three_dot);
    let three_dot_files = files(&three_dot);
    assert!(three_dot_files.contains_key("commit.txt"));
    assert!(!three_dot_files.contains_key("main-only.txt"));
    assert_eq!(
        three_dot_files,
        native_files(&repository.path, "diff", &three_dot_range)
    );

    let show = run_mire(
        &repository.path,
        [OsString::from("show"), OsString::from(&repository.topic)],
    );
    assert_success(&show);
    let show_files = files(&show);
    assert_eq!(show_files["delete.txt"]["status"], "deleted");
    assert_eq!(show_files["rename-new.txt"]["status"], "renamed");
    assert_eq!(show_files["binary.bin"]["content"]["kind"], "binary");
    assert_eq!(show_files["script.sh"]["old"]["mode"], "100644");
    assert_eq!(show_files["script.sh"]["new"]["mode"], "100755");
    assert_eq!(show_files["vendor/submodule"]["old"]["mode"], "160000");
    assert_eq!(show_files["vendor/submodule"]["new"]["mode"], "160000");
    assert_eq!(show_files, native_files(&repository.path, "show", &repository.topic));

    let show_path = run_mire(
        &repository.path,
        [
            OsString::from("show"),
            OsString::from(&repository.topic),
            OsString::from("--"),
            OsString::from("commit.txt"),
        ],
    );
    assert_success(&show_path);
    assert_eq!(files(&show_path).keys().collect::<Vec<_>>(), [&"commit.txt"]);
}

#[test]
fn reviews_do_not_change_repository_contents_or_metadata() {
    let repository = FixtureRepository::new();
    let before = repository_fingerprint(&repository.path);

    for arguments in [
        vec![OsString::from("diff")],
        vec![OsString::from("diff"), OsString::from("--staged")],
        vec![
            OsString::from("diff"),
            OsString::from(format!("{}..HEAD", repository.base)),
        ],
        vec![OsString::from("show"), OsString::from(&repository.topic)],
    ] {
        let output = run_mire(&repository.path, arguments);
        assert_success(&output);
    }

    let after = repository_fingerprint(&repository.path);
    assert_eq!(before, after, "Git review changed the fixture repository");
}

#[test]
fn missing_git_invalid_revisions_bare_repositories_and_non_repositories_have_stable_errors() {
    let repository = FixtureRepository::new();

    let invalid = run_mire(
        &repository.path,
        [OsString::from("show"), OsString::from("revision-that-does-not-exist")],
    );
    assert_eq!(invalid.status.code(), Some(6));
    assert!(stderr(&invalid).contains("Git show failed"));

    let missing = Command::new(binary())
        .arg("diff")
        .current_dir(&repository.path)
        .env("PATH", "")
        .output()
        .expect("mire runs without Git on PATH");
    assert_eq!(missing.status.code(), Some(6));
    assert!(stderr(&missing).contains("Git is not available"));

    let non_repository = repository.path.with_extension("not-a-repository");
    fs::create_dir(&non_repository).expect("non-repository directory can be created");
    let outside = run_mire(&non_repository, [OsString::from("diff")]);
    fs::remove_dir(&non_repository).expect("non-repository directory can be removed");
    assert_eq!(outside.status.code(), Some(6));
    assert!(stderr(&outside).contains("not inside a Git repository"));

    let bare = repository.path.join("bare.git");
    git(
        &repository.path,
        [
            OsStr::new("init"),
            OsStr::new("--quiet"),
            OsStr::new("--bare"),
            bare.as_os_str(),
        ],
    );
    let bare_output = run_mire(&bare, [OsString::from("show")]);
    assert_eq!(bare_output.status.code(), Some(6));
    assert!(stderr(&bare_output).contains("bare Git repositories"));
}

fn files(output: &Output) -> BTreeMap<String, Value> {
    let json: Value = serde_json::from_slice(&output.stdout).expect("mire output is JSON");
    json["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|file| {
            let path = file["new"]["path"]
                .as_array()
                .or_else(|| file["old"]["path"].as_array())
                .expect("one file side has a path")
                .iter()
                .map(|byte| byte.as_u64().expect("path component is a byte") as u8)
                .collect::<Vec<_>>();
            (String::from_utf8(path).expect("fixture paths are UTF-8"), file.clone())
        })
        .collect()
}

fn run_mire(path: &Path, arguments: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(binary())
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("mire runs")
}

fn native_files(path: &Path, operation: &str, revision: &str) -> BTreeMap<String, Value> {
    let mut arguments = vec![OsString::from(operation)];
    if operation == "show" {
        arguments.push(OsString::from("--format="));
    }
    arguments.extend([
        OsString::from("--no-color"),
        OsString::from("--no-ext-diff"),
        OsString::from("--no-textconv"),
        OsString::from("--binary"),
        OsString::from("--full-index"),
        OsString::from("--find-renames"),
        OsString::from("--src-prefix=a/"),
        OsString::from("--dst-prefix=b/"),
        OsString::from(revision),
    ]);
    let native = Command::new("git")
        .args(arguments)
        .current_dir(path)
        .output()
        .expect("native Git comparison runs");
    assert!(native.status.success(), "native Git failed: {}", stderr(&native));

    let mut child = Command::new(binary())
        .args(["patch", "-", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mire patch starts");
    child
        .stdin
        .take()
        .expect("patch stdin is piped")
        .write_all(&native.stdout)
        .expect("native patch can be supplied to Mire");
    let normalized = child.wait_with_output().expect("mire patch exits");
    assert_success(&normalized);
    files(&normalized)
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "mire failed: {}", stderr(output));
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
    String::from_utf8(output.stdout)
        .expect("fixture Git output is UTF-8")
        .trim()
        .to_owned()
}

fn detached_commit(path: &Path, parent: Option<&str>, message: &str) -> String {
    let tree = git_stdout(path, ["mktree"]);
    let mut command = Command::new("git");
    command
        .args(["commit-tree", &tree])
        .arg("-m")
        .arg(message)
        .current_dir(path);
    if let Some(parent) = parent {
        command.arg("-p").arg(parent);
    }
    let output = command.output().expect("Git can create a detached fixture commit");
    assert!(output.status.success(), "detached commit failed: {}", stderr(&output));
    String::from_utf8(output.stdout)
        .expect("commit id is UTF-8")
        .trim()
        .to_owned()
}

fn update_gitlink(path: &Path, commit: &str) {
    git(
        path,
        [
            OsStr::new("update-index"),
            OsStr::new("--add"),
            OsStr::new("--cacheinfo"),
            OsStr::new("160000"),
            OsStr::new(commit),
            OsStr::new("vendor/submodule"),
        ],
    );
}

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent directory can be created");
    }
    fs::write(path, contents).expect("fixture file can be written");
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).expect("script metadata is available").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("script can be made executable");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn repository_fingerprint(root: &Path) -> Vec<(PathBuf, u8, u64, Vec<u8>)> {
    let mut entries = Vec::new();
    collect_entries(root, root, &mut entries);
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn collect_entries(root: &Path, directory: &Path, entries: &mut Vec<(PathBuf, u8, u64, Vec<u8>)>) {
    let mut children = fs::read_dir(directory)
        .expect("fixture directory can be read")
        .map(|entry| entry.expect("fixture entry can be read"))
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("fixture metadata can be read");
        let relative = path.strip_prefix(root).expect("entry is below fixture root").to_owned();
        if metadata.is_dir() {
            entries.push((relative, 1, permissions(&metadata), Vec::new()));
            collect_entries(root, &path, entries);
        } else if metadata.is_file() {
            entries.push((
                relative,
                2,
                permissions(&metadata),
                fs::read(&path).expect("fixture file can be read"),
            ));
        } else {
            entries.push((relative, 3, permissions(&metadata), Vec::new()));
        }
    }
}

#[cfg(unix)]
fn permissions(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::PermissionsExt;

    u64::from(metadata.permissions().mode())
}

#[cfg(not(unix))]
fn permissions(metadata: &fs::Metadata) -> u64 {
    u64::from(metadata.permissions().readonly())
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_mire")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
