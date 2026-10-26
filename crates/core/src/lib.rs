//! Normalized, byte-preserving changeset types used by every Mire frontend.

mod model;
mod patch;
mod review;

pub use model::{
    BytePath, BytePathError, ByteString, CURRENT_SCHEMA_VERSION, Changeset, ChangesetSource, DiffLine, FileContent,
    FileDiff, FileMode, FileSide, FileStatus, Fingerprint, GitOperation, Hunk, LineKind, LineNumber, MissingNewline,
    ModelError, SchemaVersion,
};
pub use patch::{
    DEFAULT_MAX_PATCH_BYTES, PatchError, PatchInput, PatchLimits, PatchWriteError, parse_patch, write_patch,
};
pub use review::{
    Anchor, AnchorSide, AnnotationKind, Author, AuthorKind, CURRENT_REVIEW_SCHEMA_VERSION, FilesystemIdentity,
    LineRange, MAX_NOTE_BODY_BYTES, MAX_REVIEW_DESCRIPTION_CHARS, MAX_REVIEW_NOTES, MAX_REVIEW_TITLE_BYTES,
    NoteApplyError, NoteApplyFailure, NoteEvent, NoteEventKind, NoteId, NoteImportError, NoteImportFailure, NoteInput,
    NoteSeverity, NoteStatus, Provenance, ReanchorCandidate, ReanchorEvidence, ReanchorOutcome, RepositoryIdentity,
    Review, ReviewError, ReviewNote, ReviewRevision, SourceBinding, normalize_description, validate_review_metadata,
};
