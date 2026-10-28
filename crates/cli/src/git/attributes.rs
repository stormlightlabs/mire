//! Reads which changed files the reviewed repository marks as generated.
//!
//! A file is generated when its `linguist-generated` attribute is set or
//! `true`, the attribute GitHub reads to collapse a file. When that attribute
//! is unspecified, a file whose `diff` attribute is unset (`-diff`, or the
//! `binary` macro) counts as generated too. `-linguist-generated` or
//! `linguist-generated=false` turns the mark off whatever `diff` says.
//!
//! `git check-attr` reads the attributes, so nested `.gitattributes` files,
//! `$GIT_DIR/info/attributes`, macros and `core.attributesFile` apply as Git
//! applies them.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

use mire_core::{ByteString, Changeset, GitOperation};

use super::{Result, ensure_success, run_git_with_input};

/// The attribute GitHub's Linguist reads to mark a file as generated.
const LINGUIST_GENERATED: &str = "linguist-generated";
/// The attribute whose unset value hides a file's diff.
const DIFF: &str = "diff";
/// Output bytes `git check-attr -z` may print for one path, beyond the path
/// itself: two attribute names, two values and the separators.
const MAX_ATTRIBUTE_LINE_BYTES: usize = 128;

/// The tree whose attribute files apply to a comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttributeSource {
    /// The `.gitattributes` files in the worktree, with the index for any the
    /// worktree lacks. This is what `git diff` itself reads.
    Worktree,
    /// The `.gitattributes` files in the index, for a staged comparison.
    Index,
    /// The `.gitattributes` files in a revision's tree, for a comparison
    /// whose new side is that revision.
    Tree(OsString),
}

/// Picks the tree whose attributes describe the new side of `operation`.
///
/// A comparison against the worktree reads the worktree, a staged one the
/// index, and a comparison whose new side is a revision (`A B`, `A..B`,
/// `A...B`, `A^!`, `A^-`, or a shown commit) that revision's tree. Any other
/// revision form reads the worktree, as `git diff` does.
pub fn attribute_source(operation: &GitOperation) -> AttributeSource {
    match operation {
        GitOperation::Worktree { staged: false, .. } => AttributeSource::Worktree,
        GitOperation::Worktree { staged: true, .. } => AttributeSource::Index,
        GitOperation::Show { revision, .. } => AttributeSource::Tree(byte_string_to_os_string(revision)),
        GitOperation::Diff { revisions, .. } => match revisions.as_slice() {
            [_, new] => AttributeSource::Tree(byte_string_to_os_string(new)),
            [revision] => single_revision_source(revision.as_bytes()),
            _ => AttributeSource::Worktree,
        },
    }
}

/// Marks the files of `changeset` that the repository at `root` marks as
/// generated, reading attributes from `source`.
///
/// When Git cannot read a revision's tree, because the Git in use predates
/// `check-attr --source` (Git 2.40) or the revision does not name a tree,
/// the worktree's attributes apply instead.
pub fn mark_generated(root: &Path, changeset: &mut Changeset, source: &AttributeSource) -> Result<()> {
    if changeset.files().is_empty() {
        return Ok(());
    }
    let paths = changeset
        .files()
        .iter()
        .map(|file| file.canonical_path().as_bytes().to_vec())
        .collect::<Vec<_>>();
    let generated = match generated_paths(root, &paths, source) {
        Err(_) if matches!(source, AttributeSource::Tree(_)) => {
            generated_paths(root, &paths, &AttributeSource::Worktree)?
        }
        result => result?,
    };
    for (index, path) in paths.iter().enumerate() {
        changeset.set_generated(index, generated.contains(path));
    }
    Ok(())
}

/// Returns the members of `paths` that the repository at `root` marks as
/// generated, asking one `git check-attr --stdin -z` run.
fn generated_paths(root: &Path, paths: &[Vec<u8>], source: &AttributeSource) -> Result<BTreeSet<Vec<u8>>> {
    let mut arguments = vec![
        OsString::from("check-attr"),
        OsString::from("-z"),
        OsString::from("--stdin"),
    ];
    match source {
        AttributeSource::Worktree => {}
        AttributeSource::Index => arguments.push(OsString::from("--cached")),
        AttributeSource::Tree(revision) => {
            let mut option = OsString::from("--source=");
            option.push(revision);
            arguments.push(option);
        }
    }
    arguments.push(OsString::from(LINGUIST_GENERATED));
    arguments.push(OsString::from(DIFF));
    let mut input = Vec::new();
    let mut limit = 0_usize;
    for path in paths {
        input.extend_from_slice(path);
        input.push(0);
        limit = limit.saturating_add(path.len().saturating_mul(2).saturating_add(MAX_ATTRIBUTE_LINE_BYTES));
    }
    let output = run_git_with_input(Some(root), &arguments, input, limit)?;
    ensure_success("check attributes", &output, &[0])?;
    Ok(parse_check_attr(&output.stdout))
}

/// Reads `git check-attr -z` output, `<path> NUL <attribute> NUL <value> NUL`
/// per record, and returns the paths it finds generated.
fn parse_check_attr(stdout: &[u8]) -> BTreeSet<Vec<u8>> {
    let mut values = BTreeMap::<&[u8], (&[u8], &[u8])>::new();
    let mut fields = stdout.split(|byte| *byte == 0);
    while let (Some(path), Some(attribute), Some(value)) = (fields.next(), fields.next(), fields.next()) {
        let entry = values.entry(path).or_insert((b"unspecified", b"unspecified"));
        match attribute {
            b"linguist-generated" => entry.0 = value,
            b"diff" => entry.1 = value,
            _ => {}
        }
    }
    values
        .into_iter()
        .filter(|(_, (linguist_generated, diff))| is_generated(linguist_generated, diff))
        .map(|(path, _)| path.to_vec())
        .collect()
}

/// Decides from the two attribute values `git check-attr` printed whether a
/// file is generated.
fn is_generated(linguist_generated: &[u8], diff: &[u8]) -> bool {
    match linguist_generated {
        b"set" | b"true" => true,
        b"unset" | b"false" => false,
        _ => diff == b"unset",
    }
}

/// Reads the new side of one revision argument: the right end of `A..B` or
/// `A...B`, `HEAD` when that end is empty, and `A` for `A^!`, `A^-` and
/// `A^-n`, which mean `A^n..A`. A plain revision is compared with the
/// worktree.
fn single_revision_source(revision: &[u8]) -> AttributeSource {
    if let Some(commit) = parent_shorthand_commit(revision) {
        return AttributeSource::Tree(bytes_to_os_string(commit));
    }
    let Some(position) = revision.windows(2).position(|pair| pair == b"..") else {
        return AttributeSource::Worktree;
    };
    let new = &revision[position + 2..];
    let new = new.strip_prefix(b".").unwrap_or(new);
    if new.is_empty() {
        AttributeSource::Tree(OsString::from("HEAD"))
    } else {
        AttributeSource::Tree(bytes_to_os_string(new))
    }
}

/// Returns `A` from `A^!`, `A^-` or `A^-n`, or `None` for any other form.
fn parent_shorthand_commit(revision: &[u8]) -> Option<&[u8]> {
    if let Some(commit) = revision.strip_suffix(b"^!") {
        return Some(commit);
    }
    let position = revision.windows(2).rposition(|pair| pair == b"^-")?;
    let parent = &revision[position + 2..];
    (position > 0 && parent.iter().all(u8::is_ascii_digit)).then_some(&revision[..position])
}

fn byte_string_to_os_string(value: &ByteString) -> OsString {
    bytes_to_os_string(value.as_bytes())
}

#[cfg(unix)]
fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(bytes.to_vec())
}

#[cfg(not(unix))]
fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use mire_core::BytePath;

    use super::*;

    struct Repository(PathBuf);

    impl Repository {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("mire-attributes-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let repository = Self(root);
            repository.git(&["init", "--quiet"]);
            repository.git(&["config", "user.name", "Mire Tests"]);
            repository.git(&["config", "user.email", "mire@example.invalid"]);
            repository
        }

        fn git(&self, arguments: &[&str]) {
            let output = Command::new("git")
                .args(arguments)
                .current_dir(&self.0)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }

        fn write(&self, path: &str, content: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        fn generated(&self, paths: &[&str], source: &AttributeSource) -> Vec<String> {
            let paths = paths.iter().map(|path| path.as_bytes().to_vec()).collect::<Vec<_>>();
            generated_paths(&self.0, &paths, source)
                .unwrap()
                .into_iter()
                .map(|path| String::from_utf8(path).unwrap())
                .collect()
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Reports whether the installed Git has `check-attr --source`, added in
    /// Git 2.40.
    fn git_reads_tree_attributes() -> bool {
        let output = Command::new("git").arg("--version").output().unwrap();
        let version = String::from_utf8_lossy(&output.stdout);
        let mut numbers = version
            .trim()
            .trim_start_matches("git version ")
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0));
        (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0)) >= (2, 40)
    }

    #[test]
    fn attribute_values_decide_the_mark() {
        assert!(is_generated(b"set", b"unspecified"));
        assert!(is_generated(b"true", b"set"));
        assert!(is_generated(b"unspecified", b"unset"));
        assert!(!is_generated(b"unset", b"unset"));
        assert!(!is_generated(b"false", b"unset"));
        assert!(!is_generated(b"unspecified", b"unspecified"));
        assert!(!is_generated(b"unspecified", b"rust"));
        assert!(!is_generated(b"maybe", b"set"));
    }

    #[test]
    fn check_attr_output_is_read_per_path() {
        let output = b"a.lock\0linguist-generated\0set\0a.lock\0diff\0unspecified\0\
b.png\0linguist-generated\0unspecified\0b.png\0diff\0unset\0\
c.rs\0linguist-generated\0unspecified\0c.rs\0diff\0unspecified\0\
d.min.js\0linguist-generated\0false\0d.min.js\0diff\0unset\0";
        let generated = parse_check_attr(output);
        assert_eq!(generated, BTreeSet::from([b"a.lock".to_vec(), b"b.png".to_vec()]));
    }

    #[test]
    fn nested_files_info_attributes_and_unset_values_apply() {
        let repository = Repository::new("nested");
        repository.write(
            ".gitattributes",
            "*.lock linguist-generated\ndist/** linguist-generated=true\n*.svg -diff\n",
        );
        repository.write(
            "vendor/.gitattributes",
            "*.js linguist-generated=true\nkeep.js -linguist-generated\n",
        );
        repository.write("dist/.gitattributes", "readme.md linguist-generated=false\n");
        repository.write(".git/info/attributes", "schema.json linguist-generated\n");

        let generated = repository.generated(
            &[
                "Cargo.lock",
                "dist/app.js",
                "dist/readme.md",
                "icon.svg",
                "vendor/lib.js",
                "vendor/keep.js",
                "src/main.rs",
                "api/schema.json",
                "space name.lock",
            ],
            &AttributeSource::Worktree,
        );

        assert_eq!(
            generated,
            [
                "Cargo.lock",
                "api/schema.json",
                "dist/app.js",
                "icon.svg",
                "space name.lock",
                "vendor/lib.js",
            ]
        );
    }

    #[test]
    fn staged_and_revision_comparisons_read_their_own_tree() {
        if !git_reads_tree_attributes() {
            eprintln!("skipped: reading a revision's attributes needs Git 2.40 or later");
            return;
        }
        let repository = Repository::new("trees");
        repository.write(".gitattributes", "*.gen linguist-generated\n");
        repository.write("a.gen", "a\n");
        repository.git(&["add", "."]);
        repository.git(&["commit", "--quiet", "-m", "base"]);
        repository.write(".gitattributes", "*.out linguist-generated\n");
        repository.git(&["add", ".gitattributes"]);
        repository.write(".gitattributes", "*.txt linguist-generated\n");
        let paths = ["a.gen", "b.out", "c.txt"];

        assert_eq!(repository.generated(&paths, &AttributeSource::Worktree), ["c.txt"]);
        assert_eq!(repository.generated(&paths, &AttributeSource::Index), ["b.out"]);
        assert_eq!(
            repository.generated(&paths, &AttributeSource::Tree(OsString::from("HEAD"))),
            ["a.gen"]
        );
    }

    #[test]
    fn an_unreadable_revision_falls_back_to_the_worktree() {
        let repository = Repository::new("fallback");
        repository.write(".gitattributes", "*.gen linguist-generated\n");
        repository.write("a.gen", "a\n");
        repository.write("b.txt", "b\n");
        let patch =
            b"diff --git a/a.gen b/a.gen\nnew file mode 100644\n--- /dev/null\n+++ b/a.gen\n@@ -0,0 +1 @@\n+a\n\
diff --git a/b.txt b/b.txt\nnew file mode 100644\n--- /dev/null\n+++ b/b.txt\n@@ -0,0 +1 @@\n+b\n";
        let mut changeset = mire_core::parse_patch(
            patch,
            mire_core::ChangesetSource::DirectFiles,
            mire_core::PatchLimits::default(),
        )
        .unwrap();

        let source = AttributeSource::Tree(OsString::from("no-such-revision"));
        mark_generated(&repository.0, &mut changeset, &source).unwrap();

        let marks = changeset
            .files()
            .iter()
            .map(|file| file.is_generated())
            .collect::<Vec<_>>();
        assert_eq!(marks, [true, false]);
    }

    #[test]
    fn the_new_side_of_each_comparison_picks_the_tree() {
        let diff = |revisions: &[&str]| GitOperation::Diff {
            revisions: revisions.iter().map(|revision| ByteString::from(*revision)).collect(),
            paths: Vec::<BytePath>::new(),
        };
        let tree = |revision: &str| AttributeSource::Tree(OsString::from(revision));

        assert_eq!(
            attribute_source(&GitOperation::Worktree { staged: false, paths: Vec::new() }),
            AttributeSource::Worktree
        );
        assert_eq!(
            attribute_source(&GitOperation::Worktree { staged: true, paths: Vec::new() }),
            AttributeSource::Index
        );
        assert_eq!(
            attribute_source(&GitOperation::Show { revision: ByteString::from("v1"), paths: Vec::new() }),
            tree("v1")
        );
        assert_eq!(attribute_source(&diff(&["main"])), AttributeSource::Worktree);
        assert_eq!(attribute_source(&diff(&["main", "topic"])), tree("topic"));
        assert_eq!(attribute_source(&diff(&["main..topic"])), tree("topic"));
        assert_eq!(attribute_source(&diff(&["main...topic"])), tree("topic"));
        assert_eq!(attribute_source(&diff(&["main..."])), tree("HEAD"));
        assert_eq!(attribute_source(&diff(&["topic^!"])), tree("topic"));
        assert_eq!(attribute_source(&diff(&["topic^-"])), tree("topic"));
        assert_eq!(attribute_source(&diff(&["topic~2^-2"])), tree("topic~2"));
        assert_eq!(attribute_source(&diff(&["^-"])), AttributeSource::Worktree);
        assert_eq!(attribute_source(&diff(&["topic^-x"])), AttributeSource::Worktree);
        assert_eq!(attribute_source(&diff(&["a", "b", "c"])), AttributeSource::Worktree);
    }
}
