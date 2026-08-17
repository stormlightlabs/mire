use std::fmt;

use crate::{BytePath, Changeset, FileContent, FileDiff, FileMode, FileStatus, Hunk, LineKind, MissingNewline};

/// A changeset that cannot be represented as an applicable text patch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchWriteError {
    /// The review contains binary content, whose payload Mire does not retain.
    BinaryContent {
        /// Repository-relative paths for every binary change.
        paths: Vec<BytePath>,
    },
}

impl fmt::Display for PatchWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryContent { paths } => {
                write!(formatter, "cannot export binary changes for")?;
                for path in paths {
                    write!(formatter, " {}", quote_path(path.as_bytes()))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for PatchWriteError {}

/// Serializes a normalized changeset as a deterministic Git-compatible text patch.
///
/// The writer does not emit Git object IDs. It rejects all binary changes before
/// producing bytes because Mire retains only a binary marker, not payload data.
pub fn write_patch(changeset: &Changeset) -> Result<Vec<u8>, PatchWriteError> {
    let binary_paths = changeset
        .files()
        .iter()
        .filter(|file| matches!(file.content(), FileContent::Binary))
        .filter_map(canonical_path)
        .cloned()
        .collect::<Vec<_>>();
    if !binary_paths.is_empty() {
        return Err(PatchWriteError::BinaryContent { paths: binary_paths });
    }

    let mut output = Vec::new();
    for file in changeset.files() {
        write_file(&mut output, file);
    }
    Ok(output)
}

fn write_file(output: &mut Vec<u8>, file: &FileDiff) {
    let old_path = file.old_side().map(|side| side.path.as_bytes());
    let new_path = file.new_side().map(|side| side.path.as_bytes());
    let diff_old = old_path.or(new_path).expect("validated file diff has a path");
    let diff_new = new_path.or(old_path).expect("validated file diff has a path");

    output.extend_from_slice(b"diff --git ");
    write_prefixed_path(output, b"a/", diff_old);
    output.push(b' ');
    write_prefixed_path(output, b"b/", diff_new);
    output.push(b'\n');

    match file.status() {
        FileStatus::Added => write_addition_metadata(output, file.new_side().expect("added file has a new side")),
        FileStatus::Deleted => write_deletion_metadata(output, file.old_side().expect("deleted file has an old side")),
        FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => write_mode_change_metadata(output, file),
    }

    match file.status() {
        FileStatus::Renamed => write_move_metadata(output, b"rename", old_path, new_path, file.similarity()),
        FileStatus::Copied => write_move_metadata(output, b"copy", old_path, new_path, file.similarity()),
        FileStatus::Added | FileStatus::Deleted | FileStatus::Modified => {}
    }

    let FileContent::Text { hunks } = file.content() else {
        return;
    };
    if hunks.is_empty() {
        return;
    }

    write_file_headers(output, old_path, new_path);
    for hunk in hunks {
        write_hunk(output, hunk);
    }
}

fn write_addition_metadata(output: &mut Vec<u8>, side: &crate::FileSide) {
    if let Some(mode) = side.mode {
        write_mode_line(output, b"new file mode ", mode);
    }
}

fn write_deletion_metadata(output: &mut Vec<u8>, side: &crate::FileSide) {
    if let Some(mode) = side.mode {
        write_mode_line(output, b"deleted file mode ", mode);
    }
}

fn write_mode_change_metadata(output: &mut Vec<u8>, file: &FileDiff) {
    let old_mode = file.old_side().and_then(|side| side.mode);
    let new_mode = file.new_side().and_then(|side| side.mode);
    match (old_mode, new_mode) {
        (Some(old_mode), Some(new_mode)) if old_mode != new_mode => {
            write_mode_line(output, b"old mode ", old_mode);
            write_mode_line(output, b"new mode ", new_mode);
        }
        (Some(mode), Some(_)) => {
            write_mode_line(output, b"old mode ", mode);
            write_mode_line(output, b"new mode ", mode);
        }
        (None, None) | (Some(_), None) | (None, Some(_)) => {}
    }
}

fn write_move_metadata(
    output: &mut Vec<u8>, operation: &[u8], old_path: Option<&[u8]>, new_path: Option<&[u8]>, similarity: Option<u8>,
) {
    if let Some(similarity) = similarity {
        output.extend_from_slice(b"similarity index ");
        output.extend_from_slice(similarity.to_string().as_bytes());
        output.extend_from_slice(b"%\n");
    }
    output.extend_from_slice(operation);
    output.extend_from_slice(b" from ");
    write_path(output, old_path.expect("move has an old path"));
    output.push(b'\n');
    output.extend_from_slice(operation);
    output.extend_from_slice(b" to ");
    write_path(output, new_path.expect("move has a new path"));
    output.push(b'\n');
}

fn write_file_headers(output: &mut Vec<u8>, old_path: Option<&[u8]>, new_path: Option<&[u8]>) {
    output.extend_from_slice(b"--- ");
    match old_path {
        Some(path) => write_prefixed_path(output, b"a/", path),
        None => output.extend_from_slice(b"/dev/null"),
    }
    output.push(b'\n');
    output.extend_from_slice(b"+++ ");
    match new_path {
        Some(path) => write_prefixed_path(output, b"b/", path),
        None => output.extend_from_slice(b"/dev/null"),
    }
    output.push(b'\n');
}

fn write_hunk(output: &mut Vec<u8>, hunk: &Hunk) {
    output.extend_from_slice(
        format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start(),
            hunk.old_line_count(),
            hunk.new_start(),
            hunk.new_line_count()
        )
        .as_bytes(),
    );
    if !hunk.section().is_empty() {
        output.push(b' ');
        output.extend_from_slice(hunk.section());
    }
    output.push(b'\n');
    for line in hunk.lines() {
        output.push(match line.kind() {
            LineKind::Context => b' ',
            LineKind::Addition => b'+',
            LineKind::Deletion => b'-',
        });
        output.extend_from_slice(line.content());
        output.push(b'\n');
        if !matches!(line.missing_newline(), MissingNewline::None) {
            output.extend_from_slice(b"\\ No newline at end of file\n");
        }
    }
}

fn write_mode_line(output: &mut Vec<u8>, prefix: &[u8], mode: FileMode) {
    output.extend_from_slice(prefix);
    output.extend_from_slice(format!("{:06o}", mode.get()).as_bytes());
    output.push(b'\n');
}

fn write_prefixed_path(output: &mut Vec<u8>, prefix: &[u8], path: &[u8]) {
    let mut prefixed = Vec::with_capacity(prefix.len() + path.len());
    prefixed.extend_from_slice(prefix);
    prefixed.extend_from_slice(path);
    write_path(output, &prefixed);
}

fn write_path(output: &mut Vec<u8>, path: &[u8]) {
    output.extend_from_slice(quote_path(path).as_bytes());
}

fn quote_path(path: &[u8]) -> String {
    if path
        .iter()
        .all(|byte| matches!(byte, b'!'..=b'~') && !matches!(byte, b'"' | b'\\'))
    {
        return String::from_utf8(path.to_vec()).expect("printable ASCII is UTF-8");
    }

    let mut quoted = String::from('"');
    for byte in path {
        match byte {
            b'"' => quoted.push_str("\\\""),
            b'\\' => quoted.push_str("\\\\"),
            b'\x07' => quoted.push_str("\\a"),
            b'\x08' => quoted.push_str("\\b"),
            b'\x0c' => quoted.push_str("\\f"),
            b'\n' => quoted.push_str("\\n"),
            b'\r' => quoted.push_str("\\r"),
            b'\t' => quoted.push_str("\\t"),
            b'\x0b' => quoted.push_str("\\v"),
            b' '..=b'~' => quoted.push(char::from(*byte)),
            _ => {
                quoted.push('\\');
                quoted.push(char::from(b'0' + (byte >> 6)));
                quoted.push(char::from(b'0' + ((byte >> 3) & 0o7)));
                quoted.push(char::from(b'0' + (byte & 0o7)));
            }
        }
    }
    quoted.push('"');
    quoted
}

fn canonical_path(file: &FileDiff) -> Option<&BytePath> {
    file.new_side().or(file.old_side()).map(|side| &side.path)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::{ChangesetSource, PatchLimits, parse_patch};

    #[test]
    fn normalized_text_changes_round_trip_through_the_writer() {
        let original = parse_patch(
            include_bytes!("../../tests/fixtures/patches/git_metadata.patch"),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let without_binary = Changeset::new(
            original.source().clone(),
            original
                .files()
                .iter()
                .filter(|file| !matches!(file.content(), FileContent::Binary))
                .cloned()
                .collect(),
            original.fingerprint(),
        );
        let patch = write_patch(&without_binary).unwrap();
        let reparsed = parse_patch(&patch, without_binary.source().clone(), PatchLimits::default()).unwrap();
        assert_eq!(without_fingerprints(&without_binary), without_fingerprints(&reparsed));
    }

    #[test]
    fn empty_text_changes_round_trip_through_the_writer() {
        let changeset = parse_patch(
            b"diff --git a/empty-new.txt b/empty-new.txt\nnew file mode 100644\ndiff --git a/empty-old.txt b/empty-old.txt\ndeleted file mode 100644\n",
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let patch = write_patch(&changeset).unwrap();
        assert!(
            !patch
                .windows(b"--- /dev/null".len())
                .any(|window| window == b"--- /dev/null")
        );
        let reparsed = parse_patch(&patch, changeset.source().clone(), PatchLimits::default()).unwrap();
        assert_eq!(without_fingerprints(&changeset), without_fingerprints(&reparsed));
    }

    #[test]
    fn preserves_crlf_missing_newlines_and_quoted_paths() {
        let changeset = parse_patch(
            include_bytes!("../../tests/fixtures/patches/text_edges.patch"),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let patch = write_patch(&changeset).unwrap();
        assert!(
            patch
                .windows(b"\\303\\274nicode.txt".len())
                .any(|window| window == b"\\303\\274nicode.txt")
        );
        assert!(
            patch
                .windows(b"+na\xc3\xafve\n".len())
                .any(|window| window == b"+na\xc3\xafve\n")
        );
        assert!(patch.ends_with(b"\\ No newline at end of file\n"));
        let reparsed = parse_patch(&patch, changeset.source().clone(), PatchLimits::default()).unwrap();
        assert_eq!(without_fingerprints(&changeset), without_fingerprints(&reparsed));
    }

    #[test]
    fn rejects_every_binary_file_before_returning_output() {
        let changeset = parse_patch(
            include_bytes!("../../tests/fixtures/patches/git_metadata.patch"),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let error = write_patch(&changeset).unwrap_err();
        assert_eq!(
            error,
            PatchWriteError::BinaryContent { paths: vec![BytePath::new(b"image.bin".to_vec()).unwrap()] }
        );
        assert_eq!(error.to_string(), "cannot export binary changes for image.bin");
    }

    fn without_fingerprints(changeset: &Changeset) -> Value {
        let mut value = serde_json::to_value(changeset).unwrap();
        remove_fingerprints(&mut value);
        value
    }

    fn remove_fingerprints(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("fingerprint");
                for value in object.values_mut() {
                    remove_fingerprints(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    remove_fingerprints(value);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
}
