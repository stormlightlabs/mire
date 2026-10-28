//! Finds the blob ids of a worktree capture that Mire cannot trust.
//!
//! `git diff` hashes a changed worktree file for the `index` line and reads
//! it again for the hunks. A write between the two reads gives a patch whose
//! new blob id names one content while its hunks show another. Rebuilding the
//! new side from the old blob and the hunks, and hashing it, finds that
//! mismatch. A read cut short can also pad a text file with zeros, so Git
//! reports it as binary and prints no hunks to check.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

use mire_core::{
    BytePath, Changeset, FileContent, FileDiff, FileMode, Fingerprint, GitOperation, Hunk, LineKind, ObjectId,
    blob_matches, rebuild_new_side,
};

use super::{BLOB_CHECK_CHUNK, MAX_BLOB_CHECK_LINE_BYTES, Result, ensure_success, run_git_with_input};

/// Leading bytes Git searches for a NUL to call content binary.
const BINARY_PROBE_BYTES: usize = 8000;

/// The files of one capture whose new-side blob id Mire could not check.
///
/// Each entry is an index into [`Changeset::files`].
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Findings {
    /// Files whose content contradicts their new blob id. Another capture
    /// may read them whole.
    pub torn: BTreeSet<usize>,
    /// Files Mire cannot check in any capture: the old blob is missing or
    /// larger than the size limit.
    pub unchecked: BTreeSet<usize>,
}

/// Checks the captures of one comparison.
///
/// It keeps what it learns across the retries of one capture call, so a
/// retry reads no blob size, and checks no file, a second time.
pub struct Checker<'a> {
    root: &'a Path,
    max_blob_bytes: usize,
    sizes: BTreeMap<ObjectId, Option<usize>>,
    text: BTreeMap<ObjectId, bool>,
    checked: BTreeSet<Fingerprint>,
    bytes_read: usize,
}

/// What checking one file takes.
enum Plan<'a> {
    /// Nothing to check: the file stores no new blob id, or Git does not
    /// hash it from the worktree.
    Skip,
    /// The sides already contradict the hunks.
    Torn,
    /// A text file whose rebuilt new side must hash to `new`.
    Text {
        old: Option<&'a ObjectId>,
        new: &'a ObjectId,
        hunks: &'a [Hunk],
    },
    /// A binary file whose old blob, if it is text, makes it suspect.
    Binary { old: &'a ObjectId, path: &'a BytePath },
}

/// A file whose old blob must be read before it can be judged.
struct Pending<'a> {
    index: usize,
    old: &'a ObjectId,
    plan: Plan<'a>,
}

/// Reports whether a comparison reads worktree files, the only case where
/// Git can hash one content and diff another.
///
/// Staged and revision-to-revision comparisons read stored objects, which do
/// not change. One revision without `..` compares it with the worktree.
pub fn reads_worktree(operation: &GitOperation) -> bool {
    match operation {
        GitOperation::Worktree { staged, .. } => !staged,
        GitOperation::Diff { revisions, .. } => match revisions.as_slice() {
            [revision] => !revision.as_bytes().windows(2).any(|pair| pair == b".."),
            _ => false,
        },
        GitOperation::Show { .. } => false,
    }
}

impl<'a> Checker<'a> {
    /// Checks captures of the repository at `root`, reading no blob, and
    /// rebuilding no file, larger than `max_blob_bytes`.
    pub fn new(root: &'a Path, max_blob_bytes: usize) -> Self {
        Self {
            root,
            max_blob_bytes,
            sizes: BTreeMap::new(),
            text: BTreeMap::new(),
            checked: BTreeSet::new(),
            bytes_read: 0,
        }
    }

    /// Returns the files of `changeset` whose new blob id is torn or cannot
    /// be checked.
    ///
    /// A text file's new side is rebuilt from its old blob and hunks and
    /// hashed. A binary file is torn when its old blob is text and no `diff`
    /// attribute marks it binary. Old-side ids name stored objects, which a
    /// worktree write cannot change, so they are not checked.
    pub fn check(&mut self, changeset: &Changeset) -> Result<Findings> {
        let mut findings = Findings::default();
        let mut pending = Vec::new();
        let mut binary_text = Vec::new();
        let plans = changeset
            .files()
            .iter()
            .enumerate()
            .filter(|(_, file)| !self.checked.contains(&file.fingerprint()))
            .map(|(index, file)| (index, file, plan_for(file)))
            .collect::<Vec<_>>();
        let olds = plans
            .iter()
            .filter_map(|(_, _, plan)| match plan {
                Plan::Text { old: Some(old), .. } => Some(*old),
                Plan::Binary { old, .. } if !self.text.contains_key(*old) => Some(*old),
                _ => None,
            })
            .filter(|old| !self.sizes.contains_key(*old))
            .cloned()
            .collect::<BTreeSet<_>>();
        self.sizes.extend(blob_sizes(self.root, &olds)?);
        for (index, file, plan) in plans {
            match plan {
                Plan::Skip => {
                    self.checked.insert(file.fingerprint());
                }
                Plan::Torn => {
                    findings.torn.insert(index);
                }
                Plan::Text { old: None, new, hunks } => {
                    if rebuilt_len(0, hunks) > self.max_blob_bytes {
                        findings.unchecked.insert(index);
                    } else if matches_hunks(b"", new, hunks) {
                        self.checked.insert(file.fingerprint());
                    } else {
                        findings.torn.insert(index);
                    }
                }
                Plan::Text { old: Some(old), hunks, .. } => match self.sizes.get(old).copied().flatten() {
                    Some(size) if size <= self.max_blob_bytes && rebuilt_len(size, hunks) <= self.max_blob_bytes => {
                        pending.push(Pending { index, old, plan });
                    }
                    _ => {
                        findings.unchecked.insert(index);
                    }
                },
                Plan::Binary { old, path } => match self.text.get(old) {
                    Some(true) => binary_text.push((index, path)),
                    Some(false) => {
                        self.checked.insert(file.fingerprint());
                    }
                    None => match self.sizes.get(old).copied().flatten() {
                        Some(size) if size <= self.max_blob_bytes => pending.push(Pending { index, old, plan }),
                        _ => {
                            findings.unchecked.insert(index);
                        }
                    },
                },
            }
        }

        let reads = pending
            .iter()
            .map(|pending| (pending.old.clone(), self.sizes[pending.old].unwrap_or_default()))
            .collect::<BTreeMap<_, _>>();
        for batch in size_batches(&reads, self.max_blob_bytes) {
            let output = read_blobs(self.root, &batch, &reads)?;
            self.bytes_read = self.bytes_read.saturating_add(output.len());
            let blobs = parse_batch(&output);
            for pending in pending.iter().filter(|pending| batch.contains(pending.old)) {
                let fingerprint = changeset.files()[pending.index].fingerprint();
                let Some(old) = blobs.get(pending.old) else {
                    findings.unchecked.insert(pending.index);
                    continue;
                };
                match pending.plan {
                    Plan::Text { new, hunks, .. } => {
                        if matches_hunks(old, new, hunks) {
                            self.checked.insert(fingerprint);
                        } else {
                            findings.torn.insert(pending.index);
                        }
                    }
                    Plan::Binary { path, .. } => {
                        let text = is_text(old);
                        self.text.insert(pending.old.clone(), text);
                        if text {
                            binary_text.push((pending.index, path));
                        } else {
                            self.checked.insert(fingerprint);
                        }
                    }
                    Plan::Skip | Plan::Torn => {}
                }
            }
        }

        let marked = marked_binary(self.root, binary_text.iter().map(|(_, path)| *path))?;
        for (index, path) in binary_text {
            if marked.contains(path) {
                self.checked.insert(changeset.files()[index].fingerprint());
            } else {
                findings.torn.insert(index);
            }
        }
        Ok(findings)
    }
}

/// Returns what checking `file` takes.
fn plan_for(file: &FileDiff) -> Plan<'_> {
    let Some(new) = file.new_side() else {
        return Plan::Skip;
    };
    let Some(new_object) = new.object.as_ref() else {
        return Plan::Skip;
    };
    let gitlink = |mode: Option<FileMode>| mode == Some(FileMode::GITLINK);
    if gitlink(new.mode) || file.old_side().is_some_and(|old| gitlink(old.mode)) {
        return Plan::Skip;
    }
    let old_object = match file.old_side() {
        None => None,
        Some(old) => match old.object.as_ref() {
            None => return Plan::Skip,
            Some(old_object) => Some(old_object),
        },
    };
    match file.content() {
        FileContent::Text { hunks } => {
            if old_object == Some(new_object) {
                return if hunks.is_empty() { Plan::Skip } else { Plan::Torn };
            }
            Plan::Text { old: old_object, new: new_object, hunks }
        }
        // An added binary file has no old side to compare with. Its id is a
        // hash of one whole read, and no hunks contradict it.
        FileContent::Binary => match old_object {
            Some(old) if old != new_object => Plan::Binary { old, path: &new.path },
            _ => Plan::Skip,
        },
    }
}

/// Reports whether the new side rebuilt from `old` and `hunks` hashes to
/// `new`.
fn matches_hunks(old: &[u8], new: &ObjectId, hunks: &[Hunk]) -> bool {
    rebuild_new_side(old, hunks).is_some_and(|content| blob_matches(&content, new))
}

/// Returns about how many bytes rebuilding a file of `old` bytes with
/// `hunks` gives, without reading the file.
fn rebuilt_len(old: usize, hunks: &[Hunk]) -> usize {
    hunks
        .iter()
        .flat_map(Hunk::lines)
        .fold(old, |len, line| match line.kind() {
            LineKind::Addition => len.saturating_add(line.content().len() + 1),
            LineKind::Deletion => len.saturating_sub(line.content().len() + 1),
            LineKind::Context => len,
        })
}

/// Reports whether Git would diff `content` as text: no NUL in its first
/// [`BINARY_PROBE_BYTES`].
fn is_text(content: &[u8]) -> bool {
    !content[..content.len().min(BINARY_PROBE_BYTES)].contains(&0)
}

/// Groups ids so each `git cat-file --batch` run reads at most `limit` bytes
/// of content.
fn size_batches(sizes: &BTreeMap<ObjectId, usize>, limit: usize) -> Vec<BTreeSet<ObjectId>> {
    let mut batches = Vec::new();
    let mut batch = BTreeSet::new();
    let mut bytes = 0usize;
    for (id, size) in sizes {
        if !batch.is_empty() && (bytes.saturating_add(*size) > limit || batch.len() == BLOB_CHECK_CHUNK) {
            batches.push(std::mem::take(&mut batch));
            bytes = 0;
        }
        batch.insert(id.clone());
        bytes = bytes.saturating_add(*size);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// Returns the size of each id in `objects`, or `None` for one the
/// repository does not hold as a blob.
fn blob_sizes(root: &Path, objects: &BTreeSet<ObjectId>) -> Result<BTreeMap<ObjectId, Option<usize>>> {
    let ids = objects.iter().collect::<Vec<_>>();
    let mut sizes = objects.iter().map(|id| (id.clone(), None)).collect::<BTreeMap<_, _>>();
    for chunk in ids.chunks(BLOB_CHECK_CHUNK) {
        let input = id_lines(chunk.iter().copied());
        let arguments = [OsString::from("cat-file"), OsString::from("--batch-check")];
        let output = run_git_with_input(Some(root), &arguments, input, chunk.len() * MAX_BLOB_CHECK_LINE_BYTES)?;
        ensure_success("check objects", &output, &[0])?;
        for line in output.stdout.split(|byte| *byte == b'\n') {
            if let Some((id, size)) = blob_header(line) {
                sizes.insert(id, Some(size));
            }
        }
    }
    Ok(sizes)
}

/// Runs `git cat-file --batch` for `ids`, with an output limit that follows
/// from the sizes `git cat-file --batch-check` reported.
fn read_blobs(root: &Path, ids: &BTreeSet<ObjectId>, sizes: &BTreeMap<ObjectId, usize>) -> Result<Vec<u8>> {
    let limit = ids
        .iter()
        .map(|id| sizes[id].saturating_add(MAX_BLOB_CHECK_LINE_BYTES + 1))
        .fold(0usize, usize::saturating_add);
    let arguments = [OsString::from("cat-file"), OsString::from("--batch")];
    let output = run_git_with_input(Some(root), &arguments, id_lines(ids.iter()), limit)?;
    ensure_success("read objects", &output, &[0])?;
    Ok(output.stdout)
}

/// Returns the paths whose `diff` attribute is unset, as the `binary` macro
/// and `-diff` set it, so Git diffs them as binary whatever their content.
fn marked_binary<'p>(root: &Path, paths: impl Iterator<Item = &'p BytePath>) -> Result<BTreeSet<BytePath>> {
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    if input.is_empty() {
        return Ok(BTreeSet::new());
    }
    let arguments = ["check-attr", "-z", "--stdin", "diff"].map(OsString::from);
    // Each record repeats the path and adds `diff` and a value no longer
    // than `unspecified`, each followed by a NUL.
    let limit = input.len() + (input.iter().filter(|byte| **byte == 0).count() * 32);
    let output = run_git_with_input(Some(root), &arguments, input, limit)?;
    ensure_success("check attributes", &output, &[0])?;
    let fields = output.stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    Ok(fields
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|[_, _, value]| *value == b"unset")
        .filter_map(|[path, _, _]| BytePath::new(path.to_vec()).ok())
        .collect())
}

fn id_lines<'i>(ids: impl Iterator<Item = &'i ObjectId>) -> Vec<u8> {
    let mut input = Vec::new();
    for id in ids {
        input.extend_from_slice(id.as_str().as_bytes());
        input.push(b'\n');
    }
    input
}

/// Parses `git cat-file --batch` output: for each blob a `<id> blob <size>`
/// line, the content, and a newline. Stops at the first entry it cannot read.
/// The contents borrow `stdout`.
fn parse_batch(mut stdout: &[u8]) -> BTreeMap<ObjectId, &[u8]> {
    let mut blobs = BTreeMap::new();
    while let Some(end) = stdout.iter().position(|byte| *byte == b'\n') {
        let Some((id, size)) = blob_header(&stdout[..end]) else {
            stdout = &stdout[end + 1..];
            continue;
        };
        let Some(content) = stdout.get(end + 1..end + 1 + size) else {
            break;
        };
        blobs.insert(id, content);
        stdout = stdout.get(end + 2 + size..).unwrap_or_default();
    }
    blobs
}

/// Parses a `<id> blob <size>` header line.
fn blob_header(line: &[u8]) -> Option<(ObjectId, usize)> {
    let line = std::str::from_utf8(line).ok()?;
    let mut fields = line.split(' ');
    let id = ObjectId::new(fields.next()?)?;
    if fields.next()? != "blob" {
        return None;
    }
    Some((id, fields.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    /// `git hash-object` of "a\n".
    const A: &str = "78981922613b2afb6025042ff6bd878ac1994e85";
    /// `git hash-object` of "b\n".
    const B: &str = "61780798228d17af2d34fce4cfbdf35556832472";
    /// `git hash-object` of "a\nb\n".
    const AB: &str = "422c2b7ab3b3c668038da977e4e93a5fc623169c";
    /// `git hash-object` of "a\nb" with no final newline.
    const AB_NO_NEWLINE: &str = "0a207c060e61f3b88eaee0a8cd0696f46fb155eb";

    /// A throwaway repository that removes itself.
    struct Repository(PathBuf);

    impl Repository {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("mire-checker-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            let repository = Self(path);
            repository.git(&["init", "--quiet"]);
            repository
        }

        fn git(&self, arguments: &[&str]) -> String {
            let output = Command::new("git")
                .args(arguments)
                .current_dir(&self.0)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }

        /// Stores `content` as a blob and returns its id.
        fn blob(&self, content: &[u8]) -> String {
            let path = self.0.join("blob");
            fs::write(&path, content).unwrap();
            self.git(&["hash-object", "-w", "blob"])
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn changeset(patch: &str) -> Changeset {
        parse_patch(patch.as_bytes(), ChangesetSource::DirectFiles, PatchLimits::default()).unwrap()
    }

    /// Checks the first file with the old blob given instead of read from
    /// Git.
    fn torn(patch: &str, old: &[u8]) -> bool {
        let changeset = changeset(patch);
        match plan_for(&changeset.files()[0]) {
            Plan::Text { old: Some(_), new, hunks } => !matches_hunks(old, new, hunks),
            Plan::Text { old: None, new, hunks } => !matches_hunks(b"", new, hunks),
            Plan::Torn => true,
            Plan::Skip | Plan::Binary { .. } => false,
        }
    }

    fn modified(old: &str, new: &str, removed: &str, added: &str) -> String {
        format!("diff --git a/f b/f\nindex {old}..{new} 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-{removed}\n+{added}\n")
    }

    fn binary(old: &str, new: &str) -> String {
        format!("diff --git a/f b/f\nindex {old}..{new} 100644\nBinary files a/f and b/f differ\n")
    }

    fn findings(torn: &[usize], unchecked: &[usize]) -> Findings {
        Findings { torn: torn.iter().copied().collect(), unchecked: unchecked.iter().copied().collect() }
    }

    #[test]
    fn consistent_patch_passes() {
        assert!(!torn(&modified(A, B, "a", "b"), b"a\n"));
    }

    #[test]
    fn index_id_of_other_content_is_torn() {
        // The `index` line names "a\nb\n" but the hunks give "b\n".
        assert!(torn(&modified(A, AB, "a", "b"), b"a\n"));
    }

    #[test]
    fn same_ids_with_hunks_are_torn() {
        assert!(torn(&modified(A, A, "a", "b"), b"a\n"));
    }

    #[test]
    fn added_file_passes() {
        let patch = format!(
            "diff --git a/f b/f\nnew file mode 100644\nindex 0000000000000000000000000000000000000000..{AB}\n--- /dev/null\n+++ b/f\n@@ -0,0 +1,2 @@\n+a\n+b\n"
        );
        assert!(!torn(&patch, b""));
        assert!(torn(&patch.replace(AB, B), b""));
    }

    #[test]
    fn file_without_final_newline_passes() {
        let patch = format!(
            "diff --git a/f b/f\nindex {A}..{AB_NO_NEWLINE} 100644\n--- a/f\n+++ b/f\n@@ -1 +1,2 @@\n a\n+b\n\\ No newline at end of file\n"
        );
        assert!(!torn(&patch, b"a\n"));
        assert!(torn(&patch.replace(AB_NO_NEWLINE, AB), b"a\n"));
    }

    #[test]
    fn mode_only_and_added_binary_files_are_skipped() {
        let mode = "diff --git a/f b/f\nold mode 100644\nnew mode 100755\n";
        assert!(matches!(plan_for(&changeset(mode).files()[0]), Plan::Skip));
        let added = format!(
            "diff --git a/f b/f\nnew file mode 100644\nindex 0000000000000000000000000000000000000000..{AB}\nBinary files /dev/null and b/f differ\n"
        );
        assert!(matches!(plan_for(&changeset(&added).files()[0]), Plan::Skip));
        assert!(matches!(
            plan_for(&changeset(&binary(A, AB)).files()[0]),
            Plan::Binary { .. }
        ));
    }

    #[test]
    fn only_worktree_comparisons_are_checked() {
        let worktree = |staged| GitOperation::Worktree { staged, paths: Vec::new() };
        let diff = |revisions: &[&str]| GitOperation::Diff {
            revisions: revisions
                .iter()
                .map(|value| mire_core::ByteString::new(value.as_bytes()))
                .collect(),
            paths: Vec::new(),
        };
        assert!(reads_worktree(&worktree(false)));
        assert!(!reads_worktree(&worktree(true)));
        assert!(reads_worktree(&diff(&["HEAD~1"])));
        assert!(!reads_worktree(&diff(&["HEAD~1..HEAD"])));
        assert!(!reads_worktree(&diff(&["HEAD~1", "HEAD"])));
    }

    #[test]
    fn batch_output_parses_blobs_with_any_bytes() {
        let id = ObjectId::new(A).unwrap();
        let output = format!("{A} blob 2\na\n\n{B} missing\n");
        let blobs = parse_batch(output.as_bytes());
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[&id], b"a\n");
    }

    #[test]
    fn checker_reads_old_blobs_from_git_once_per_checked_file() {
        let repository = Repository::new("read");
        let old = repository.blob(b"a\n");
        let other = repository.blob(&b"a\n".repeat(100));
        let patch = format!(
            "{}{}",
            modified(&old, B, "a", "b"),
            modified(&other, AB, "a", "b").replace("/f", "/g")
        );
        let mut checker = Checker::new(&repository.0, 1024);
        assert_eq!(checker.check(&changeset(&patch)).unwrap(), findings(&[1], &[]));
        let first = checker.bytes_read;
        // A retry reads only the old blob of the file still torn.
        assert_eq!(checker.check(&changeset(&patch)).unwrap(), findings(&[1], &[]));
        let retry = checker.bytes_read - first;
        assert!(retry > 200 && retry < first, "first {first}, retry {retry}");
    }

    #[test]
    fn blobs_over_the_limit_are_not_read_and_are_unchecked() {
        let repository = Repository::new("limit");
        let big = "a\n".repeat(600);
        let old = repository.blob(big.as_bytes());
        let patch = format!("diff --git a/f b/f\nindex {old}..{B} 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n");
        let mut checker = Checker::new(&repository.0, 1024);
        assert_eq!(checker.check(&changeset(&patch)).unwrap(), findings(&[], &[0]));
        assert_eq!(checker.bytes_read, 0);

        // A rebuilt side over the limit is not read either, though the old
        // blob fits.
        let small = repository.blob(b"a\n");
        let added = "+x\n".repeat(600);
        let patch =
            format!("diff --git a/f b/f\nindex {small}..{B} 100644\n--- a/f\n+++ b/f\n@@ -1 +1,600 @@\n-a\n{added}");
        assert_eq!(checker.check(&changeset(&patch)).unwrap(), findings(&[], &[0]));
        assert_eq!(checker.bytes_read, 0);
    }

    #[test]
    fn missing_old_blobs_are_unchecked() {
        let repository = Repository::new("missing");
        let mut checker = Checker::new(&repository.0, 1024);
        assert_eq!(
            checker.check(&changeset(&modified(A, B, "a", "b"))).unwrap(),
            findings(&[], &[0])
        );
    }

    #[test]
    fn text_that_turns_binary_is_torn_unless_attributes_say_binary() {
        let repository = Repository::new("binary");
        let text = repository.blob(b"a\n");
        let data = repository.blob(b"a\0b\n");
        let mut checker = Checker::new(&repository.0, 1024);
        assert_eq!(
            checker.check(&changeset(&binary(&text, AB))).unwrap(),
            findings(&[0], &[])
        );
        assert_eq!(
            checker.check(&changeset(&binary(&data, AB))).unwrap(),
            findings(&[], &[])
        );

        fs::write(repository.0.join(".gitattributes"), "f binary\n").unwrap();
        let mut checker = Checker::new(&repository.0, 1024);
        assert_eq!(
            checker.check(&changeset(&binary(&text, AB))).unwrap(),
            findings(&[], &[])
        );
    }
}
