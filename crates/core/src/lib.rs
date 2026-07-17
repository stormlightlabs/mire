//! Normalized, byte-preserving changeset types used by every Mire frontend.

mod model;
mod patch;
mod review;

pub use model::{
    BytePath, BytePathError, ByteString, CURRENT_SCHEMA_VERSION, Changeset, ChangesetSource, DiffLine, FileContent,
    FileDiff, FileMode, FileSide, FileStatus, Fingerprint, GitOperation, Hunk, LineKind, LineNumber, MissingNewline,
    ModelError, SchemaVersion,
};
pub use patch::{DEFAULT_MAX_PATCH_BYTES, PatchError, PatchInput, PatchLimits, parse_patch};
pub use review::{
    Anchor, AnchorSide, AnnotationKind, Author, AuthorKind, CURRENT_REVIEW_SCHEMA_VERSION, LineRange,
    MAX_NOTE_BODY_BYTES, MAX_REVIEW_NOTES, NoteEvent, NoteEventKind, NoteId, NoteImportError, NoteImportFailure,
    NoteSeverity, NoteStatus, Provenance, Review, ReviewError, ReviewNote, ReviewRevision,
};
