//! Detects a torn worktree capture.
//!
//! `git diff` hashes a changed worktree file for the `index` line and reads
//! it again for the hunks. A write between the two reads gives a patch whose
//! new blob id names one content while its hunks show another. Rebuilding the
//! new side from the old blob and the hunks, and hashing it, finds that
//! mismatch.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

use mire_core::{
    BytePath, Changeset, FileContent, FileDiff, FileMode, GitOperation, Hunk, ObjectId, blob_matches, rebuild_new_side,
};

use super::{BLOB_CHECK_CHUNK, MAX_BLOB_CHECK_LINE_BYTES, Result, ensure_success, run_git_with_input};

/// Old-blob bytes read by one `git cat-file --batch` run. A blob larger than
/// this is read alone.
const MAX_BLOB_READ_BYTES: usize = 64 * 1024 * 1024;

/// One text file whose new side can be rebuilt and hashed.
struct Check<'a> {
    path: &'a BytePath,
    old: Option<&'a ObjectId>,
    new: &'a ObjectId,
    hunks: &'a [Hunk],
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

/// Returns the path of the first text file whose hunks do not rebuild to its
/// new blob id, or `None` when every checked file matches.
///
/// It reads the old blobs from the repository at `root` in bounded batches.
/// Binary files, submodules, and sides without a full blob id are skipped,
/// as is a file whose old blob Git does not hold.
pub fn torn_file(root: &Path, changeset: &Changeset) -> Result<Option<BytePath>> {
    let mut checks = Vec::new();
    for file in changeset.files() {
        match check_for(file) {
            Some(Ok(check)) => checks.push(check),
            Some(Err(path)) => return Ok(Some(path.clone())),
            None => {}
        }
    }
    if let Some(check) = checks.iter().find(|check| check.old.is_none() && is_torn(b"", check)) {
        return Ok(Some(check.path.clone()));
    }
    let old_objects = checks
        .iter()
        .filter_map(|check| check.old)
        .cloned()
        .collect::<BTreeSet<_>>();
    let sizes = blob_sizes(root, &old_objects)?;
    for batch in size_batches(&sizes) {
        let blobs = read_blobs(root, &batch, &sizes)?;
        for check in &checks {
            if let Some(old) = check.old.and_then(|old| blobs.get(old))
                && is_torn(old, check)
            {
                return Ok(Some(check.path.clone()));
            }
        }
    }
    Ok(None)
}

/// Returns the check for one file, `None` when the file cannot be checked,
/// or the file's path when its sides already contradict its hunks.
fn check_for(file: &FileDiff) -> Option<std::result::Result<Check<'_>, &BytePath>> {
    let FileContent::Text { hunks } = file.content() else {
        return None;
    };
    let new = file.new_side()?;
    let new_object = new.object.as_ref()?;
    let gitlink = |mode: Option<FileMode>| mode == Some(FileMode::GITLINK);
    if gitlink(new.mode) || file.old_side().is_some_and(|old| gitlink(old.mode)) {
        return None;
    }
    let old = match file.old_side() {
        None => None,
        Some(old) => {
            let old_object = old.object.as_ref()?;
            if old_object == new_object {
                return (!hunks.is_empty()).then_some(Err(&new.path));
            }
            Some(old_object)
        }
    };
    Some(Ok(Check { path: &new.path, old, new: new_object, hunks }))
}

fn is_torn(old: &[u8], check: &Check<'_>) -> bool {
    !rebuild_new_side(old, check.hunks).is_some_and(|content| blob_matches(&content, check.new))
}

/// Groups ids so each `git cat-file --batch` run reads at most
/// [`MAX_BLOB_READ_BYTES`] of content, or one larger blob.
fn size_batches(sizes: &BTreeMap<ObjectId, usize>) -> Vec<Vec<ObjectId>> {
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut bytes = 0usize;
    for (id, size) in sizes {
        if !batch.is_empty() && (bytes.saturating_add(*size) > MAX_BLOB_READ_BYTES || batch.len() == BLOB_CHECK_CHUNK) {
            batches.push(std::mem::take(&mut batch));
            bytes = 0;
        }
        batch.push(id.clone());
        bytes = bytes.saturating_add(*size);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// Returns the size of each id the repository holds as a blob.
fn blob_sizes(root: &Path, objects: &BTreeSet<ObjectId>) -> Result<BTreeMap<ObjectId, usize>> {
    let ids = objects.iter().collect::<Vec<_>>();
    let mut sizes = BTreeMap::new();
    for chunk in ids.chunks(BLOB_CHECK_CHUNK) {
        let input = id_lines(chunk.iter().copied());
        let arguments = [OsString::from("cat-file"), OsString::from("--batch-check")];
        let output = run_git_with_input(Some(root), &arguments, input, chunk.len() * MAX_BLOB_CHECK_LINE_BYTES)?;
        ensure_success("check objects", &output, &[0])?;
        for line in output.stdout.split(|byte| *byte == b'\n') {
            if let Some((id, size)) = blob_header(line) {
                sizes.insert(id, size);
            }
        }
    }
    Ok(sizes)
}

/// Reads the blobs `ids` with one `git cat-file --batch` run, whose output
/// limit follows from the sizes `git cat-file --batch-check` reported.
fn read_blobs(root: &Path, ids: &[ObjectId], sizes: &BTreeMap<ObjectId, usize>) -> Result<BTreeMap<ObjectId, Vec<u8>>> {
    let limit = ids
        .iter()
        .map(|id| sizes[id].saturating_add(MAX_BLOB_CHECK_LINE_BYTES + 1))
        .fold(0usize, usize::saturating_add);
    let arguments = [OsString::from("cat-file"), OsString::from("--batch")];
    let output = run_git_with_input(Some(root), &arguments, id_lines(ids.iter()), limit)?;
    ensure_success("read objects", &output, &[0])?;
    Ok(parse_batch(&output.stdout))
}

fn id_lines<'a>(ids: impl Iterator<Item = &'a ObjectId>) -> Vec<u8> {
    let mut input = Vec::new();
    for id in ids {
        input.extend_from_slice(id.as_str().as_bytes());
        input.push(b'\n');
    }
    input
}

/// Parses `git cat-file --batch` output: for each blob a `<id> blob <size>`
/// line, the content, and a newline. Stops at the first entry it cannot read.
fn parse_batch(mut stdout: &[u8]) -> BTreeMap<ObjectId, Vec<u8>> {
    let mut blobs = BTreeMap::new();
    while let Some(end) = stdout.iter().position(|byte| *byte == b'\n') {
        let Some((id, size)) = blob_header(&stdout[..end]) else {
            stdout = &stdout[end + 1..];
            continue;
        };
        let Some(content) = stdout.get(end + 1..end + 1 + size) else {
            break;
        };
        blobs.insert(id, content.to_vec());
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

    fn changeset(patch: &str) -> Changeset {
        parse_patch(patch.as_bytes(), ChangesetSource::DirectFiles, PatchLimits::default()).unwrap()
    }

    /// Runs the check with the old blob given instead of read from Git.
    fn torn(patch: &str, old: &[u8]) -> bool {
        let changeset = changeset(patch);
        let check = match check_for(&changeset.files()[0]) {
            Some(Ok(check)) => check,
            Some(Err(_)) => return true,
            None => return false,
        };
        is_torn(if check.old.is_some() { old } else { b"" }, &check)
    }

    fn modified(new: &str, removed: &str, added: &str) -> String {
        format!("diff --git a/f b/f\nindex {A}..{new} 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-{removed}\n+{added}\n")
    }

    #[test]
    fn consistent_patch_passes() {
        assert!(!torn(&modified(B, "a", "b"), b"a\n"));
    }

    #[test]
    fn index_id_of_other_content_is_torn() {
        // The `index` line names "a\nb\n" but the hunks give "b\n".
        assert!(torn(&modified(AB, "a", "b"), b"a\n"));
    }

    #[test]
    fn same_ids_with_hunks_are_torn() {
        assert!(torn(&modified(A, "a", "b"), b"a\n"));
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
    fn binary_and_mode_only_files_are_skipped() {
        let binary = format!("diff --git a/f b/f\nindex {A}..{AB} 100644\nBinary files a/f and b/f differ\n");
        assert!(check_for(&changeset(&binary).files()[0]).is_none());
        let mode = "diff --git a/f b/f\nold mode 100644\nnew mode 100755\n";
        assert!(check_for(&changeset(mode).files()[0]).is_none());
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
}
