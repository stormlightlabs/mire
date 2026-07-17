use std::cmp::Ordering;
use std::num::NonZeroU64;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

/// The changeset JSON schema emitted by this version of Mire.
pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 0 };

/// A major/minor version for a serialized Mire schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Changes when readers must reject an incompatible document.
    pub major: u16,
    /// Changes when compatible fields or behavior are added.
    pub minor: u16,
}

/// Arbitrary bytes that must survive normalization without text conversion.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteString(Vec<u8>);

impl ByteString {
    /// Creates a byte string without interpreting its encoding.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Returns the original bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the value and returns the original bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl From<&str> for ByteString {
    fn from(value: &str) -> Self {
        Self::new(value.as_bytes())
    }
}

/// A validated, repository-relative path stored as raw bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BytePath(Vec<u8>);

impl BytePath {
    /// Validates and constructs a repository-relative byte path.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, BytePathError> {
        let bytes = bytes.into();
        validate_path(&bytes)?;
        Ok(Self(bytes))
    }

    /// Returns the original path bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the path and returns its bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl<'de> Deserialize<'de> for BytePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Self::new(bytes).map_err(de::Error::custom)
    }
}

/// Why raw bytes cannot represent a safe repository-relative path.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BytePathError {
    /// The path contains no components.
    #[error("path is empty")]
    Empty,
    /// The path begins at a filesystem root.
    #[error("path must be repository-relative")]
    Absolute,
    /// The path contains an empty component.
    #[error("path contains an empty component")]
    EmptyComponent,
    /// The path contains `.` or `..` as a component.
    #[error("path contains a traversal component")]
    TraversalComponent,
    /// The path contains a NUL byte.
    #[error("path contains a NUL byte")]
    Nul,
}

/// A stable 256-bit identity for normalized content.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// Creates a fingerprint from an already-computed digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Serialize for Fingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut encoded = [0_u8; 64];
        for (index, byte) in self.0.iter().copied().enumerate() {
            encoded[index * 2] = hex_digit(byte >> 4);
            encoded[index * 2 + 1] = hex_digit(byte & 0x0f);
        }
        let encoded = std::str::from_utf8(&encoded).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(encoded)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        decode_fingerprint(&encoded).map_err(de::Error::custom)
    }
}

/// Describes where a normalized changeset came from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangesetSource {
    /// Patch bytes read from a file or standard input.
    Patch {
        /// Optional byte-preserving input label; it is metadata, not a path to open.
        label: Option<ByteString>,
    },
    /// Output captured from a native Git operation.
    Git {
        /// The Git operation used to produce the patch.
        operation: GitOperation,
    },
    /// A comparison between two explicitly supplied files.
    DirectFiles,
}

/// The Git operation whose output produced a changeset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitOperation {
    /// A worktree comparison, optionally restricted to the index.
    Worktree {
        /// Whether the comparison was made against the staged index.
        staged: bool,
    },
    /// A revision comparison. Revisions retain their original bytes and order.
    Diff {
        /// Revision arguments passed to Git without shell interpretation.
        revisions: Vec<ByteString>,
        /// Repository-relative path filters.
        paths: Vec<BytePath>,
    },
    /// A single commit or object shown by Git.
    Show {
        /// Revision argument passed to Git without shell interpretation.
        revision: ByteString,
        /// Repository-relative path filters.
        paths: Vec<BytePath>,
    },
}

/// A normalized changeset with canonical file and hunk ordering.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Changeset {
    schema_version: SchemaVersion,
    source: ChangesetSource,
    files: Vec<FileDiff>,
    fingerprint: Fingerprint,
}

impl Changeset {
    /// Builds a changeset and canonicalizes file and hunk order.
    pub fn new(source: ChangesetSource, mut files: Vec<FileDiff>, fingerprint: Fingerprint) -> Self {
        for file in &mut files {
            file.sort_hunks();
        }
        files.sort_by(compare_files);
        Self { schema_version: CURRENT_SCHEMA_VERSION, source, files, fingerprint }
    }

    /// Returns the schema version serialized with this changeset.
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Returns the captured source description.
    pub const fn source(&self) -> &ChangesetSource {
        &self.source
    }

    /// Returns files in canonical byte-path order.
    pub fn files(&self) -> &[FileDiff] {
        &self.files
    }

    /// Returns the changeset fingerprint.
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

#[derive(Deserialize)]
struct ChangesetWire {
    schema_version: SchemaVersion,
    source: ChangesetSource,
    files: Vec<FileDiff>,
    fingerprint: Fingerprint,
}

impl<'de> Deserialize<'de> for Changeset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ChangesetWire::deserialize(deserializer)?;
        if wire.schema_version.major != CURRENT_SCHEMA_VERSION.major {
            return Err(de::Error::custom(ModelError::UnsupportedSchemaMajor {
                found: wire.schema_version.major,
                supported: CURRENT_SCHEMA_VERSION.major,
            }));
        }
        let mut changeset = Self::new(wire.source, wire.files, wire.fingerprint);
        changeset.schema_version = wire.schema_version;
        Ok(changeset)
    }
}

/// How a file relates to its old and new sides.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    /// The new side was added.
    Added,
    /// The old side was deleted.
    Deleted,
    /// Both sides use the same path.
    Modified,
    /// The old side moved to a new path.
    Renamed,
    /// The new side was copied from the old side.
    Copied,
}

/// A path and mode on one side of a file diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileSide {
    /// Repository-relative raw path.
    pub path: BytePath,
    /// File mode reported by the source, if available.
    pub mode: Option<FileMode>,
    /// Fingerprint of the complete side content, if captured.
    pub fingerprint: Option<Fingerprint>,
}

/// A Git-compatible six-digit file mode retained as an octal JSON string.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FileMode(u32);

impl FileMode {
    /// A regular non-executable file.
    pub const REGULAR: Self = Self(0o100644);
    /// A regular executable file.
    pub const EXECUTABLE: Self = Self(0o100755);
    /// A symbolic link.
    pub const SYMLINK: Self = Self(0o120000);
    /// A Git submodule entry.
    pub const GITLINK: Self = Self(0o160000);

    /// Creates a mode when it fits Git's six octal digits.
    pub fn new(mode: u32) -> Result<Self, ModelError> {
        if mode <= 0o777777 { Ok(Self(mode)) } else { Err(ModelError::InvalidFileMode(mode)) }
    }

    /// Returns the numeric mode.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Serialize for FileMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:06o}", self.0))
    }
}

impl<'de> Deserialize<'de> for FileMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() != 6 || !encoded.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
            return Err(de::Error::custom("file mode must contain six octal digits"));
        }
        let mode = u32::from_str_radix(&encoded, 8).map_err(de::Error::custom)?;
        Self::new(mode).map_err(de::Error::custom)
    }
}

/// Text hunks or an explicit binary marker for one file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileContent {
    /// A textual file represented by zero or more hunks.
    Text {
        /// Hunks in old/new source order.
        hunks: Vec<Hunk>,
    },
    /// A binary file marker. Binary payloads are intentionally not retained.
    Binary,
}

/// A normalized file-level diff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileDiff {
    status: FileStatus,
    old: Option<FileSide>,
    new: Option<FileSide>,
    similarity: Option<u8>,
    content: FileContent,
    fingerprint: Fingerprint,
}

impl FileDiff {
    /// Builds a file diff after validating side and similarity invariants.
    pub fn new(
        status: FileStatus, old: Option<FileSide>, new: Option<FileSide>, similarity: Option<u8>, content: FileContent,
        fingerprint: Fingerprint,
    ) -> Result<Self, ModelError> {
        validate_file_sides(status, old.as_ref(), new.as_ref())?;
        if let Some(value) = similarity {
            if value > 100 || !matches!(status, FileStatus::Renamed | FileStatus::Copied) {
                return Err(ModelError::InvalidSimilarity { status, value });
            }
        }
        Ok(Self { status, old, new, similarity, content, fingerprint })
    }

    /// Returns the file status.
    pub const fn status(&self) -> FileStatus {
        self.status
    }

    /// Returns the old side, absent for additions.
    pub const fn old_side(&self) -> Option<&FileSide> {
        self.old.as_ref()
    }

    /// Returns the new side, absent for deletions.
    pub const fn new_side(&self) -> Option<&FileSide> {
        self.new.as_ref()
    }

    /// Returns rename or copy similarity as a percentage.
    pub const fn similarity(&self) -> Option<u8> {
        self.similarity
    }

    /// Returns the normalized text or binary content marker.
    pub const fn content(&self) -> &FileContent {
        &self.content
    }

    /// Returns the file diff fingerprint.
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    fn canonical_path(&self) -> &BytePath {
        self.new
            .as_ref()
            .or(self.old.as_ref())
            .map(|side| &side.path)
            .expect("validated file diff has at least one side")
    }

    fn sort_hunks(&mut self) {
        if let FileContent::Text { hunks } = &mut self.content {
            hunks.sort_by_key(|hunk| (hunk.old_start, hunk.new_start));
        }
    }
}

#[derive(Deserialize)]
struct FileDiffWire {
    status: FileStatus,
    old: Option<FileSide>,
    new: Option<FileSide>,
    similarity: Option<u8>,
    content: FileContent,
    fingerprint: Fingerprint,
}

impl<'de> Deserialize<'de> for FileDiff {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FileDiffWire::deserialize(deserializer)?;
        Self::new(
            wire.status,
            wire.old,
            wire.new,
            wire.similarity,
            wire.content,
            wire.fingerprint,
        )
        .map_err(de::Error::custom)
    }
}

/// A one-based line number on one side of a diff.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineNumber(NonZeroU64);

impl LineNumber {
    /// Creates a line number, returning `None` for zero.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the one-based number.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The role of a line in a text hunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Present on both sides.
    Context,
    /// Present only on the new side.
    Addition,
    /// Present only on the old side.
    Deletion,
}

/// Which side or sides lack a final line terminator after this line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingNewline {
    /// Both applicable sides include a final line terminator.
    #[default]
    None,
    /// Only the old side lacks a final line terminator.
    Old,
    /// Only the new side lacks a final line terminator.
    New,
    /// Both sides lack a final line terminator.
    Both,
}

/// One byte-preserving line within a hunk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiffLine {
    kind: LineKind,
    old_line: Option<LineNumber>,
    new_line: Option<LineNumber>,
    content: ByteString,
    missing_newline: MissingNewline,
}

impl DiffLine {
    /// Builds a line and validates which side numbers are present.
    pub fn new(
        kind: LineKind, old_line: Option<LineNumber>, new_line: Option<LineNumber>, content: ByteString,
        missing_newline: MissingNewline,
    ) -> Result<Self, ModelError> {
        let valid_numbers = match kind {
            LineKind::Context => old_line.is_some() && new_line.is_some(),
            LineKind::Addition => old_line.is_none() && new_line.is_some(),
            LineKind::Deletion => old_line.is_some() && new_line.is_none(),
        };
        let valid_marker = match kind {
            LineKind::Context => {
                matches!(missing_newline, MissingNewline::None | MissingNewline::Both)
            }
            LineKind::Addition => {
                matches!(missing_newline, MissingNewline::None | MissingNewline::New)
            }
            LineKind::Deletion => {
                matches!(missing_newline, MissingNewline::None | MissingNewline::Old)
            }
        };
        if !valid_numbers || !valid_marker {
            return Err(ModelError::InvalidLineSides(kind));
        }
        Ok(Self { kind, old_line, new_line, content, missing_newline })
    }

    /// Returns the line role.
    pub const fn kind(&self) -> LineKind {
        self.kind
    }

    /// Returns the old-side number when the line exists there.
    pub const fn old_line(&self) -> Option<LineNumber> {
        self.old_line
    }

    /// Returns the new-side number when the line exists there.
    pub const fn new_line(&self) -> Option<LineNumber> {
        self.new_line
    }

    /// Returns bytes without the diff prefix or framing LF.
    ///
    /// A trailing CR is retained so CRLF source content remains distinguishable.
    pub fn content(&self) -> &[u8] {
        self.content.as_bytes()
    }

    /// Returns the missing-final-newline marker.
    pub const fn missing_newline(&self) -> MissingNewline {
        self.missing_newline
    }
}

#[derive(Deserialize)]
struct DiffLineWire {
    kind: LineKind,
    old_line: Option<LineNumber>,
    new_line: Option<LineNumber>,
    content: ByteString,
    missing_newline: MissingNewline,
}

impl<'de> Deserialize<'de> for DiffLine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DiffLineWire::deserialize(deserializer)?;
        Self::new(
            wire.kind,
            wire.old_line,
            wire.new_line,
            wire.content,
            wire.missing_newline,
        )
        .map_err(de::Error::custom)
    }
}

/// A contiguous changed region with old and new source coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Hunk {
    old_start: u64,
    old_line_count: u64,
    new_start: u64,
    new_line_count: u64,
    section: ByteString,
    lines: Vec<DiffLine>,
    fingerprint: Fingerprint,
}

impl Hunk {
    /// Builds a hunk and verifies the header counts against its lines.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        old_start: u64, old_line_count: u64, new_start: u64, new_line_count: u64, section: ByteString,
        lines: Vec<DiffLine>, fingerprint: Fingerprint,
    ) -> Result<Self, ModelError> {
        let actual_old = lines
            .iter()
            .filter(|line| !matches!(line.kind, LineKind::Addition))
            .count() as u64;
        let actual_new = lines
            .iter()
            .filter(|line| !matches!(line.kind, LineKind::Deletion))
            .count() as u64;
        if actual_old != old_line_count || actual_new != new_line_count {
            return Err(ModelError::HunkLineCount {
                expected_old: old_line_count,
                actual_old,
                expected_new: new_line_count,
                actual_new,
            });
        }
        validate_line_sequence(old_start, new_start, &lines)?;
        Ok(Self { old_start, old_line_count, new_start, new_line_count, section, lines, fingerprint })
    }

    /// Returns the old-side start, which may be zero for an addition.
    pub const fn old_start(&self) -> u64 {
        self.old_start
    }

    /// Returns the number of old-side lines.
    pub const fn old_line_count(&self) -> u64 {
        self.old_line_count
    }

    /// Returns the new-side start, which may be zero for a deletion.
    pub const fn new_start(&self) -> u64 {
        self.new_start
    }

    /// Returns the number of new-side lines.
    pub const fn new_line_count(&self) -> u64 {
        self.new_line_count
    }

    /// Returns the optional raw section heading after the hunk coordinates.
    pub fn section(&self) -> &[u8] {
        self.section.as_bytes()
    }

    /// Returns lines in patch order.
    pub fn lines(&self) -> &[DiffLine] {
        &self.lines
    }

    /// Returns the hunk fingerprint.
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

#[derive(Deserialize)]
struct HunkWire {
    old_start: u64,
    old_line_count: u64,
    new_start: u64,
    new_line_count: u64,
    section: ByteString,
    lines: Vec<DiffLine>,
    fingerprint: Fingerprint,
}

impl<'de> Deserialize<'de> for Hunk {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HunkWire::deserialize(deserializer)?;
        Self::new(
            wire.old_start,
            wire.old_line_count,
            wire.new_start,
            wire.new_line_count,
            wire.section,
            wire.lines,
            wire.fingerprint,
        )
        .map_err(de::Error::custom)
    }
}

/// A model invariant rejected during construction or deserialization.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ModelError {
    /// The serialized schema major cannot be read safely.
    #[error("unsupported changeset schema major {found}; supported major is {supported}")]
    UnsupportedSchemaMajor { found: u16, supported: u16 },
    /// The file status does not agree with its old and new sides.
    #[error("old and new sides do not match {0:?} status")]
    InvalidFileSides(FileStatus),
    /// Similarity is out of range or attached to the wrong status.
    #[error("similarity {value} is invalid for {status:?} status")]
    InvalidSimilarity { status: FileStatus, value: u8 },
    /// A file mode is too large for six octal digits.
    #[error("file mode {0:o} is too large")]
    InvalidFileMode(u32),
    /// A line's old/new identities do not agree with its kind or marker.
    #[error("old and new line identities do not match {0:?}")]
    InvalidLineSides(LineKind),
    /// Hunk line totals do not agree with its header.
    #[error("hunk counts old={expected_old},new={expected_new} do not match lines old={actual_old},new={actual_new}")]
    HunkLineCount {
        expected_old: u64,
        actual_old: u64,
        expected_new: u64,
        actual_new: u64,
    },
    /// A line number is not the next identity described by the hunk header.
    #[error("hunk line {line_index} has non-sequential old or new identity")]
    HunkLineSequence {
        /// Zero-based line index within the hunk.
        line_index: usize,
    },
}

fn validate_path(bytes: &[u8]) -> Result<(), BytePathError> {
    if bytes.is_empty() {
        return Err(BytePathError::Empty);
    }
    if bytes[0] == b'/' || bytes.get(1) == Some(&b':') {
        return Err(BytePathError::Absolute);
    }
    if bytes.contains(&0) {
        return Err(BytePathError::Nul);
    }
    for component in bytes.split(|byte| *byte == b'/') {
        if component.is_empty() {
            return Err(BytePathError::EmptyComponent);
        }
        if component == b"." || component == b".." {
            return Err(BytePathError::TraversalComponent);
        }
    }
    Ok(())
}

fn validate_file_sides(status: FileStatus, old: Option<&FileSide>, new: Option<&FileSide>) -> Result<(), ModelError> {
    let valid = match status {
        FileStatus::Added => old.is_none() && new.is_some(),
        FileStatus::Deleted => old.is_some() && new.is_none(),
        FileStatus::Modified => {
            matches!((old, new), (Some(old), Some(new)) if old.path == new.path)
        }
        FileStatus::Renamed | FileStatus::Copied => {
            matches!((old, new), (Some(old), Some(new)) if old.path != new.path)
        }
    };
    if valid { Ok(()) } else { Err(ModelError::InvalidFileSides(status)) }
}

fn validate_line_sequence(old_start: u64, new_start: u64, lines: &[DiffLine]) -> Result<(), ModelError> {
    let mut expected_old = old_start;
    let mut expected_new = new_start;
    for (line_index, line) in lines.iter().enumerate() {
        let old_matches = line.old_line.map(LineNumber::get) == expected_side(line.kind, expected_old, true);
        let new_matches = line.new_line.map(LineNumber::get) == expected_side(line.kind, expected_new, false);
        if !old_matches || !new_matches {
            return Err(ModelError::HunkLineSequence { line_index });
        }
        if !matches!(line.kind, LineKind::Addition) {
            expected_old = expected_old.saturating_add(1);
        }
        if !matches!(line.kind, LineKind::Deletion) {
            expected_new = expected_new.saturating_add(1);
        }
    }
    Ok(())
}

const fn expected_side(kind: LineKind, number: u64, old: bool) -> Option<u64> {
    match (kind, old) {
        (LineKind::Addition, true) | (LineKind::Deletion, false) => None,
        _ => Some(number),
    }
}

fn compare_files(left: &FileDiff, right: &FileDiff) -> Ordering {
    left.canonical_path().cmp(right.canonical_path()).then_with(|| {
        left.old
            .as_ref()
            .map(|side| &side.path)
            .cmp(&right.old.as_ref().map(|side| &side.path))
    })
}

const fn hex_digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        10..=15 => b'a' + value - 10,
        _ => b'?',
    }
}

fn decode_fingerprint(encoded: &str) -> Result<Fingerprint, &'static str> {
    if encoded.len() != 64 {
        return Err("fingerprint must contain 64 hexadecimal characters");
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Ok(Fingerprint::new(bytes))
}

const fn hex_value(value: u8) -> Result<u8, &'static str> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("fingerprint contains a non-hexadecimal character"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: Fingerprint = Fingerprint::new([0xabu8; 32]);

    #[test]
    fn byte_paths_reject_unsafe_components() {
        assert_eq!(BytePath::new(b"".to_vec()), Err(BytePathError::Empty));
        assert_eq!(BytePath::new(b"/etc/passwd".to_vec()), Err(BytePathError::Absolute));
        assert_eq!(
            BytePath::new(b"../secret".to_vec()),
            Err(BytePathError::TraversalComponent)
        );
        assert_eq!(BytePath::new(b"a//b".to_vec()), Err(BytePathError::EmptyComponent));
        assert_eq!(BytePath::new(b"bad\0name".to_vec()), Err(BytePathError::Nul));
        assert!(BytePath::new(b"src/non-utf8-\xff.rs".to_vec()).is_ok());
    }

    #[test]
    fn file_diff_rejects_sides_that_disagree_with_status() {
        let side = file_side("same.txt", FileMode::REGULAR);
        let result = FileDiff::new(
            FileStatus::Added,
            Some(side.clone()),
            Some(side),
            None,
            FileContent::Text { hunks: Vec::new() },
            FINGERPRINT,
        );
        assert_eq!(result, Err(ModelError::InvalidFileSides(FileStatus::Added)));
    }

    #[test]
    fn changeset_json_is_versioned_canonical_and_byte_preserving() {
        let later_hunk = hunk(10, 10, b"later".to_vec());
        let earlier_hunk = hunk(2, 2, vec![0xff, b'a']);
        let z_file = modified_file("z.txt", vec![later_hunk, earlier_hunk]);
        let a_file = modified_file("a.txt", Vec::new());
        let changeset = Changeset::new(
            ChangesetSource::Patch { label: Some(ByteString::from("stdin")) },
            vec![z_file, a_file],
            FINGERPRINT,
        );

        let first = serde_json::to_string(&changeset).expect("changeset serializes");
        let second = serde_json::to_string(&changeset).expect("changeset serializes repeatedly");
        assert_eq!(first, second);
        assert!(first.starts_with("{\"schema_version\":{\"major\":1,\"minor\":0}"));
        assert!(first.find("[97,46,116,120,116]") < first.find("[122,46,116,120,116]"));
        assert!(first.find("[255,97]") < first.find("[108,97,116,101,114]"));

        let decoded: Changeset = serde_json::from_str(&first).expect("changeset round trips");
        assert_eq!(decoded, changeset);
    }

    #[test]
    fn deserialization_rejects_unsupported_schema_versions() {
        let changeset = Changeset::new(ChangesetSource::DirectFiles, Vec::new(), FINGERPRINT);
        let json = serde_json::to_string(&changeset)
            .expect("changeset serializes")
            .replacen("\"major\":1", "\"major\":2", 1);

        let error = serde_json::from_str::<Changeset>(&json).expect_err("major two is unsupported");
        assert!(error.to_string().contains("unsupported changeset schema major 2"));
    }

    #[test]
    fn hunk_counts_are_validated() {
        let line = DiffLine::new(
            LineKind::Addition,
            None,
            LineNumber::new(1),
            ByteString::from("new"),
            MissingNewline::New,
        )
        .expect("addition has a new-side identity");
        let result = Hunk::new(0, 1, 1, 1, ByteString::default(), vec![line], FINGERPRINT);
        assert_eq!(
            result,
            Err(ModelError::HunkLineCount { expected_old: 1, actual_old: 0, expected_new: 1, actual_new: 1 })
        );
    }

    fn file_side(path: &str, mode: FileMode) -> FileSide {
        FileSide {
            path: BytePath::new(path.as_bytes()).expect("test path is valid"),
            mode: Some(mode),
            fingerprint: None,
        }
    }

    fn modified_file(path: &str, hunks: Vec<Hunk>) -> FileDiff {
        let side = file_side(path, FileMode::REGULAR);
        FileDiff::new(
            FileStatus::Modified,
            Some(side.clone()),
            Some(side),
            None,
            FileContent::Text { hunks },
            FINGERPRINT,
        )
        .expect("test file is valid")
    }

    fn hunk(old_start: u64, new_start: u64, content: Vec<u8>) -> Hunk {
        let line = DiffLine::new(
            LineKind::Context,
            LineNumber::new(old_start),
            LineNumber::new(new_start),
            ByteString::new(content),
            MissingNewline::None,
        )
        .expect("context line has both identities");
        Hunk::new(
            old_start,
            1,
            new_start,
            1,
            ByteString::default(),
            vec![line],
            FINGERPRINT,
        )
        .expect("test hunk counts match")
    }
}
