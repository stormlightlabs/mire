//! Rebuilds a file's new side from its old side and hunks, and checks it
//! against a Git blob id.

use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::{Hunk, LineKind, MissingNewline, ObjectId};

/// Rebuilds the new side of a text file from its old content and its hunks.
///
/// An added file has no old side; pass empty content and the hunks become
/// the whole file. Returns `None` when the hunks do not fit `old`: a hunk
/// starts past its end or overlaps the previous one.
pub fn rebuild_new_side(old: &[u8], hunks: &[Hunk]) -> Option<Vec<u8>> {
    let old_lines = old.split_inclusive(|byte| *byte == b'\n').collect::<Vec<_>>();
    let mut rebuilt = Vec::with_capacity(old.len());
    let mut cursor = 0usize;
    for hunk in hunks {
        let start = if hunk.old_line_count() == 0 { hunk.old_start() } else { hunk.old_start().checked_sub(1)? };
        let start = usize::try_from(start).ok()?;
        if start < cursor || start > old_lines.len() {
            return None;
        }
        old_lines[cursor..start]
            .iter()
            .for_each(|line| rebuilt.extend_from_slice(line));
        cursor = start;
        for line in hunk.lines() {
            match line.kind() {
                LineKind::Deletion => cursor += 1,
                LineKind::Context | LineKind::Addition => {
                    if line.kind() == LineKind::Context {
                        cursor += 1;
                    }
                    rebuilt.extend_from_slice(line.content());
                    if !matches!(line.missing_newline(), MissingNewline::New | MissingNewline::Both) {
                        rebuilt.push(b'\n');
                    }
                }
            }
        }
        if cursor > old_lines.len() {
            return None;
        }
    }
    old_lines[cursor..]
        .iter()
        .for_each(|line| rebuilt.extend_from_slice(line));
    Some(rebuilt)
}

/// Reports whether `content` hashes to the Git blob id `object`.
///
/// The id's length picks the hash: 40 hex digits for SHA-1, 64 for SHA-256.
pub fn blob_matches(content: &[u8], object: &ObjectId) -> bool {
    let header = format!("blob {}\0", content.len());
    let digest = if object.as_str().len() == 64 {
        hex(&Sha256::new().chain_update(&header).chain_update(content).finalize())
    } else {
        hex(&Sha1::new().chain_update(&header).chain_update(content).finalize())
    };
    digest == object.as_str()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangesetSource, FileContent, PatchLimits, parse_patch};

    /// `git hash-object` of "one\ntwo\nthree\n".
    const OLD: &str = "4cb29ea38f70d7c61b2a3a25b02e3bdf44905402";

    fn hunks(patch: &str) -> Vec<Hunk> {
        let changeset = parse_patch(patch.as_bytes(), ChangesetSource::DirectFiles, PatchLimits::default()).unwrap();
        match changeset.files()[0].content() {
            FileContent::Text { hunks } => hunks.clone(),
            FileContent::Binary => panic!("text patch"),
        }
    }

    fn id(text: &str) -> ObjectId {
        ObjectId::new(text).unwrap()
    }

    #[test]
    fn blob_ids_match_git_for_both_hash_formats() {
        assert!(blob_matches(b"", &id("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391")));
        assert!(blob_matches(
            b"",
            &id("473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813")
        ));
        assert!(blob_matches(b"one\ntwo\nthree\n", &id(OLD)));
        assert!(blob_matches(
            b"a\nc",
            &id("7abf9b1ff18e1729379e879860ae5d59a982cdd6539d066deaad54825d2f518a")
        ));
        assert!(!blob_matches(b"one\ntwo\n", &id(OLD)));
    }

    #[test]
    fn modified_file_rebuilds_to_its_new_blob() {
        let patch = "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let rebuilt = rebuild_new_side(b"one\ntwo\nthree\n", &hunks(patch)).unwrap();
        assert_eq!(rebuilt, b"one\nTWO\nthree\n");
    }

    #[test]
    fn untouched_lines_outside_hunks_are_kept() {
        let old = (1..=20).map(|line| format!("{line}\n")).collect::<String>();
        let patch = "--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-2\n+two\n@@ -15,0 +16,1 @@\n+inserted\n";
        let rebuilt = rebuild_new_side(old.as_bytes(), &hunks(patch)).unwrap();
        let expected = old.replace("\n2\n", "\ntwo\n").replace("\n15\n", "\n15\ninserted\n");
        assert_eq!(rebuilt, expected.as_bytes());
    }

    #[test]
    fn added_file_is_its_hunks() {
        let patch = "--- /dev/null\n+++ b/f\n@@ -0,0 +1,2 @@\n+a\n+b\n";
        assert_eq!(rebuild_new_side(b"", &hunks(patch)).unwrap(), b"a\nb\n");
    }

    #[test]
    fn missing_final_newline_is_rebuilt_without_one() {
        let patch = "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n\\ No newline at end of file\n";
        assert_eq!(rebuild_new_side(b"a\nb\n", &hunks(patch)).unwrap(), b"a\nc");
        let patch = "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+c\n";
        assert_eq!(rebuild_new_side(b"a\nb", &hunks(patch)).unwrap(), b"a\nc\n");
    }

    #[test]
    fn hunk_past_the_old_side_does_not_rebuild() {
        let patch = "--- a/f\n+++ b/f\n@@ -5,1 +5,1 @@\n-x\n+y\n";
        assert_eq!(rebuild_new_side(b"a\n", &hunks(patch)), None);
    }
}
