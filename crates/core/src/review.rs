use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroU64;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    BytePath, ByteString, Changeset, ChangesetSource, DiffLine, FileContent, Fingerprint, GitOperation, LineNumber,
    SchemaVersion,
};

/// The review-file JSON schema emitted by this version of Mire.
pub const CURRENT_REVIEW_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 2 };
/// Maximum number of notes accepted in one review file.
pub const MAX_REVIEW_NOTES: usize = 10_000;
/// Maximum UTF-8 byte length of one note body.
pub const MAX_NOTE_BODY_BYTES: usize = 1024 * 1024;

type Result<T> = std::result::Result<T, ReviewError>;

/// Failures produced while constructing or validating review data.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ReviewError {
    /// The serialized schema major cannot be read safely.
    #[error("unsupported review schema major {found}; supported major is {supported}")]
    UnsupportedSchemaMajor { found: u16, supported: u16 },
    /// Review revisions and event sequences must be positive.
    #[error("{field} must be greater than zero")]
    ZeroNumber { field: &'static str },
    /// A line range must run from a lower or equal start to its end.
    #[error("anchor line range starts at {start} after it ends at {end}")]
    ReversedLineRange { start: u64, end: u64 },
    /// The requested path and side do not occur in the captured changeset.
    #[error("anchor path does not identify the requested {side:?} file side")]
    FileSideNotFound { side: AnchorSide },
    /// The requested hunk fingerprint does not occur on the selected file.
    #[error("anchor hunk fingerprint does not identify a hunk on the selected file")]
    HunkNotFound,
    /// More than one hunk has the complete stored anchor identity.
    #[error("anchor path, side, and hunk fingerprint are ambiguous in the captured changeset")]
    AmbiguousAnchor,
    /// The requested line range is not fully present on the selected hunk side.
    #[error("anchor range {start}-{end} is not fully present on the {side:?} hunk side")]
    LineRangeNotFound { side: AnchorSide, start: u64, end: u64 },
    /// The stored content fingerprint does not match the selected lines.
    #[error("anchor content fingerprint does not match the captured changeset")]
    ContentFingerprintMismatch,
    /// No captured file and hunk contain the requested source location.
    #[error("source location is not present on one changed hunk")]
    LocationNotFound,
    /// More than one captured file has the requested path and side.
    #[error("source location matches duplicate file sides")]
    DuplicateLocation,
    /// More than one hunk contains the complete requested range.
    #[error("source location is ambiguous across changed hunks")]
    AmbiguousLocation,
    /// The requested range contains only unchanged context lines.
    #[error("source location contains only unchanged context")]
    ContextOnlyLocation,
    /// Location-based agent and tool input cannot claim human provenance.
    #[error("location-based note input cannot claim human provenance")]
    HumanProvenanceForbidden,
    /// A required identifier or author field is empty or too large.
    #[error("{field} must contain between 1 and {max} UTF-8 bytes")]
    InvalidTextField { field: &'static str, max: usize },
    /// A note body exceeds the review-file resource limit.
    #[error("note body is {actual} bytes; limit is {limit} bytes")]
    NoteBodyTooLarge { actual: usize, limit: usize },
    /// The review contains too many notes.
    #[error("review contains {actual} notes; limit is {limit} notes")]
    TooManyNotes { actual: usize, limit: usize },
    /// Two notes use the same stable identifier.
    #[error("duplicate note identifier {0:?}")]
    DuplicateNoteId(String),
    /// A requested note does not exist in the review.
    #[error("review note {0:?} was not found")]
    NoteNotFound(String),
    /// Event sequences are not strictly increasing.
    #[error("note event sequence {current} does not follow {previous}")]
    EventSequence { previous: u64, current: u64 },
    /// An event refers to a note absent from the current review.
    #[error("note event refers to unknown note {0:?}")]
    UnknownEventNote(String),
    /// A note has no creation event or has more than one.
    #[error("note {note_id:?} must have exactly one creation event")]
    InvalidCreationEvents { note_id: String },
    /// A status event does not follow the preceding recorded status.
    #[error("note {note_id:?} status event expected {expected:?}, found {found:?}")]
    StatusHistory {
        note_id: String,
        expected: NoteStatus,
        found: NoteStatus,
    },
    /// The last event status does not match the stored note.
    #[error("note {note_id:?} ends at {recorded:?}, but its event history ends at {history:?}")]
    FinalStatus {
        note_id: String,
        recorded: NoteStatus,
        history: NoteStatus,
    },
    /// The review revision cannot be incremented again.
    #[error("review revision cannot be incremented beyond {0}")]
    RevisionOverflow(u64),
    /// The event sequence cannot be incremented again.
    #[error("note event sequence cannot be incremented beyond {0}")]
    EventSequenceOverflow(u64),
    /// A source binding must describe the Git operation that produced the capture.
    #[error("review source binding does not match its captured changeset")]
    SourceBindingMismatch,
    /// Review initialization binds comparisons, not single-commit shows.
    #[error("a reloadable review source must be a Git comparison")]
    UnsupportedBindingOperation,
}

impl ReviewError {
    pub const fn error_code(&self) -> &'static str {
        match self {
            ReviewError::UnsupportedSchemaMajor { .. } => "unsupported_schema_major",
            ReviewError::ZeroNumber { .. } => "zero_number",
            ReviewError::ReversedLineRange { .. } => "reversed_line_range",
            ReviewError::FileSideNotFound { .. } => "file_side_not_found",
            ReviewError::HunkNotFound => "hunk_not_found",
            ReviewError::AmbiguousAnchor => "ambiguous_anchor",
            ReviewError::LineRangeNotFound { .. } => "line_range_not_found",
            ReviewError::ContentFingerprintMismatch => "content_fingerprint_mismatch",
            ReviewError::LocationNotFound => "location_not_found",
            ReviewError::DuplicateLocation => "duplicate_location",
            ReviewError::AmbiguousLocation => "ambiguous_location",
            ReviewError::ContextOnlyLocation => "context_only_location",
            ReviewError::HumanProvenanceForbidden => "human_provenance_forbidden",
            ReviewError::InvalidTextField { .. } => "invalid_text_field",
            ReviewError::NoteBodyTooLarge { .. } => "note_body_too_large",
            ReviewError::TooManyNotes { .. } => "too_many_notes",
            ReviewError::DuplicateNoteId(_) => "duplicate_note_id",
            ReviewError::NoteNotFound(_) => "note_not_found",
            ReviewError::EventSequence { .. } => "event_sequence",
            ReviewError::UnknownEventNote(_) => "unknown_event_note",
            ReviewError::InvalidCreationEvents { .. } => "invalid_creation_events",
            ReviewError::StatusHistory { .. } => "status_history",
            ReviewError::FinalStatus { .. } => "final_status",
            ReviewError::RevisionOverflow(_) => "revision_overflow",
            ReviewError::EventSequenceOverflow(_) => "event_sequence_overflow",
            ReviewError::SourceBindingMismatch => "source_binding_mismatch",
            ReviewError::UnsupportedBindingOperation => "unsupported_binding_operation",
        }
    }
}

/// The old or new side selected by an anchor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorSide {
    /// The pre-change side of a file.
    Old,
    /// The post-change side of a file.
    New,
}

impl fmt::Display for AnchorSide {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Old => formatter.write_str("old side"),
            Self::New => formatter.write_str("new side"),
        }
    }
}

/// A review note's current decision state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteStatus {
    /// The note still needs a decision or change.
    Open,
    /// The reviewer considers the note addressed.
    Resolved,
    /// The reviewer closed the note without accepting its claim.
    Dismissed,
    /// The reviewer acknowledged the issue and accepted its remaining risk.
    AcceptedRisk,
}

impl fmt::Display for NoteStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => formatter.write_str("open"),
            Self::Resolved => formatter.write_str("resolved"),
            Self::Dismissed => formatter.write_str("dismissed"),
            Self::AcceptedRisk => formatter.write_str("accepted-risk"),
        }
    }
}

/// The impact assigned to a review note.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteSeverity {
    /// Informational feedback with no identified defect.
    Note,
    /// A low-impact issue or maintainability concern.
    Low,
    /// A material issue that should normally be addressed.
    Medium,
    /// A serious correctness, safety, or operational issue.
    High,
    /// A release-blocking correctness, safety, or data-loss issue.
    Critical,
}

impl fmt::Display for NoteSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note => formatter.write_str("Note"),
            Self::Low => formatter.write_str("Low"),
            Self::Medium => formatter.write_str("Medium"),
            Self::High => formatter.write_str("High"),
            Self::Critical => formatter.write_str("Critical"),
        }
    }
}

/// The intent of an annotation, independent of its impact or decision state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// General review context or feedback.
    #[default]
    Comment,
    /// A concrete defect or risk.
    Defect,
    /// A proposed code or design change.
    Suggestion,
    /// A question that needs an answer.
    Question,
}

impl fmt::Display for AnnotationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Comment => formatter.write_str("comment"),
            Self::Defect => formatter.write_str("defect"),
            Self::Suggestion => formatter.write_str("suggestion"),
            Self::Question => formatter.write_str("question"),
        }
    }
}

/// The broad producer category used for review filtering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorKind {
    /// A person entered the note directly.
    Human,
    /// An autonomous or assisted agent produced the note.
    Agent,
    /// An analyzer or interchange adapter produced the note.
    Tool,
}

impl fmt::Display for AuthorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Human => formatter.write_str("human"),
            Self::Agent => formatter.write_str("agent"),
            Self::Tool => formatter.write_str("tool"),
        }
    }
}

/// How a note entered the review without implying that its claim is verified.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    /// A person entered the note directly in Mire.
    Human,
    /// An external agent produced the note.
    Agent {
        /// Stable producer name or identifier.
        producer: String,
    },
    /// A static or dynamic analysis tool produced the note.
    Analyzer {
        /// Stable tool name or identifier.
        producer: String,
    },
    /// Mire imported the note from another interchange format.
    Interchange {
        /// Format name, such as `sarif`.
        format: String,
        /// Producer recorded by the imported document, when present.
        producer: Option<String>,
    },
}

impl Provenance {
    /// Returns the broad producer category without discarding detailed provenance.
    pub const fn author_kind(&self) -> AuthorKind {
        match self {
            Self::Human => AuthorKind::Human,
            Self::Agent { .. } => AuthorKind::Agent,
            Self::Analyzer { .. } | Self::Interchange { .. } => AuthorKind::Tool,
        }
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Human => formatter.write_str("human"),
            Self::Agent { producer } => write!(formatter, "agent ({producer})"),
            Self::Analyzer { producer } => write!(formatter, "analyzer ({producer})"),
            Self::Interchange { format, producer: Some(producer) } => {
                write!(formatter, "{format} interchange ({producer})")
            }
            Self::Interchange { format, producer: None } => write!(formatter, "{format} interchange"),
        }
    }
}

/// A state transition retained in a review's note history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoteEventKind {
    /// The note was created with this initial state.
    Created {
        /// Initial note state.
        status: NoteStatus,
    },
    /// The note moved from one decision state to another.
    StatusChanged {
        /// State immediately before this event.
        from: NoteStatus,
        /// State immediately after this event.
        to: NoteStatus,
    },
}

/// A positive, monotonically increasing review revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReviewRevision(NonZeroU64);

impl ReviewRevision {
    /// Creates a positive review revision.
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ReviewError::ZeroNumber { field: "review revision" })
    }

    /// Returns the numeric revision.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for ReviewRevision {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// A stable note identifier supplied by the note producer.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    /// Creates a bounded, non-empty note identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_text("note identifier", &value, 128)?;
        Ok(Self(value))
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NoteId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// One rejected note from an otherwise atomic batch import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteImportFailure {
    note_id: NoteId,
    error: ReviewError,
}

impl NoteImportFailure {
    /// Returns the identifier of the rejected note.
    pub const fn note_id(&self) -> &NoteId {
        &self.note_id
    }

    /// Returns why the note was rejected.
    pub const fn error(&self) -> &ReviewError {
        &self.error
    }
}

/// All note failures found while validating one atomic import batch.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("batch import rejected {count} note(s)", count = .failures.len())]
pub struct NoteImportError {
    failures: Vec<NoteImportFailure>,
}

impl NoteImportError {
    /// Returns every rejected note in input order.
    pub fn failures(&self) -> &[NoteImportFailure] {
        &self.failures
    }
}

/// One rejected location-based note in an otherwise atomic batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteApplyFailure {
    input_index: usize,
    error: ReviewError,
}

impl NoteApplyFailure {
    /// Returns the zero-based input position.
    pub const fn input_index(&self) -> usize {
        self.input_index
    }

    /// Returns why this input was rejected.
    pub const fn error(&self) -> &ReviewError {
        &self.error
    }
}

/// All failures found while resolving one atomic location-based batch.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("batch apply rejected {count} note(s)", count = .failures.len())]
pub struct NoteApplyError {
    failures: Vec<NoteApplyFailure>,
}

impl NoteApplyError {
    /// Returns every rejected input in request order.
    pub fn failures(&self) -> &[NoteApplyFailure] {
        &self.failures
    }
}

/// High-level finding input whose durable anchor and identifier Mire creates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteInput {
    path: BytePath,
    side: AnchorSide,
    range: LineRange,
    author: Author,
    provenance: Provenance,
    severity: NoteSeverity,
    annotation_kind: AnnotationKind,
    body: String,
}

impl NoteInput {
    /// Creates a location-based finding request.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: BytePath, side: AnchorSide, range: LineRange, author: Author, provenance: Provenance,
        severity: NoteSeverity, annotation_kind: AnnotationKind, body: String,
    ) -> Result<Self> {
        validate_note_body(&body)?;
        validate_provenance(&provenance)?;
        Ok(Self { path, side, range, author, provenance, severity, annotation_kind, body })
    }
}

/// An inclusive line range on one side of a diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LineRange {
    start: LineNumber,
    end: LineNumber,
}

/// A durable location within one captured changeset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    path: BytePath,
    side: AnchorSide,
    range: LineRange,
    hunk_fingerprint: Fingerprint,
    content_fingerprint: Fingerprint,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl Anchor {
    /// Creates an anchor and computes its content fingerprint from the captured changeset.
    pub fn new(
        changeset: &Changeset, path: BytePath, side: AnchorSide, range: LineRange, hunk_fingerprint: Fingerprint,
    ) -> Result<Self> {
        let content_fingerprint = anchor_content_fingerprint(changeset, &path, side, range, hunk_fingerprint)?;
        Ok(Self { path, side, range, hunk_fingerprint, content_fingerprint, extensions: BTreeMap::new() })
    }

    /// Returns the repository-relative path selected by this anchor.
    pub const fn path(&self) -> &BytePath {
        &self.path
    }

    /// Returns the selected file side.
    pub const fn side(&self) -> AnchorSide {
        self.side
    }

    /// Returns the inclusive selected line range.
    pub const fn range(&self) -> LineRange {
        self.range
    }

    /// Returns the hunk identity captured by this anchor.
    pub const fn hunk_fingerprint(&self) -> Fingerprint {
        self.hunk_fingerprint
    }

    /// Returns the fingerprint of the selected content.
    pub const fn content_fingerprint(&self) -> Fingerprint {
        self.content_fingerprint
    }

    /// Verifies this anchor against a captured changeset.
    pub fn validate(&self, changeset: &Changeset) -> Result<()> {
        let actual = anchor_content_fingerprint(changeset, &self.path, self.side, self.range, self.hunk_fingerprint)?;
        if actual != self.content_fingerprint {
            return Err(ReviewError::ContentFingerprintMismatch);
        }
        Ok(())
    }
}

/// The person or process responsible for a note or event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Author {
    id: String,
    display_name: Option<String>,
}

impl Author {
    /// Creates an attributed author with an optional display name.
    pub fn new(id: impl Into<String>, display_name: Option<String>) -> Result<Self> {
        let id = id.into();
        validate_text("author identifier", &id, 256)?;
        if let Some(name) = &display_name {
            validate_text("author display name", name, 256)?;
        }
        Ok(Self { id, display_name })
    }

    /// Returns the stable author identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable name when one was recorded.
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

/// One anchored review finding and its current state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewNote {
    id: NoteId,
    anchor: Anchor,
    author: Author,
    severity: NoteSeverity,
    #[serde(default)]
    annotation_kind: AnnotationKind,
    status: NoteStatus,
    body: String,
    provenance: Provenance,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl ReviewNote {
    /// Creates one bounded, attributed review note.
    pub fn new(
        id: NoteId, anchor: Anchor, author: Author, severity: NoteSeverity, status: NoteStatus, body: String,
        provenance: Provenance,
    ) -> Result<Self> {
        validate_note_body(&body)?;
        validate_provenance(&provenance)?;
        Ok(Self {
            id,
            anchor,
            author,
            severity,
            annotation_kind: AnnotationKind::default(),
            status,
            body,
            provenance,
            extensions: BTreeMap::new(),
        })
    }

    /// Returns the stable note identifier.
    pub const fn id(&self) -> &NoteId {
        &self.id
    }

    /// Returns the note's durable anchor.
    pub const fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    /// Returns the attributed author.
    pub const fn author(&self) -> &Author {
        &self.author
    }

    /// Returns the assigned severity.
    pub const fn severity(&self) -> NoteSeverity {
        self.severity
    }

    /// Returns the annotation's intent.
    pub const fn annotation_kind(&self) -> AnnotationKind {
        self.annotation_kind
    }

    /// Assigns an annotation intent while preserving the rest of the note.
    pub fn with_annotation_kind(mut self, annotation_kind: AnnotationKind) -> Self {
        self.annotation_kind = annotation_kind;
        self
    }

    /// Returns the current decision state.
    pub const fn status(&self) -> NoteStatus {
        self.status
    }

    /// Returns the note body.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns how the note entered the review.
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// One ordered change to a note's state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NoteEvent {
    sequence: NonZeroU64,
    note_id: NoteId,
    author: Author,
    event: NoteEventKind,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl NoteEvent {
    /// Creates an ordered note event.
    pub fn new(sequence: u64, note_id: NoteId, author: Author, event: NoteEventKind) -> Result<Self> {
        let sequence = NonZeroU64::new(sequence).ok_or(ReviewError::ZeroNumber { field: "note event sequence" })?;
        Ok(Self { sequence, note_id, author, event, extensions: BTreeMap::new() })
    }

    /// Returns this event's ordering key.
    pub const fn sequence(&self) -> u64 {
        self.sequence.get()
    }

    /// Returns the affected note identifier.
    pub const fn note_id(&self) -> &NoteId {
        &self.note_id
    }

    /// Returns the author responsible for this event.
    pub const fn author(&self) -> &Author {
        &self.author
    }

    /// Returns the state change recorded by this event.
    pub const fn event(&self) -> &NoteEventKind {
        &self.event
    }
}

/// A platform filesystem identity used to detect a replaced repository.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilesystemIdentity {
    /// Device and inode identity on Unix filesystems.
    Unix {
        /// Filesystem device number.
        device: u64,
        /// File identity within the device.
        inode: u64,
        /// Compatible fields written by newer schema versions.
        #[serde(default, flatten)]
        extensions: BTreeMap<String, Value>,
    },
    /// Volume and file identity on Windows filesystems.
    Windows {
        /// Filesystem volume serial number.
        volume_serial_number: u32,
        /// File identity within the volume.
        file_index: u64,
        /// Compatible fields written by newer schema versions.
        #[serde(default, flatten)]
        extensions: BTreeMap<String, Value>,
    },
}

impl FilesystemIdentity {
    /// Creates a Unix device and inode identity.
    pub fn unix(device: u64, inode: u64) -> Self {
        Self::Unix { device, inode, extensions: BTreeMap::new() }
    }

    /// Creates a Windows volume and file identity.
    pub fn windows(volume_serial_number: u32, file_index: u64) -> Self {
        Self::Windows { volume_serial_number, file_index, extensions: BTreeMap::new() }
    }
}

/// Stable local identity for the worktree and Git administrative directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    root: ByteString,
    git_directory: ByteString,
    root_filesystem: FilesystemIdentity,
    git_directory_filesystem: FilesystemIdentity,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl RepositoryIdentity {
    /// Creates a repository identity from canonical paths and filesystem identities.
    pub fn new(
        root: ByteString, git_directory: ByteString, root_filesystem: FilesystemIdentity,
        git_directory_filesystem: FilesystemIdentity,
    ) -> Self {
        Self { root, git_directory, root_filesystem, git_directory_filesystem, extensions: BTreeMap::new() }
    }

    /// Returns the canonical worktree root bytes.
    pub const fn root(&self) -> &ByteString {
        &self.root
    }

    /// Returns the canonical Git administrative-directory bytes.
    pub const fn git_directory(&self) -> &ByteString {
        &self.git_directory
    }

    /// Returns the worktree root's filesystem identity.
    pub const fn root_filesystem(&self) -> &FilesystemIdentity {
        &self.root_filesystem
    }

    /// Returns the Git administrative directory's filesystem identity.
    pub const fn git_directory_filesystem(&self) -> &FilesystemIdentity {
        &self.git_directory_filesystem
    }
}

/// The local source information needed to repeat a native comparison safely.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceBinding {
    /// A native Git comparison bound to one repository identity.
    Git {
        /// The repository captured when the review was initialized.
        repository: RepositoryIdentity,
        /// The exact native Git request, including path filters.
        comparison: GitOperation,
        /// Compatible fields written by newer schema versions.
        #[serde(default, flatten)]
        extensions: BTreeMap<String, Value>,
    },
}

impl SourceBinding {
    /// Creates a Git source binding for a worktree or revision comparison.
    pub fn git(repository: RepositoryIdentity, comparison: GitOperation) -> Result<Self> {
        if matches!(comparison, GitOperation::Show { .. }) {
            return Err(ReviewError::UnsupportedBindingOperation);
        }
        Ok(Self::Git { repository, comparison, extensions: BTreeMap::new() })
    }

    /// Returns the bound Git repository identity and comparison request.
    pub const fn git_parts(&self) -> (&RepositoryIdentity, &GitOperation) {
        match self {
            Self::Git { repository, comparison, .. } => (repository, comparison),
        }
    }
}

/// A versioned review around an immutable captured changeset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Review {
    schema_version: SchemaVersion,
    revision: ReviewRevision,
    changeset: Changeset,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_binding: Option<SourceBinding>,
    notes: Vec<ReviewNote>,
    events: Vec<NoteEvent>,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl Review {
    /// Creates and validates a review around an immutable changeset capture.
    pub fn new(
        revision: ReviewRevision, changeset: Changeset, notes: Vec<ReviewNote>, events: Vec<NoteEvent>,
    ) -> Result<Self> {
        let mut review = Self {
            schema_version: CURRENT_REVIEW_SCHEMA_VERSION,
            revision,
            changeset,
            source_binding: None,
            notes,
            events,
            extensions: BTreeMap::new(),
        };
        review.canonicalize();
        review.validate()?;
        Ok(review)
    }

    /// Returns the review-file schema version.
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Returns the current review revision.
    pub const fn revision(&self) -> ReviewRevision {
        self.revision
    }

    /// Returns the immutable captured changeset.
    pub const fn changeset(&self) -> &Changeset {
        &self.changeset
    }

    /// Returns the reloadable local source, or `None` for older and imported reviews.
    pub const fn source_binding(&self) -> Option<&SourceBinding> {
        self.source_binding.as_ref()
    }

    /// Attaches a reloadable source that matches the captured Git operation.
    pub fn with_source_binding(mut self, source_binding: SourceBinding) -> Result<Self> {
        self.source_binding = Some(source_binding);
        self.validate()?;
        Ok(self)
    }

    /// Returns notes in their stored deterministic order.
    pub fn notes(&self) -> &[ReviewNote] {
        &self.notes
    }

    /// Returns note events in sequence order.
    pub fn events(&self) -> &[NoteEvent] {
        &self.events
    }

    /// Validates and appends a batch of notes without partially applying it.
    pub fn import_notes(&self, imported: Vec<ReviewNote>) -> std::result::Result<Self, NoteImportError> {
        if imported.is_empty() {
            return Ok(self.clone());
        }
        let failures = validate_imported_notes(self, &imported);
        if !failures.is_empty() {
            return Err(NoteImportError { failures });
        }

        let first_note_id = imported[0].id.clone();
        let revision = self.revision.get().checked_add(1).ok_or_else(|| NoteImportError {
            failures: vec![NoteImportFailure {
                note_id: first_note_id.clone(),
                error: ReviewError::RevisionOverflow(self.revision.get()),
            }],
        })?;
        let mut sequence = self.events.last().map_or(0, NoteEvent::sequence);
        let mut events = self.events.clone();
        for note in &imported {
            sequence = sequence.checked_add(1).ok_or_else(|| NoteImportError {
                failures: vec![NoteImportFailure {
                    note_id: note.id.clone(),
                    error: ReviewError::EventSequenceOverflow(sequence),
                }],
            })?;
            events.push(
                NoteEvent::new(
                    sequence,
                    note.id.clone(),
                    note.author.clone(),
                    NoteEventKind::Created { status: note.status },
                )
                .map_err(|error| NoteImportError {
                    failures: vec![NoteImportFailure { note_id: note.id.clone(), error }],
                })?,
            );
        }
        let mut notes = self.notes.clone();
        notes.extend(imported);
        let mut review = Self::new(
            ReviewRevision::new(revision).map_err(|error| NoteImportError {
                failures: vec![NoteImportFailure { note_id: first_note_id.clone(), error }],
            })?,
            self.changeset.clone(),
            notes,
            events,
        )
        .map_err(|error| NoteImportError { failures: vec![NoteImportFailure { note_id: first_note_id, error }] })?;
        review.source_binding = self.source_binding.clone();
        review.extensions = self.extensions.clone();
        Ok(review)
    }

    /// Resolves and atomically appends location-based agent or tool findings.
    pub fn apply_notes(&self, inputs: Vec<NoteInput>) -> std::result::Result<Self, NoteApplyError> {
        if inputs.is_empty() {
            return Ok(self.clone());
        }
        let mut failures = Vec::new();
        let mut anchors = Vec::with_capacity(inputs.len());
        for (input_index, input) in inputs.iter().enumerate() {
            match resolve_location(self.changeset(), input) {
                Ok(anchor) => anchors.push(Some(anchor)),
                Err(error) => {
                    failures.push(NoteApplyFailure { input_index, error });
                    anchors.push(None);
                }
            }
        }
        if self.notes.len().saturating_add(inputs.len()) > MAX_REVIEW_NOTES {
            failures.push(NoteApplyFailure {
                input_index: 0,
                error: ReviewError::TooManyNotes {
                    actual: self.notes.len().saturating_add(inputs.len()),
                    limit: MAX_REVIEW_NOTES,
                },
            });
        }
        if !failures.is_empty() {
            return Err(NoteApplyError { failures });
        }

        let mut identifiers = self
            .notes
            .iter()
            .map(|note| note.id.as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let mut next = 1_u64;
        let mut notes = Vec::with_capacity(inputs.len());
        for (input, anchor) in inputs.into_iter().zip(anchors) {
            let id = loop {
                let candidate = format!("note-{next}");
                next = next.checked_add(1).ok_or_else(|| NoteApplyError {
                    failures: vec![NoteApplyFailure { input_index: 0, error: ReviewError::RevisionOverflow(u64::MAX) }],
                })?;
                if identifiers.insert(candidate.clone()) {
                    break NoteId::new(candidate).map_err(|error| NoteApplyError {
                        failures: vec![NoteApplyFailure { input_index: 0, error }],
                    })?;
                }
            };
            let note = ReviewNote::new(
                id,
                anchor.expect("all anchors are present after validation"),
                input.author,
                input.severity,
                NoteStatus::Open,
                input.body,
                input.provenance,
            )
            .map(|note| note.with_annotation_kind(input.annotation_kind))
            .map_err(|error| NoteApplyError { failures: vec![NoteApplyFailure { input_index: notes.len(), error }] })?;
            notes.push(note);
        }
        self.import_notes(notes).map_err(|error| NoteApplyError {
            failures: error
                .failures
                .into_iter()
                .enumerate()
                .map(|(input_index, failure)| NoteApplyFailure { input_index, error: failure.error })
                .collect(),
        })
    }

    /// Replaces the editable content of one note and advances the review revision.
    pub fn edit_note(
        &self, note_id: &NoteId, body: String, severity: NoteSeverity, annotation_kind: AnnotationKind,
    ) -> Result<Self> {
        validate_note_body(&body)?;
        let Some(position) = self.notes.iter().position(|note| note.id == *note_id) else {
            return Err(ReviewError::NoteNotFound(note_id.as_str().to_owned()));
        };
        if self.notes[position].body == body
            && self.notes[position].severity == severity
            && self.notes[position].annotation_kind == annotation_kind
        {
            return Ok(self.clone());
        }
        let mut notes = self.notes.clone();
        notes[position].body = body;
        notes[position].severity = severity;
        notes[position].annotation_kind = annotation_kind;
        self.rebuild(self.next_revision()?, notes, self.events.clone())
    }

    /// Changes one note's disposition and appends an attributed status event.
    pub fn change_note_status(&self, note_id: &NoteId, status: NoteStatus, author: Author) -> Result<Self> {
        let Some(position) = self.notes.iter().position(|note| note.id == *note_id) else {
            return Err(ReviewError::NoteNotFound(note_id.as_str().to_owned()));
        };
        let previous = self.notes[position].status;
        if previous == status {
            return Ok(self.clone());
        }
        let previous_sequence = self.events.last().map_or(0, NoteEvent::sequence);
        let sequence = previous_sequence
            .checked_add(1)
            .ok_or(ReviewError::EventSequenceOverflow(previous_sequence))?;
        let mut notes = self.notes.clone();
        notes[position].status = status;
        let mut events = self.events.clone();
        events.push(NoteEvent::new(
            sequence,
            note_id.clone(),
            author,
            NoteEventKind::StatusChanged { from: previous, to: status },
        )?);
        self.rebuild(self.next_revision()?, notes, events)
    }

    /// Validates limits, anchors, identifiers, attribution, and event history.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version.major != CURRENT_REVIEW_SCHEMA_VERSION.major {
            return Err(ReviewError::UnsupportedSchemaMajor {
                found: self.schema_version.major,
                supported: CURRENT_REVIEW_SCHEMA_VERSION.major,
            });
        }
        if self.notes.len() > MAX_REVIEW_NOTES {
            return Err(ReviewError::TooManyNotes { actual: self.notes.len(), limit: MAX_REVIEW_NOTES });
        }
        if let Some(binding) = &self.source_binding {
            let (_, comparison) = binding.git_parts();
            if matches!(comparison, GitOperation::Show { .. }) {
                return Err(ReviewError::UnsupportedBindingOperation);
            }
            if !matches!(self.changeset.source(), ChangesetSource::Git { operation } if operation == comparison) {
                return Err(ReviewError::SourceBindingMismatch);
            }
        }
        validate_notes(&self.changeset, &self.notes)?;
        validate_events(&self.notes, &self.events)
    }

    fn canonicalize(&mut self) {
        self.notes.sort_by(|left, right| left.id.cmp(&right.id));
        self.events.sort_by_key(NoteEvent::sequence);
    }

    fn next_revision(&self) -> Result<ReviewRevision> {
        let revision = self
            .revision
            .get()
            .checked_add(1)
            .ok_or_else(|| ReviewError::RevisionOverflow(self.revision.get()))?;
        ReviewRevision::new(revision)
    }

    fn rebuild(&self, revision: ReviewRevision, notes: Vec<ReviewNote>, events: Vec<NoteEvent>) -> Result<Self> {
        let mut review = Self::new(revision, self.changeset.clone(), notes, events)?;
        review.source_binding = self.source_binding.clone();
        review.extensions = self.extensions.clone();
        Ok(review)
    }
}

impl<'de> Deserialize<'de> for Review {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReviewWire::deserialize(deserializer)?;
        if wire.schema_version.major != CURRENT_REVIEW_SCHEMA_VERSION.major {
            return Err(de::Error::custom(ReviewError::UnsupportedSchemaMajor {
                found: wire.schema_version.major,
                supported: CURRENT_REVIEW_SCHEMA_VERSION.major,
            }));
        }
        let mut review = Self {
            schema_version: wire.schema_version,
            revision: wire.revision,
            changeset: wire.changeset,
            source_binding: wire.source_binding,
            notes: wire.notes,
            events: wire.events,
            extensions: wire.extensions,
        };
        review.canonicalize();
        review.validate().map_err(de::Error::custom)?;
        Ok(review)
    }
}

#[derive(Deserialize)]
struct ReviewWire {
    schema_version: SchemaVersion,
    revision: ReviewRevision,
    changeset: Changeset,
    #[serde(default)]
    source_binding: Option<SourceBinding>,
    notes: Vec<ReviewNote>,
    events: Vec<NoteEvent>,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl LineRange {
    /// Creates an inclusive, ordered line range.
    pub fn new(start: LineNumber, end: LineNumber) -> Result<Self> {
        if start > end {
            return Err(ReviewError::ReversedLineRange { start: start.get(), end: end.get() });
        }
        Ok(Self { start, end })
    }

    /// Returns the first selected line.
    pub const fn start(self) -> LineNumber {
        self.start
    }

    /// Returns the last selected line.
    pub const fn end(self) -> LineNumber {
        self.end
    }
}

impl<'de> Deserialize<'de> for LineRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            start: LineNumber,
            end: LineNumber,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start, wire.end).map_err(de::Error::custom)
    }
}

fn resolve_location(changeset: &Changeset, input: &NoteInput) -> Result<Anchor> {
    if matches!(input.provenance, Provenance::Human) {
        return Err(ReviewError::HumanProvenanceForbidden);
    }
    let files = changeset
        .files()
        .iter()
        .filter(|file| match input.side {
            AnchorSide::Old => file.old_side().is_some_and(|side| side.path == input.path),
            AnchorSide::New => file.new_side().is_some_and(|side| side.path == input.path),
        })
        .collect::<Vec<_>>();
    let file = match files.as_slice() {
        [] => return Err(ReviewError::LocationNotFound),
        [file] => *file,
        _ => return Err(ReviewError::DuplicateLocation),
    };
    let FileContent::Text { hunks } = file.content() else {
        return Err(ReviewError::LocationNotFound);
    };
    let expected_count = input
        .range
        .end
        .get()
        .checked_sub(input.range.start.get())
        .and_then(|count| count.checked_add(1))
        .ok_or(ReviewError::LineRangeNotFound {
            side: input.side,
            start: input.range.start.get(),
            end: input.range.end.get(),
        })?;
    let matching = hunks
        .iter()
        .filter(|hunk| {
            let selected = hunk
                .lines()
                .iter()
                .filter_map(|line| line_on_side(line, input.side).map(|number| (number, line)))
                .filter(|(number, _)| *number >= input.range.start && *number <= input.range.end)
                .collect::<Vec<_>>();
            selected.len() as u64 == expected_count
        })
        .collect::<Vec<_>>();
    let hunk = match matching.as_slice() {
        [] => return Err(ReviewError::LocationNotFound),
        [hunk] => *hunk,
        _ => return Err(ReviewError::AmbiguousLocation),
    };
    let changed = hunk.lines().iter().any(|line| {
        line_on_side(line, input.side).is_some_and(|number| {
            number >= input.range.start && number <= input.range.end && line.kind() != crate::LineKind::Context
        })
    });
    if !changed {
        return Err(ReviewError::ContextOnlyLocation);
    }
    Anchor::new(
        changeset,
        input.path.clone(),
        input.side,
        input.range,
        hunk.fingerprint(),
    )
}

fn anchor_content_fingerprint(
    changeset: &Changeset, path: &BytePath, side: AnchorSide, range: LineRange, hunk_fingerprint: Fingerprint,
) -> Result<Fingerprint> {
    let matching_files = changeset
        .files()
        .iter()
        .filter(|file| match side {
            AnchorSide::Old => file.old_side().is_some_and(|file_side| file_side.path == *path),
            AnchorSide::New => file.new_side().is_some_and(|file_side| file_side.path == *path),
        })
        .collect::<Vec<_>>();
    if matching_files.is_empty() {
        return Err(ReviewError::FileSideNotFound { side });
    }
    let matching_hunks = matching_files
        .iter()
        .filter_map(|file| match file.content() {
            FileContent::Text { hunks } => Some(hunks),
            FileContent::Binary => None,
        })
        .flatten()
        .filter(|hunk| hunk.fingerprint() == hunk_fingerprint)
        .collect::<Vec<_>>();
    let hunk = match matching_hunks.as_slice() {
        [] => return Err(ReviewError::HunkNotFound),
        [hunk] => *hunk,
        _ => return Err(ReviewError::AmbiguousAnchor),
    };
    let selected = hunk
        .lines()
        .iter()
        .filter_map(|line| line_on_side(line, side).map(|number| (number, line)))
        .filter(|(number, _)| *number >= range.start && *number <= range.end)
        .collect::<Vec<_>>();
    let expected_count = range
        .end
        .get()
        .checked_sub(range.start.get())
        .and_then(|count| count.checked_add(1))
        .ok_or(ReviewError::LineRangeNotFound { side, start: range.start.get(), end: range.end.get() })?;
    if selected.len() as u64 != expected_count {
        return Err(ReviewError::LineRangeNotFound { side, start: range.start.get(), end: range.end.get() });
    }

    let mut digest = Sha256::new();
    digest.update([match side {
        AnchorSide::Old => 0,
        AnchorSide::New => 1,
    }]);
    for (number, line) in selected {
        digest.update(number.get().to_be_bytes());
        digest.update((line.content().len() as u64).to_be_bytes());
        digest.update(line.content());
        digest.update([missing_newline_tag(line)]);
    }
    Ok(Fingerprint::new(digest.finalize().into()))
}

fn line_on_side(line: &DiffLine, side: AnchorSide) -> Option<LineNumber> {
    match side {
        AnchorSide::Old => line.old_line(),
        AnchorSide::New => line.new_line(),
    }
}

fn missing_newline_tag(line: &DiffLine) -> u8 {
    use crate::MissingNewline;

    match line.missing_newline() {
        MissingNewline::None => 0,
        MissingNewline::Old => 1,
        MissingNewline::New => 2,
        MissingNewline::Both => 3,
    }
}

fn validate_notes(changeset: &Changeset, notes: &[ReviewNote]) -> Result<()> {
    let mut identifiers = BTreeSet::new();
    for note in notes {
        if !identifiers.insert(note.id.as_str()) {
            return Err(ReviewError::DuplicateNoteId(note.id.as_str().to_owned()));
        }
        validate_note(changeset, note)?;
    }
    Ok(())
}

fn validate_imported_notes(review: &Review, imported: &[ReviewNote]) -> Vec<NoteImportFailure> {
    let mut identifiers = review
        .notes
        .iter()
        .map(|note| note.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut failures = Vec::new();
    if review.notes.len().saturating_add(imported.len()) > MAX_REVIEW_NOTES {
        if let Some(note) = imported.first() {
            failures.push(NoteImportFailure {
                note_id: note.id.clone(),
                error: ReviewError::TooManyNotes {
                    actual: review.notes.len().saturating_add(imported.len()),
                    limit: MAX_REVIEW_NOTES,
                },
            });
        }
    }
    for note in imported {
        if !identifiers.insert(note.id.as_str()) {
            failures.push(NoteImportFailure {
                note_id: note.id.clone(),
                error: ReviewError::DuplicateNoteId(note.id.as_str().to_owned()),
            });
        }
        if let Err(error) = validate_note(review.changeset(), note) {
            failures.push(NoteImportFailure { note_id: note.id.clone(), error });
        }
    }
    failures
}

fn validate_note(changeset: &Changeset, note: &ReviewNote) -> Result<()> {
    note.anchor.validate(changeset)?;
    validate_text("author identifier", note.author.id(), 256)?;
    if let Some(name) = note.author.display_name() {
        validate_text("author display name", name, 256)?;
    }
    validate_note_body(note.body())?;
    validate_provenance(note.provenance())
}

fn validate_events(notes: &[ReviewNote], events: &[NoteEvent]) -> Result<()> {
    let notes_by_id = notes
        .iter()
        .map(|note| (note.id.as_str(), note))
        .collect::<BTreeMap<_, _>>();
    let mut statuses = BTreeMap::new();
    let mut previous_sequence = 0;
    for event in events {
        let sequence = event.sequence();
        if sequence <= previous_sequence {
            return Err(ReviewError::EventSequence { previous: previous_sequence, current: sequence });
        }
        previous_sequence = sequence;
        let note_id = event.note_id.as_str();
        if !notes_by_id.contains_key(note_id) {
            return Err(ReviewError::UnknownEventNote(note_id.to_owned()));
        }
        validate_text("author identifier", event.author.id(), 256)?;
        if let Some(name) = event.author.display_name() {
            validate_text("author display name", name, 256)?;
        }
        match event.event {
            NoteEventKind::Created { status } => {
                if statuses.insert(note_id, status).is_some() {
                    return Err(ReviewError::InvalidCreationEvents { note_id: note_id.to_owned() });
                }
            }
            NoteEventKind::StatusChanged { from, to } => {
                let Some(current) = statuses.get_mut(note_id) else {
                    return Err(ReviewError::InvalidCreationEvents { note_id: note_id.to_owned() });
                };
                if *current != from {
                    return Err(ReviewError::StatusHistory {
                        note_id: note_id.to_owned(),
                        expected: *current,
                        found: from,
                    });
                }
                *current = to;
            }
        }
    }
    for note in notes {
        let Some(history) = statuses.get(note.id.as_str()).copied() else {
            return Err(ReviewError::InvalidCreationEvents { note_id: note.id.as_str().to_owned() });
        };
        if history != note.status {
            return Err(ReviewError::FinalStatus {
                note_id: note.id.as_str().to_owned(),
                recorded: note.status,
                history,
            });
        }
    }
    Ok(())
}

fn validate_note_body(body: &str) -> Result<()> {
    if body.len() > MAX_NOTE_BODY_BYTES {
        return Err(ReviewError::NoteBodyTooLarge { actual: body.len(), limit: MAX_NOTE_BODY_BYTES });
    }
    Ok(())
}

fn validate_provenance(provenance: &Provenance) -> Result<()> {
    match provenance {
        Provenance::Human => Ok(()),
        Provenance::Agent { producer } | Provenance::Analyzer { producer } => {
            validate_text("provenance producer", producer, 256)
        }
        Provenance::Interchange { format, producer } => {
            validate_text("interchange format", format, 64)?;
            if let Some(producer) = producer {
                validate_text("provenance producer", producer, 256)?;
            }
            Ok(())
        }
    }
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max {
        return Err(ReviewError::InvalidTextField { field, max });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ByteString, ChangesetSource, DiffLine, FileDiff, FileMode, FileSide, FileStatus, Hunk, LineKind, MissingNewline,
    };

    const FINGERPRINT: Fingerprint = Fingerprint::new([0xabu8; 32]);

    #[test]
    fn anchors_bind_path_side_range_hunk_and_content() {
        let changeset = changeset();
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let range = LineRange::new(LineNumber::new(2).unwrap(), LineNumber::new(2).unwrap()).unwrap();
        let anchor = Anchor::new(&changeset, path, AnchorSide::New, range, FINGERPRINT).unwrap();
        assert_eq!(anchor.range().start().get(), 2);
        assert!(anchor.validate(&changeset).is_ok());

        let other = changeset_with_new_content("changed");
        assert_eq!(anchor.validate(&other), Err(ReviewError::ContentFingerprintMismatch));
    }

    #[test]
    fn location_inputs_create_anchors_and_reject_human_provenance() {
        let review = empty_review();
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let line = LineNumber::new(2).unwrap();
        let range = LineRange::new(line, line).unwrap();
        let input = NoteInput::new(
            path.clone(),
            AnchorSide::New,
            range,
            Author::new("agent", None).unwrap(),
            Provenance::Agent { producer: "test-agent".to_owned() },
            NoteSeverity::High,
            AnnotationKind::Defect,
            "Finding".to_owned(),
        )
        .unwrap();
        let updated = review.apply_notes(vec![input]).unwrap();
        assert_eq!(updated.notes()[0].id().as_str(), "note-1");
        assert!(updated.notes()[0].anchor().validate(updated.changeset()).is_ok());

        let human = NoteInput::new(
            path,
            AnchorSide::New,
            range,
            Author::new("human", None).unwrap(),
            Provenance::Human,
            NoteSeverity::Note,
            AnnotationKind::Comment,
            "Claim".to_owned(),
        )
        .unwrap();
        let error = review.apply_notes(vec![human]).unwrap_err();
        assert_eq!(error.failures()[0].error(), &ReviewError::HumanProvenanceForbidden);
    }

    #[test]
    fn review_round_trip_preserves_unknown_fields() {
        let review = empty_review();
        let json = serde_json::to_string(&review)
            .unwrap()
            .replacen("\"minor\":2", "\"minor\":1", 1);
        let json = json.replacen("\"revision\":1", "\"future\":{\"enabled\":true},\"revision\":1", 1);
        let decoded: Review = serde_json::from_str(&json).unwrap();
        assert!(decoded.source_binding().is_none());
        let encoded = serde_json::to_string(&decoded).unwrap();
        assert!(encoded.contains("\"future\":{\"enabled\":true}"));
        assert!(!encoded.contains("source_binding"));
    }

    #[test]
    fn source_binding_round_trip_preserves_unknown_fields() {
        let operation = GitOperation::Worktree { staged: false, paths: vec![BytePath::new(b"src".to_vec()).unwrap()] };
        let changeset = Changeset::new(
            ChangesetSource::Git { operation: operation.clone() },
            Vec::new(),
            FINGERPRINT,
        );
        let repository = RepositoryIdentity::new(
            ByteString::from("/repo"),
            ByteString::from("/repo/.git"),
            FilesystemIdentity::unix(1, 2),
            FilesystemIdentity::unix(1, 3),
        );
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new())
            .unwrap()
            .with_source_binding(SourceBinding::git(repository, operation).unwrap())
            .unwrap();
        let json = serde_json::to_string(&review)
            .unwrap()
            .replacen("\"comparison\":", "\"future_binding\":true,\"comparison\":", 1)
            .replacen("\"git_directory\":", "\"future_repository\":7,\"git_directory\":", 1);
        let decoded: Review = serde_json::from_str(&json).unwrap();
        let encoded = serde_json::to_string(&decoded).unwrap();
        assert!(encoded.contains("\"future_binding\":true"));
        assert!(encoded.contains("\"future_repository\":7"));
    }

    #[test]
    fn unsupported_review_schema_is_rejected() {
        let json = serde_json::to_string(&empty_review())
            .unwrap()
            .replacen("\"major\":1", "\"major\":2", 1);
        let error = serde_json::from_str::<Review>(&json).unwrap_err();
        assert!(error.to_string().contains("unsupported review schema major 2"));
    }

    #[test]
    fn deserialization_rejects_reversed_ranges_and_unknown_statuses() {
        let range_error = serde_json::from_str::<LineRange>(r#"{"start":2,"end":1}"#).unwrap_err();
        assert!(range_error.to_string().contains("starts at 2 after it ends at 1"));

        let status_error = serde_json::from_str::<NoteStatus>(r#""waived""#).unwrap_err();
        assert!(status_error.to_string().contains("unknown variant `waived`"));
    }

    #[test]
    fn review_labels_have_stable_human_readable_text() {
        assert_eq!(AnchorSide::New.to_string(), "new side");
        assert_eq!(NoteStatus::AcceptedRisk.to_string(), "accepted-risk");
        assert_eq!(NoteSeverity::Critical.to_string(), "Critical");
        assert_eq!(Provenance::Human.to_string(), "human");
        assert_eq!(Provenance::Human.author_kind(), AuthorKind::Human);
        assert_eq!(
            Provenance::Agent { producer: "agent".to_owned() }.author_kind(),
            AuthorKind::Agent
        );
        assert_eq!(
            Provenance::Analyzer { producer: "tool".to_owned() }.author_kind(),
            AuthorKind::Tool
        );
        assert_eq!(
            Provenance::Interchange { format: "sarif".to_owned(), producer: Some("scanner".to_owned()) }.to_string(),
            "sarif interchange (scanner)"
        );
    }

    #[test]
    fn note_status_must_match_its_event_history() {
        let changeset = changeset();
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let range = LineRange::new(LineNumber::new(2).unwrap(), LineNumber::new(2).unwrap()).unwrap();
        let anchor = Anchor::new(&changeset, path, AnchorSide::New, range, FINGERPRINT).unwrap();
        let note_id = NoteId::new("note-1").unwrap();
        let author = Author::new("human@example.invalid", Some("Reviewer".to_owned())).unwrap();
        let note = ReviewNote::new(
            note_id.clone(),
            anchor,
            author.clone(),
            NoteSeverity::High,
            NoteStatus::Resolved,
            "The new value needs a guard.".to_owned(),
            Provenance::Human,
        )
        .unwrap();
        let event = NoteEvent::new(1, note_id, author, NoteEventKind::Created { status: NoteStatus::Open }).unwrap();
        let error = Review::new(ReviewRevision::new(1).unwrap(), changeset, vec![note], vec![event]).unwrap_err();
        assert!(matches!(error, ReviewError::FinalStatus { .. }));
    }

    #[test]
    fn note_edits_and_status_changes_advance_revision_without_losing_history() {
        let changeset = changeset();
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let range = LineRange::new(LineNumber::new(2).unwrap(), LineNumber::new(2).unwrap()).unwrap();
        let anchor = Anchor::new(&changeset, path, AnchorSide::New, range, FINGERPRINT).unwrap();
        let note_id = NoteId::new("note-1").unwrap();
        let author = Author::new("reviewer", None).unwrap();
        let note = ReviewNote::new(
            note_id.clone(),
            anchor,
            author.clone(),
            NoteSeverity::Low,
            NoteStatus::Open,
            "Original".to_owned(),
            Provenance::Human,
        )
        .unwrap();
        let review = empty_review().import_notes(vec![note]).unwrap();

        let edited = review
            .edit_note(
                &note_id,
                "Updated".to_owned(),
                NoteSeverity::High,
                AnnotationKind::Defect,
            )
            .unwrap();
        assert_eq!(edited.revision().get(), 3);
        assert_eq!(edited.notes()[0].body(), "Updated");
        assert_eq!(edited.notes()[0].annotation_kind(), AnnotationKind::Defect);
        assert_eq!(edited.events().len(), 1, "body edits do not invent status events");

        let resolved = edited
            .change_note_status(&note_id, NoteStatus::Resolved, author)
            .unwrap();
        assert_eq!(resolved.revision().get(), 4);
        assert_eq!(resolved.notes()[0].status(), NoteStatus::Resolved);
        assert!(matches!(
            resolved.events()[1].event(),
            NoteEventKind::StatusChanged { from: NoteStatus::Open, to: NoteStatus::Resolved }
        ));
        resolved.validate().unwrap();
    }

    #[test]
    fn old_note_json_defaults_annotation_kind_to_comment() {
        let changeset = changeset();
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let range = LineRange::new(LineNumber::new(2).unwrap(), LineNumber::new(2).unwrap()).unwrap();
        let anchor = Anchor::new(&changeset, path, AnchorSide::New, range, FINGERPRINT).unwrap();
        let note = ReviewNote::new(
            NoteId::new("note-1").unwrap(),
            anchor,
            Author::new("reviewer", None).unwrap(),
            NoteSeverity::Note,
            NoteStatus::Open,
            "Body".to_owned(),
            Provenance::Human,
        )
        .unwrap();
        let json = serde_json::to_string(&note).unwrap();
        let without_kind = json.replace("\"annotation_kind\":\"comment\",", "");
        let decoded: ReviewNote = serde_json::from_str(&without_kind).unwrap();
        assert_eq!(decoded.annotation_kind(), AnnotationKind::Comment);
    }

    fn empty_review() -> Review {
        Review::new(ReviewRevision::new(1).unwrap(), changeset(), Vec::new(), Vec::new()).unwrap()
    }

    fn changeset() -> Changeset {
        changeset_with_new_content("new")
    }

    fn changeset_with_new_content(new_content: &str) -> Changeset {
        let path = BytePath::new(b"src/lib.rs".to_vec()).unwrap();
        let side = FileSide { path, mode: Some(FileMode::REGULAR), fingerprint: None };
        let lines = vec![
            DiffLine::new(
                LineKind::Deletion,
                LineNumber::new(2),
                None,
                ByteString::from("old"),
                MissingNewline::None,
            )
            .unwrap(),
            DiffLine::new(
                LineKind::Addition,
                None,
                LineNumber::new(2),
                ByteString::from(new_content),
                MissingNewline::None,
            )
            .unwrap(),
        ];
        let hunk = Hunk::new(2, 1, 2, 1, ByteString::default(), lines, FINGERPRINT).unwrap();
        let file = FileDiff::new(
            FileStatus::Modified,
            Some(side.clone()),
            Some(side),
            None,
            FileContent::Text { hunks: vec![hunk] },
            FINGERPRINT,
        )
        .unwrap();
        Changeset::new(ChangesetSource::DirectFiles, vec![file], FINGERPRINT)
    }
}
