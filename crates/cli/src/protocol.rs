use std::ffi::OsStr;
use std::io::{self, Write};

use mire_core::{
    BytePath, Changeset, FileContent, FileDiff, NoteImportError, NoteStatus, Provenance, Review, ReviewNote,
    SchemaVersion,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

/// Protocol schema emitted by non-interactive review commands.
pub const CURRENT_PROTOCOL_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 1 };

type Result<T> = std::result::Result<T, ProtocolError>;

/// A bounded context selection.
#[derive(Clone, Copy, Debug)]
pub enum ContextSelection<'a> {
    /// File and hunk identities without source lines.
    Manifest,
    /// The complete normalized patch capture.
    Patch,
    /// One complete normalized file diff.
    File(&'a OsStr),
}

/// Failures while producing or consuming the offline review protocol.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// The input belongs to an incompatible protocol generation.
    #[error("unsupported protocol schema major {found}; supported major is {supported}")]
    UnsupportedSchemaMajor { found: u16, supported: u16 },
    /// A requested file is absent from the captured changeset.
    #[error("captured changeset has no file matching {0:?}")]
    FileNotFound(Vec<u8>),
    /// An explicitly requested context payload exceeds its caller-supplied bound.
    #[error("context output is {actual} bytes; requested limit is {limit} bytes")]
    ContextTooLarge { actual: usize, limit: usize },
    /// A protocol response could not be encoded.
    #[error("cannot serialize protocol output: {0}")]
    Serialize(serde_json::Error),
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ContextPayload<'a> {
    Manifest {
        files: Vec<FileManifest<'a>>,
        notes: Vec<NoteSummary<'a>>,
    },
    Patch {
        changeset: &'a Changeset,
    },
    File {
        file: &'a FileDiff,
    },
}

/// A schema-versioned batch of notes accepted by `notes import`.
#[derive(Debug, Deserialize)]
pub struct NoteBatch {
    schema_version: SchemaVersion,
    notes: Vec<ReviewNote>,
}

impl NoteBatch {
    /// Validates the batch schema and returns its notes.
    pub fn into_notes(self) -> Result<Vec<ReviewNote>> {
        if self.schema_version.major != CURRENT_PROTOCOL_SCHEMA_VERSION.major {
            return Err(ProtocolError::UnsupportedSchemaMajor {
                found: self.schema_version.major,
                supported: CURRENT_PROTOCOL_SCHEMA_VERSION.major,
            });
        }
        Ok(self.notes)
    }
}

#[derive(Serialize)]
struct ContextDocument<'a> {
    schema_version: SchemaVersion,
    review_revision: u64,
    payload: ContextPayload<'a>,
}

#[derive(Serialize)]
struct FileManifest<'a> {
    old_path: Option<&'a BytePath>,
    new_path: Option<&'a BytePath>,
    status: mire_core::FileStatus,
    fingerprint: mire_core::Fingerprint,
    hunks: Vec<HunkManifest>,
}

impl<'a> From<&'a FileDiff> for FileManifest<'a> {
    fn from(file: &'a FileDiff) -> Self {
        let hunks = match file.content() {
            FileContent::Text { hunks } => hunks
                .iter()
                .map(|hunk| HunkManifest {
                    old_start: hunk.old_start(),
                    old_line_count: hunk.old_line_count(),
                    new_start: hunk.new_start(),
                    new_line_count: hunk.new_line_count(),
                    fingerprint: hunk.fingerprint(),
                })
                .collect(),
            FileContent::Binary => Vec::new(),
        };
        Self {
            old_path: file.old_side().map(|side| &side.path),
            new_path: file.new_side().map(|side| &side.path),
            status: file.status(),
            fingerprint: file.fingerprint(),
            hunks,
        }
    }
}

#[derive(Serialize)]
struct HunkManifest {
    old_start: u64,
    old_line_count: u64,
    new_start: u64,
    new_line_count: u64,
    fingerprint: mire_core::Fingerprint,
}

#[derive(Serialize)]
struct NoteSummary<'a> {
    id: &'a str,
    path: &'a BytePath,
    side: mire_core::AnchorSide,
    start: u64,
    end: u64,
    severity: mire_core::NoteSeverity,
    annotation_kind: mire_core::AnnotationKind,
    status: NoteStatus,
    provenance: &'a Provenance,
}

impl<'a> From<&'a ReviewNote> for NoteSummary<'a> {
    fn from(note: &'a ReviewNote) -> Self {
        Self {
            id: note.id().as_str(),
            path: note.anchor().path(),
            side: note.anchor().side(),
            start: note.anchor().range().start().get(),
            end: note.anchor().range().end().get(),
            severity: note.severity(),
            annotation_kind: note.annotation_kind(),
            status: note.status(),
            provenance: note.provenance(),
        }
    }
}

#[derive(Serialize)]
struct NoteDocument<'a> {
    schema_version: SchemaVersion,
    review_revision: u64,
    changeset_fingerprint: mire_core::Fingerprint,
    notes: &'a [ReviewNote],
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
    total: usize,
}

impl BoundedBuffer {
    fn new(limit: usize) -> Self {
        Self { bytes: Vec::with_capacity(limit.min(64 * 1024)), limit, total: 0 }
    }
}

impl Write for BoundedBuffer {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        self.bytes.extend_from_slice(&input[..input.len().min(remaining)]);
        self.total = self.total.saturating_add(input.len());
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serializes compact or explicitly requested review context.
pub fn context_json(review: &Review, selection: ContextSelection<'_>, limit: Option<usize>) -> Result<Vec<u8>> {
    let payload = match selection {
        ContextSelection::Manifest => ContextPayload::Manifest {
            files: review.changeset().files().iter().map(FileManifest::from).collect(),
            notes: review.notes().iter().map(NoteSummary::from).collect(),
        },
        ContextSelection::Patch => ContextPayload::Patch { changeset: review.changeset() },
        ContextSelection::File(path) => {
            let path = BytePath::new(path.as_encoded_bytes())
                .map_err(|_| ProtocolError::FileNotFound(path.as_encoded_bytes().to_vec()))?;
            let file = review
                .changeset()
                .files()
                .iter()
                .find(|file| {
                    file.old_side().is_some_and(|side| side.path == path)
                        || file.new_side().is_some_and(|side| side.path == path)
                })
                .ok_or_else(|| ProtocolError::FileNotFound(path.as_bytes().to_vec()))?;
            ContextPayload::File { file }
        }
    };
    let document = ContextDocument {
        schema_version: CURRENT_PROTOCOL_SCHEMA_VERSION,
        review_revision: review.revision().get(),
        payload,
    };
    match limit {
        Some(limit) => bounded_json(&document, limit),
        None => {
            let mut bytes = serde_json::to_vec(&document).map_err(ProtocolError::Serialize)?;
            bytes.push(b'\n');
            Ok(bytes)
        }
    }
}

/// Serializes deterministic, schema-versioned note JSON.
pub fn notes_json(review: &Review) -> Result<Vec<u8>> {
    let document = NoteDocument {
        schema_version: CURRENT_PROTOCOL_SCHEMA_VERSION,
        review_revision: review.revision().get(),
        changeset_fingerprint: review.changeset().fingerprint(),
        notes: review.notes(),
    };
    let mut bytes = serde_json::to_vec(&document).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Serializes a machine-readable atomic-import result.
pub fn import_result_json(review: &Review, imported: usize) -> Result<Vec<u8>> {
    let value = json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": "imported",
        "imported": imported,
        "review_revision": review.revision().get(),
    });
    let mut bytes = serde_json::to_vec(&value).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Serializes every rejected note from an atomic import.
pub fn import_error_json(error: &NoteImportError) -> Result<Vec<u8>> {
    let failures = error
        .failures()
        .iter()
        .map(|failure| {
            json!({
                "note_id": failure.note_id().as_str(),
                "code": failure.error().error_code(),
                "error": failure.error().to_string(),
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": "rejected",
        "failures": failures,
    });
    let mut bytes = serde_json::to_vec(&value).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Renders review notes as standalone Markdown.
pub fn notes_markdown(review: &Review) -> Vec<u8> {
    let mut output = format!(
        "# Mire review notes\n\nReview revision: {}\n\nChangeset: `{}`\n",
        review.revision().get(),
        review.changeset().fingerprint()
    );
    if review.notes().is_empty() {
        output.push_str("\nNo review notes.\n");
        return output.into_bytes();
    }
    for note in review.notes() {
        let anchor = note.anchor();
        output.push_str(&format!(
            "\n## {}: {}\n\n- Kind: `{}`\n- Status: `{}`\n- Author: {}\n- Provenance: {}\n- Location: `{}` ({}, lines {}–{})\n\n{}\n",
            note.severity(),
            note.id().as_str(),
            note.annotation_kind(),
            note.status(),
            note.author().display_name().unwrap_or(note.author().id()),
            note.provenance(),
            String::from_utf8_lossy(anchor.path().as_bytes()),
            anchor.side(),
            anchor.range().start().get(),
            anchor.range().end().get(),
            note.body()
        ));
    }
    output.into_bytes()
}

fn bounded_json(value: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    let mut output = BoundedBuffer::new(limit);
    serde_json::to_writer(&mut output, value).map_err(ProtocolError::Serialize)?;
    let actual = output.total.saturating_add(1);
    if actual > limit {
        return Err(ProtocolError::ContextTooLarge { actual, limit });
    }
    output.bytes.push(b'\n');
    Ok(output.bytes)
}
