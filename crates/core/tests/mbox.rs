use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use mire_core::{ChangesetSource, FileContent, FileStatus, LineKind, PatchLimits, parse_patch, write_patch};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

const GITHUB_PR_PATCH: &[u8] = include_bytes!("fixtures/patches/github_pr_series.patch");
const GITHUB_PR_SHOTS: &str = include_str!("fixtures/patches/github_pr_series_shots.sh");

/// A Git repository for building commit series, removed on drop.
struct Repository {
    root: PathBuf,
}

impl Repository {
    fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mire-mbox-{}-{id}", std::process::id()));
        let repository = Self { root };
        fs::create_dir_all(&repository.root).expect("repository directory can be created");
        repository.git(&["init", "--quiet"]);
        repository
    }

    fn write(&self, path: &str, contents: &[u8]) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=Mire Tests", "-c", "user.email=mire@example.invalid"])
            .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("git prints UTF-8")
    }

    /// Applies `patch` to a checkout of `base` and returns whether the result
    /// matches `head` exactly.
    fn applies_to(&self, base: &str, head: &str, patch: &[u8]) -> bool {
        let checkout = self.root.with_extension("checkout");
        let _ = fs::remove_dir_all(&checkout);
        self.git(&[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            checkout.to_str().unwrap(),
            base,
        ]);
        let patch_path = self.root.with_extension("patch");
        fs::write(&patch_path, patch).unwrap();
        let applied = Command::new("git")
            .args(["apply", "--index", patch_path.to_str().unwrap()])
            .current_dir(&checkout)
            .status()
            .unwrap()
            .success();
        let matches = applied
            && Command::new("git")
                .args(["diff", "--quiet", "--cached", head])
                .current_dir(&checkout)
                .status()
                .unwrap()
                .success();
        self.git(&["worktree", "remove", "--force", checkout.to_str().unwrap()]);
        let _ = fs::remove_file(patch_path);
        matches
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(self.root.with_extension("checkout"));
    }
}

#[test]
fn folded_series_applies_to_the_base_and_matches_the_final_tree() {
    let repository = Repository::new();
    let numbered = |count: usize, label: &str| {
        (1..=count)
            .map(|line| format!("{label} line {line}\n"))
            .collect::<String>()
    };
    repository.write("a.txt", numbered(30, "a").as_bytes());
    repository.write("tail.txt", b"one\ntwo\nthree");
    repository.write("old.txt", numbered(8, "old").as_bytes());
    repository.write("gone.txt", numbered(5, "gone").as_bytes());
    repository.write("run.sh", b"#!/bin/sh\necho run\n");
    repository.write("same.txt", b"unchanged\n");
    let base = repository.commit("base");

    let mut a = numbered(30, "a").replace("a line 2\n", "a line 2 first\n");
    a = a.replace("a line 20\n", "a line 20 first\n");
    repository.write("a.txt", a.as_bytes());
    fs::rename(repository.root.join("old.txt"), repository.root.join("new.txt")).unwrap();
    repository.write(
        "new.txt",
        numbered(8, "old")
            .replace("old line 4\n", "old line 4 renamed\n")
            .as_bytes(),
    );
    repository.write("added.txt", b"added one\nadded two\n");
    let script = repository.root.join("run.sh");
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    fs::set_permissions(&script, permissions).unwrap();
    fs::remove_file(repository.root.join("gone.txt")).unwrap();
    repository.commit("first");

    a = a.replace("a line 3\n", "a line 3 second\n");
    a = a.replace("a line 27\n", "a line 27 second\n");
    repository.write("a.txt", a.as_bytes());
    repository.write(
        "new.txt",
        numbered(8, "old")
            .replace("old line 4\n", "old line 4 renamed\n")
            .replace("old line 8\n", "old line 8 again\n")
            .as_bytes(),
    );
    repository.write("added.txt", b"added one\nadded two\nadded three\n");
    repository.write("tail.txt", b"one\ntwo\nthree\nfour\n");
    repository.write("temp.txt", b"temporary\n");
    repository.commit("second");

    a = a.replace("a line 20 first\n", "a line 20\n");
    a = a.replace("a line 2 first\n", "a line 2 third\n");
    repository.write("a.txt", a.as_bytes());
    fs::remove_file(repository.root.join("temp.txt")).unwrap();
    let head = repository.commit("third");

    let series = repository.git(&["format-patch", "--stdout", &format!("{base}..{head}")]);
    let changeset = parse_patch(
        series.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("the series parses");

    let paths = changeset
        .files()
        .iter()
        .map(|file| {
            let side = file.new_side().or(file.old_side()).unwrap();
            (String::from_utf8(side.path.as_bytes().to_vec()).unwrap(), file.status())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            ("a.txt".to_owned(), FileStatus::Modified),
            ("added.txt".to_owned(), FileStatus::Added),
            ("gone.txt".to_owned(), FileStatus::Deleted),
            ("new.txt".to_owned(), FileStatus::Renamed),
            ("run.sh".to_owned(), FileStatus::Modified),
            ("tail.txt".to_owned(), FileStatus::Modified),
        ],
        "each file appears once, and a file added then deleted is dropped"
    );

    let patch = write_patch(&changeset).expect("the folded series is text");
    assert!(
        repository.applies_to(&base, &head, &patch),
        "the folded diff turns the base into the final tree:\n{}",
        String::from_utf8_lossy(&patch)
    );
}

#[test]
fn single_commit_series_matches_its_plain_diff() {
    let repository = Repository::new();
    repository.write("file.txt", b"one\ntwo\nthree\n");
    let base = repository.commit("base");
    repository.write("file.txt", b"one\n2\nthree\n");
    let head = repository.commit("change");

    let series = repository.git(&["format-patch", "--stdout", &format!("{base}..{head}")]);
    let plain = repository.git(&["diff", &base, &head]);
    let parse = |text: &str| {
        parse_patch(
            text.as_bytes(),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap()
    };
    assert_eq!(parse(&series).files(), parse(&plain).files());
}

#[test]
fn binary_changes_in_a_series_stay_binary() {
    let repository = Repository::new();
    repository.write("text.txt", b"text\n");
    let base = repository.commit("base");
    repository.write("image.bin", b"\x00\x01\x02");
    repository.commit("add binary");
    repository.write("image.bin", b"\x00\x01\x02\x03");
    let head = repository.commit("change binary");

    for flag in ["--binary", "--no-binary"] {
        let series = repository.git(&["format-patch", flag, "--stdout", &format!("{base}..{head}")]);
        let changeset = parse_patch(
            series.as_bytes(),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{flag} series parses: {error}"));
        assert_eq!(changeset.files().len(), 1, "{flag}");
        assert_eq!(changeset.files()[0].status(), FileStatus::Added, "{flag}");
        assert!(matches!(changeset.files()[0].content(), FileContent::Binary), "{flag}");
    }
}

#[test]
fn series_that_does_not_chain_is_rejected() {
    let series = "From 1111111111111111111111111111111111111111 Mon Sep 17 00:00:00 2001
Subject: [PATCH 1/2] One
---
diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-one
+two
From 2222222222222222222222222222222222222222 Mon Sep 17 00:00:00 2001
Subject: [PATCH 2/2] Two
---
diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-one
+three
";
    let error = parse_patch(
        series.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect_err("the second commit expects a line the first commit removed");
    assert!(error.to_string().contains("does not apply"), "{error}");
}

#[test]
fn github_pull_request_patch_folds_to_one_entry_per_file() {
    let changeset = parse_patch(
        GITHUB_PR_PATCH,
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("GitHub's patch for a four-commit pull request parses");

    let paths = changeset
        .files()
        .iter()
        .map(|file| file.new_side().or(file.old_side()).unwrap().path.as_bytes().to_vec())
        .collect::<Vec<_>>();
    let mut unique = paths.clone();
    unique.dedup();
    assert_eq!(paths, unique, "each path appears once");
    assert_eq!(paths.len(), 28);

    let shots = changeset
        .files()
        .iter()
        .find(|file| {
            file.new_side()
                .is_some_and(|side| side.path.as_bytes() == b"scripts/shots.sh")
        })
        .expect("the script three commits changed is present");
    assert_eq!(shots.status(), FileStatus::Added);
    let FileContent::Text { hunks } = shots.content() else {
        panic!("the script is text");
    };
    let added = hunks
        .iter()
        .flat_map(|hunk| hunk.lines())
        .filter(|line| line.kind() == LineKind::Addition)
        .map(|line| format!("{}\n", String::from_utf8_lossy(line.content())))
        .collect::<String>();
    assert_eq!(added, GITHUB_PR_SHOTS, "the folded script is the final version");
}
