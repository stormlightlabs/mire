use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use mire_core::{ChangesetSource, FileContent, FileDiff, FileStatus, LineKind, PatchLimits, parse_patch, write_patch};

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

/// Builds a series of commits, each a list of `(path, diff body)` pairs.
fn series(commits: &[&[(&str, String)]]) -> String {
    let mut text = String::new();
    for (number, files) in commits.iter().enumerate() {
        text.push_str(&format!(
            "From {:040x} Mon Sep 17 00:00:00 2001\nSubject: [PATCH {}/{}] Commit\n---\n",
            number + 1,
            number + 1,
            commits.len()
        ));
        for (path, body) in *files {
            text.push_str(&format!(
                "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{body}"
            ));
        }
    }
    text
}

fn hunk_starts(changeset: &mire_core::Changeset) -> Vec<(u64, u64)> {
    changeset
        .files()
        .iter()
        .flat_map(|file| match file.content() {
            FileContent::Text { hunks } => hunks.iter().map(|hunk| (hunk.old_start(), hunk.new_start())).collect(),
            FileContent::Binary => Vec::new(),
        })
        .collect()
}

#[test]
fn hunks_at_large_line_numbers_fold_without_a_line_per_number() {
    for line in [40_000_000_u64, 1_000_000_000, 1 << 40] {
        let first = "@@ -1 +1 @@\n-one\n+two\n".to_owned();
        let second = format!("@@ -{line},2 +{line},2 @@\n far\n-x\n+y\n");
        let text = series(&[&[("file.txt", first)], &[("file.txt", second)]]);
        let started = std::time::Instant::now();
        let changeset = parse_patch(
            text.as_bytes(),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap_or_else(|error| panic!("line {line}: {error}"));
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "line {line}");
        assert_eq!(hunk_starts(&changeset), [(1, 1), (line, line)], "line {line}");
        let FileContent::Text { hunks } = changeset.files()[0].content() else {
            panic!("the file is text");
        };
        let kinds = hunks[1].lines().iter().map(|line| line.kind()).collect::<Vec<_>>();
        assert_eq!(kinds, [LineKind::Context, LineKind::Deletion, LineKind::Addition]);
    }
}

#[test]
fn hunk_past_the_largest_line_number_is_rejected() {
    let first = "@@ -1 +1 @@\n-one\n+two\n".to_owned();
    let second = format!("@@ -{max},2 +{max},2 @@\n far\n-x\n+y\n", max = u64::MAX);
    let text = series(&[&[("file.txt", first)], &[("file.txt", second)]]);
    let result = parse_patch(
        text.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    );
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn series_with_many_hunks_folds_in_linear_time() {
    // Quadratic work over this many hunks takes over twenty seconds in a debug
    // build; linear work takes about two.
    const HUNKS: u64 = 20_000;
    let first = "@@ -1 +1 @@\n-line 1\n+changed 1\n".to_owned();
    let second = (1..=HUNKS)
        .map(|index| {
            let line = index * 8 + 1;
            format!("@@ -{line} +{line} @@\n-line {line}\n+changed {line}\n")
        })
        .collect::<String>();
    let text = series(&[&[("file.txt", first)], &[("file.txt", second)]]);
    let started = std::time::Instant::now();
    let changeset = parse_patch(
        text.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("the series parses");
    let elapsed = started.elapsed();
    assert_eq!(hunk_starts(&changeset).len(), HUNKS as usize + 1);
    assert!(elapsed < std::time::Duration::from_secs(10), "folding took {elapsed:?}");
}

#[test]
fn copy_starts_from_its_source_before_the_same_commit_changes_it() {
    let repository = Repository::new();
    let numbered = |count: usize| (1..=count).map(|line| format!("line {line}\n")).collect::<String>();
    repository.write("a.txt", numbered(30).as_bytes());
    let base = repository.commit("base");

    let a = numbered(30).replace("line 10\n", "line 10 first\n");
    repository.write("a.txt", a.as_bytes());
    repository.commit("first");

    repository.write("a.txt", a.replace("line 19\n", "line 19 second\n").as_bytes());
    repository.write("c.txt", a.replace("line 1\n", "line 1 copy\n").as_bytes());
    let head = repository.commit("second");

    let series = repository.git(&[
        "format-patch",
        "-C",
        "--find-copies-harder",
        "--stdout",
        &format!("{base}..{head}"),
    ]);
    assert!(
        series.contains("copy from a.txt"),
        "the series records a copy:\n{series}"
    );
    let changeset = parse_patch(
        series.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("the series parses");
    let statuses = changeset.files().iter().map(FileDiff::status).collect::<Vec<_>>();
    assert_eq!(statuses, [FileStatus::Modified, FileStatus::Copied]);

    let patch = write_patch(&changeset).expect("the folded series is text");
    assert!(
        repository.applies_to(&base, &head, &patch),
        "the folded diff turns the base into the final tree:\n{}",
        String::from_utf8_lossy(&patch)
    );
}

#[test]
fn copy_overlapping_its_source_change_in_the_same_commit_folds() {
    let repository = Repository::new();
    let numbered = |count: usize| (1..=count).map(|line| format!("line {line}\n")).collect::<String>();
    repository.write("a.txt", numbered(30).as_bytes());
    let base = repository.commit("base");

    let a = numbered(30).replace("line 10\n", "line 10 first\n");
    repository.write("a.txt", a.as_bytes());
    repository.commit("first");

    repository.write("a.txt", a.replace("line 2\n", "line 2 source\n").as_bytes());
    repository.write("c.txt", a.replace("line 2\n", "line 2 copy\n").as_bytes());
    let head = repository.commit("second");

    let series = repository.git(&[
        "format-patch",
        "-C",
        "--find-copies-harder",
        "--stdout",
        &format!("{base}..{head}"),
    ]);
    assert!(
        series.contains("copy from a.txt"),
        "the series records a copy:\n{series}"
    );
    let changeset = parse_patch(
        series.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("the copy applies to its source as the commit found it");
    let patch = write_patch(&changeset).expect("the folded series is text");
    assert!(
        repository.applies_to(&base, &head, &patch),
        "the folded diff turns the base into the final tree:\n{}",
        String::from_utf8_lossy(&patch)
    );
}

#[test]
fn insertion_without_context_splits_an_unseen_run() {
    let text = series(&[
        &[("file.txt", "@@ -1 +1 @@\n-one\n+two\n".to_owned())],
        &[("file.txt", "@@ -10,0 +11 @@\n+inserted\n".to_owned())],
        &[("file.txt", "@@ -20 +20 @@\n-x\n+y\n".to_owned())],
    ]);
    let changeset = parse_patch(
        text.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .expect("the series parses");
    assert_eq!(hunk_starts(&changeset), [(1, 1), (10, 11), (19, 20)]);
}
