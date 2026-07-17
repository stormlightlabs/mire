//! Normalized, byte-preserving changeset types used by every Mire frontend.

mod model;
mod patch;

pub use model::{
    BytePath, BytePathError, ByteString, CURRENT_SCHEMA_VERSION, Changeset, ChangesetSource, DiffLine, FileContent,
    FileDiff, FileMode, FileSide, FileStatus, Fingerprint, GitOperation, Hunk, LineKind, LineNumber, MissingNewline,
    ModelError, SchemaVersion,
};
pub use patch::{DEFAULT_MAX_PATCH_BYTES, PatchError, PatchInput, PatchLimits, parse_patch};
