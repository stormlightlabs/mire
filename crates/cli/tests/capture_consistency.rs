//! A worktree comparison whose files do not change while Git reads them must
//! pass the torn-capture check, whatever filters, attributes or file kinds
//! sit between the worktree and the blob ids Git prints.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static REPOSITORY_ID: AtomicU64 = AtomicU64::new(0);

/// A throwaway repository with one base commit.
struct Repository {
    path: PathBuf,
}

impl Repository {
    /// Creates a repository, applies `setup` before the base commit so
    /// filters and attributes cover the committed blobs, and commits.
    fn new(setup: impl FnOnce(&Self)) -> Self {
        Self::with_init(&["init", "--quiet"], setup).expect("git init succeeds")
    }

    /// Like [`Repository::new`], or `None` when `git init` rejects `init`.
    fn with_init(init: &[&str], setup: impl FnOnce(&Self)) -> Option<Self> {
        let id = REPOSITORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mire-consistency-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let repository = Self { path };
        if !Command::new("git")
            .args(init)
            .current_dir(&repository.path)
            .output()
            .unwrap()
            .status
            .success()
        {
            return None;
        }
        repository.git(&["config", "user.name", "Mire Tests"]);
        repository.git(&["config", "user.email", "mire@example.invalid"]);
        repository.git(&["config", "commit.gpgsign", "false"]);
        setup(&repository);
        repository.git(&["add", "--all"]);
        repository.git(&["commit", "--quiet", "--allow-empty", "-m", "base"]);
        Some(repository)
    }

    fn git(&self, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&self.path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    /// Runs `mire diff` and asserts it succeeds with every path in
    /// `checked` captured as text with a new blob id, so the torn-capture
    /// check ran on it. Returns the captured files.
    fn assert_accepted(&self, checked: &[&str]) -> Vec<(String, Value)> {
        self.assert_accepted_against(None, checked)
    }

    /// Like [`Repository::assert_accepted`], comparing the worktree with
    /// `revision` instead of the index when one is given.
    fn assert_accepted_against(&self, revision: Option<&str>, checked: &[&str]) -> Vec<(String, Value)> {
        let output = Command::new(env!("CARGO_BIN_EXE_mire"))
            .args(["diff", "--format", "json"])
            .args(revision)
            .current_dir(&self.path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "a consistent capture was rejected (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let files = files(&output);
        for path in checked {
            let file = &files
                .iter()
                .find(|(name, _)| name == path)
                .unwrap_or_else(|| panic!("{path} is missing from {:?}", names(&files)))
                .1;
            assert_eq!(file["content"]["kind"], "text", "{path} is not captured as text");
            assert!(file["new"]["object"].is_string(), "{path} has no new blob id");
        }
        files
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn autocrlf_worktree_files_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.git(&["config", "core.autocrlf", "true"]);
        repository.write("file.txt", "one\r\ntwo\r\n");
    });
    repository.write("file.txt", "one\r\nthree\r\n");
    repository.write("new.txt", "new\r\n");
    repository.assert_accepted(&["file.txt", "new.txt"]);
}

#[test]
fn clean_filtered_files_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.git(&["config", "filter.upper.clean", "tr a-z A-Z"]);
        repository.git(&["config", "filter.upper.smudge", "cat"]);
        repository.write(".gitattributes", "*.txt filter=upper\n");
        repository.write("file.txt", "one\ntwo\n");
    });
    repository.write("file.txt", "one\nthree\n");
    repository.assert_accepted(&["file.txt"]);
}

#[test]
fn eol_crlf_files_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.write(".gitattributes", "*.txt text eol=crlf\n");
        repository.write("file.txt", "one\r\ntwo\r\n");
    });
    repository.write("file.txt", "one\r\nthree\r\n");
    repository.assert_accepted(&["file.txt"]);
}

#[test]
fn ident_expanded_files_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.write(".gitattributes", "*.c ident\n");
        repository.write("file.c", "/* $Id$ */\nint one;\n");
    });
    // Check the file out again so the worktree holds the expanded keyword.
    fs::remove_file(repository.path.join("file.c")).unwrap();
    repository.git(&["checkout", "--", "file.c"]);
    let expanded = fs::read_to_string(repository.path.join("file.c")).unwrap();
    assert!(expanded.contains("$Id: "), "ident did not expand: {expanded}");
    repository.write("file.c", expanded.replace("int one;", "int two;"));
    repository.assert_accepted(&["file.c"]);
}

#[test]
fn working_tree_encoded_files_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.write(".gitattributes", "*.txt text working-tree-encoding=UTF-16\n");
        repository.write("file.txt", utf16("one\ntwo\n"));
    });
    repository.write("file.txt", utf16("one\nthree\n"));
    repository.assert_accepted(&["file.txt"]);
}

#[test]
fn untracked_files_under_a_filter_are_accepted() {
    let repository = Repository::new(|repository| {
        repository.git(&["config", "core.autocrlf", "true"]);
        repository.git(&["config", "filter.upper.clean", "tr a-z A-Z"]);
        repository.git(&["config", "filter.upper.smudge", "cat"]);
        repository.write(".gitattributes", "*.up filter=upper\n*.txt text eol=crlf\n");
    });
    repository.write("new.up", "one\ntwo\n");
    repository.write("new.txt", "one\r\ntwo\r\n");
    repository.assert_accepted(&["new.up", "new.txt"]);
}

#[cfg(unix)]
#[test]
fn symlink_changes_are_accepted() {
    use std::os::unix::fs::symlink;

    let repository = Repository::new(|repository| {
        repository.write("one.txt", "one\n");
        repository.write("two.txt", "two\n");
        symlink("one.txt", repository.path.join("link")).unwrap();
    });
    fs::remove_file(repository.path.join("link")).unwrap();
    symlink("two.txt", repository.path.join("link")).unwrap();
    symlink("one.txt", repository.path.join("new-link")).unwrap();
    repository.assert_accepted(&["link", "new-link"]);
}

#[test]
fn renames_with_edits_are_accepted() {
    let lines = (1..=20).map(|line| format!("line {line}\n")).collect::<String>();
    let repository = Repository::new(|repository| repository.write("old.txt", &lines));
    repository.git(&["mv", "old.txt", "new.txt"]);
    repository.write("new.txt", lines.replace("line 10\n", "line ten\n"));
    // The index already holds the rename, so compare the worktree with HEAD.
    let files = repository.assert_accepted_against(Some("HEAD"), &["new.txt"]);
    assert_eq!(files[0].1["status"], "renamed");
}

#[test]
fn files_that_become_empty_are_accepted() {
    let repository = Repository::new(|repository| repository.write("file.txt", "one\ntwo\n"));
    repository.write("file.txt", "");
    repository.write("empty.txt", "");
    repository.assert_accepted(&["file.txt", "empty.txt"]);
}

#[test]
fn intent_to_add_files_are_accepted() {
    let repository = Repository::new(|_| {});
    repository.write("added.txt", "one\ntwo\n");
    repository.git(&["add", "-N", "added.txt"]);
    repository.assert_accepted(&["added.txt"]);
}

#[test]
fn sha256_repositories_are_accepted() {
    let Some(repository) = Repository::with_init(&["init", "--quiet", "--object-format=sha256"], |repository| {
        repository.write("file.txt", "one\ntwo\n");
    }) else {
        eprintln!("skipped: this git cannot create a SHA-256 repository");
        return;
    };
    repository.write("file.txt", "one\nthree\n");
    repository.write("new.txt", "new\n");
    let files = repository.assert_accepted(&["file.txt", "new.txt"]);
    for (path, file) in files {
        let object = file["new"]["object"].as_str().unwrap();
        assert_eq!(object.len(), 64, "{path} has a non-SHA-256 blob id {object}");
    }
}

fn files(output: &Output) -> Vec<(String, Value)> {
    let json: Value = serde_json::from_slice(&output.stdout).expect("mire prints JSON");
    json["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|file| {
            let side = if file["new"].is_null() { &file["old"] } else { &file["new"] };
            let path = side["path"]
                .as_array()
                .expect("a file side has a path")
                .iter()
                .map(|byte| byte.as_u64().expect("a path byte") as u8)
                .collect::<Vec<_>>();
            (String::from_utf8(path).expect("test paths are UTF-8"), file.clone())
        })
        .collect()
}

fn names(files: &[(String, Value)]) -> Vec<&str> {
    files.iter().map(|(name, _)| name.as_str()).collect()
}

fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}
