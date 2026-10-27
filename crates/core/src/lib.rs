//! Normalized, byte-preserving changeset types used by every Mire frontend.

mod model;
mod patch;
mod review;

pub use model::{
    BytePath, BytePathError, ByteString, CURRENT_SCHEMA_VERSION, Changeset, ChangesetSource, DiffLine, FileContent,
    FileDiff, FileMode, FileSide, FileStatus, Fingerprint, GitOperation, Hunk, LineKind, LineNumber, MissingNewline,
    ModelError, ObjectId, SchemaVersion,
};
pub use patch::{
    DEFAULT_MAX_PATCH_BYTES, PatchError, PatchInput, PatchLimits, PatchWriteError, parse_patch, write_patch,
};
pub use review::{
    Anchor, AnchorSide, AnnotationKind, Author, AuthorKind, CURRENT_REVIEW_SCHEMA_VERSION, Checkpoint, CheckpointRef,
    CheckpointRow, FilesystemIdentity, HunkLocation, LineRange, MAX_HISTORY_BYTES, MAX_NOTE_BODY_BYTES,
    MAX_PATCH_COPY_BYTES, MAX_REVIEW_DESCRIPTION_CHARS, MAX_REVIEW_NOTES, MAX_REVIEW_TITLE_BYTES, MAX_STEP_TITLE_BYTES,
    MAX_VIEWED_MARKS, MAX_VIEWED_READERS, MAX_WALKTHROUGH_ANCHORS, MAX_WALKTHROUGH_STEPS, MAX_WALKTHROUGH_TEXT_BYTES,
    NoteApplyError, NoteApplyFailure, NoteEvent, NoteEventKind, NoteId, NoteImportError, NoteImportFailure, NoteInput,
    NoteSeverity, NoteStatus, PatchCopy, Provenance, ReanchorCandidate, ReanchorEvidence, ReanchorOutcome,
    RepositoryIdentity, Review, ReviewError, ReviewNote, ReviewRevision, SourceBinding, StepAnchor, StepId, StepInput,
    StepLocation, ViewedMap, ViewedMark, Walkthrough, WalkthroughCoverage, WalkthroughInput, WalkthroughStep,
    normalize_description, validate_review_metadata,
};
