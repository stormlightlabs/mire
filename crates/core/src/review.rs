use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{BytePath, Changeset, DiffLine, FileContent, Fingerprint, LineNumber, SchemaVersion};

/// The review-file JSON schema emitted by this version of Mire.
pub const CURRENT_REVIEW_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 0 };
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
}

/// The old or new side selected by an anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorSide {
    /// The pre-change side of a file.
    Old,
    /// The post-change side of a file.
    New,
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
        Ok(Self { id, anchor, author, severity, status, body, provenance, extensions: BTreeMap::new() })
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

/// A versioned review around an immutable captured changeset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Review {
    schema_version: SchemaVersion,
    revision: ReviewRevision,
    changeset: Changeset,
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

    /// Returns notes in their stored deterministic order.
    pub fn notes(&self) -> &[ReviewNote] {
        &self.notes
    }

    /// Returns note events in sequence order.
    pub fn events(&self) -> &[NoteEvent] {
        &self.events
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
        validate_notes(&self.changeset, &self.notes)?;
        validate_events(&self.notes, &self.events)
    }

    fn canonicalize(&mut self) {
        self.notes.sort_by(|left, right| left.id.cmp(&right.id));
        self.events.sort_by_key(NoteEvent::sequence);
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
    let expected_count = range.end.get() - range.start.get() + 1;
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
        note.anchor.validate(changeset)?;
        validate_text("author identifier", note.author.id(), 256)?;
        if let Some(name) = note.author.display_name() {
            validate_text("author display name", name, 256)?;
        }
        validate_note_body(note.body())?;
        validate_provenance(note.provenance())?;
    }
    Ok(())
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
    fn review_round_trip_preserves_unknown_fields() {
        let review = empty_review();
        let json = serde_json::to_string(&review).unwrap();
        let json = json.replacen("\"revision\":1", "\"future\":{\"enabled\":true},\"revision\":1", 1);
        let decoded: Review = serde_json::from_str(&json).unwrap();
        let encoded = serde_json::to_string(&decoded).unwrap();
        assert!(encoded.contains("\"future\":{\"enabled\":true}"));
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
