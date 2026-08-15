use std::ffi::OsStr;
use std::io::{self, Write};

use mire_core::{
    AnchorSide, AnnotationKind, Author, BytePath, Changeset, ChangesetSource, FileContent, FileDiff, Fingerprint, Hunk,
    LineKind, LineNumber, LineRange, NoteApplyError, NoteEvent, NoteEventKind, NoteImportError, NoteInput,
    NoteSeverity, NoteStatus, Provenance, ReanchorEvidence, ReanchorOutcome, Review, ReviewNote, SchemaVersion,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

/// Protocol schema emitted by non-interactive review commands.
pub const CURRENT_PROTOCOL_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 2 };

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
    /// One normalized hunk selected by its manifest fingerprint.
    Hunk(Fingerprint),
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
    /// A requested hunk fingerprint is absent from the captured changeset.
    #[error("captured changeset has no hunk matching {0}")]
    HunkNotFound(Fingerprint),
    /// A requested hunk fingerprint occurs more than once.
    #[error("captured changeset has duplicate hunks matching {0}")]
    DuplicateHunkFingerprint(Fingerprint),
    /// An explicitly requested context payload exceeds its caller-supplied bound.
    #[error("context output is {actual} bytes; requested limit is {limit} bytes")]
    ContextTooLarge { actual: usize, limit: usize },
    /// A location request contains an invalid high-level field.
    #[error("invalid location-based note input: {0}")]
    InvalidInput(String),
    /// A protocol response could not be encoded.
    #[error("cannot serialize protocol output: {0}")]
    Serialize(serde_json::Error),
}

impl ProtocolError {
    /// Returns the stable machine-readable failure code.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::UnsupportedSchemaMajor { .. } => "unsupported_schema_major",
            Self::FileNotFound(_) => "file_not_found",
            Self::HunkNotFound(_) => "hunk_not_found",
            Self::DuplicateHunkFingerprint(_) => "duplicate_hunk_fingerprint",
            Self::ContextTooLarge { .. } => "context_too_large",
            Self::InvalidInput(_) => "invalid_input",
            Self::Serialize(_) => "serialize_error",
        }
    }
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
    Hunk {
        hunk: &'a Hunk,
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

/// A schema-versioned atomic batch of location-based findings.
#[derive(Debug, Deserialize)]
pub struct LocationBatch {
    schema_version: SchemaVersion,
    review_revision: u64,
    notes: Vec<LocationRequest>,
}

impl LocationBatch {
    /// Validates and converts the wire request into core note inputs.
    pub fn into_inputs(self) -> Result<(u64, Vec<NoteInput>)> {
        if self.schema_version.major != CURRENT_PROTOCOL_SCHEMA_VERSION.major {
            return Err(ProtocolError::UnsupportedSchemaMajor {
                found: self.schema_version.major,
                supported: CURRENT_PROTOCOL_SCHEMA_VERSION.major,
            });
        }
        let inputs = self
            .notes
            .into_iter()
            .map(LocationRequest::into_input)
            .collect::<Result<_>>()?;
        Ok((self.review_revision, inputs))
    }
}

#[derive(Debug, Deserialize)]
struct LocationRequest {
    #[serde(alias = "path")]
    file: PathInput,
    #[serde(default)]
    side: Option<AnchorSide>,
    #[serde(default)]
    start: Option<u64>,
    #[serde(default)]
    old_line: Option<u64>,
    #[serde(default)]
    new_line: Option<u64>,
    #[serde(default, alias = "end_line")]
    end: Option<u64>,
    author: Author,
    provenance: Provenance,
    severity: NoteSeverity,
    #[serde(alias = "kind")]
    annotation_kind: AnnotationKind,
    body: String,
}

impl LocationRequest {
    fn into_input(self) -> Result<NoteInput> {
        let path = match self.file {
            PathInput::Text(path) => BytePath::new(path.into_bytes()),
            PathInput::Bytes(path) => BytePath::new(path),
        }
        .map_err(|error| ProtocolError::InvalidInput(error.to_string()))?;
        let (side, start_value) = match (self.side, self.start, self.old_line, self.new_line) {
            (Some(side), Some(start), None, None) => (side, start),
            (None, None, Some(start), None) => (AnchorSide::Old, start),
            (None, None, None, Some(start)) => (AnchorSide::New, start),
            _ => {
                return Err(ProtocolError::InvalidInput(
                    "supply either side and start, old_line, or new_line".to_owned(),
                ));
            }
        };
        let start = LineNumber::new(start_value)
            .ok_or_else(|| ProtocolError::InvalidInput("line number must be greater than zero".to_owned()))?;
        let end_value = self.end.unwrap_or(start_value);
        let end = LineNumber::new(end_value)
            .ok_or_else(|| ProtocolError::InvalidInput("line number must be greater than zero".to_owned()))?;
        let range = LineRange::new(start, end).map_err(|error| ProtocolError::InvalidInput(error.to_string()))?;
        NoteInput::new(
            path,
            side,
            range,
            self.author,
            self.provenance,
            self.severity,
            self.annotation_kind,
            self.body,
        )
        .map_err(|error| ProtocolError::InvalidInput(error.to_string()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PathInput {
    Text(String),
    Bytes(Vec<u8>),
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
    reanchor_outcome: Option<&'a mire_core::ReanchorOutcome>,
}

impl<'a> From<&'a ReviewNote> for NoteSummary<'a> {
    fn from(note: &'a ReviewNote) -> Self {
        let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
        Self {
            id: note.id().as_str(),
            path: anchor.path(),
            side: anchor.side(),
            start: anchor.range().start().get(),
            end: anchor.range().end().get(),
            severity: note.severity(),
            annotation_kind: note.annotation_kind(),
            status: note.status(),
            provenance: note.provenance(),
            reanchor_outcome: note.reanchor_outcome(),
        }
    }
}

#[derive(Serialize)]
struct NoteDocument<'a> {
    schema_version: SchemaVersion,
    review_revision: u64,
    changeset_fingerprint: mire_core::Fingerprint,
    notes: &'a [ReviewNote],
    events: &'a [NoteEvent],
}

#[derive(Default, Serialize)]
struct FindingTotals {
    total: usize,
    open: usize,
    resolved: usize,
    dismissed: usize,
    accepted_risk: usize,
}

#[derive(Default, Serialize)]
struct ReanchorTotals {
    original: usize,
    exact: usize,
    moved: usize,
    stale: usize,
    ambiguous: usize,
}

#[derive(Serialize)]
struct ReviewStatusDocument<'a> {
    schema_version: SchemaVersion,
    review_revision: u64,
    source: &'a ChangesetSource,
    changeset_fingerprint: Fingerprint,
    files: usize,
    additions: usize,
    deletions: usize,
    findings: FindingTotals,
    reanchor: ReanchorTotals,
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
        ContextSelection::Hunk(fingerprint) => {
            let matching = review
                .changeset()
                .files()
                .iter()
                .filter_map(|file| match file.content() {
                    FileContent::Text { hunks } => Some(hunks),
                    FileContent::Binary => None,
                })
                .flatten()
                .filter(|hunk| hunk.fingerprint() == fingerprint)
                .collect::<Vec<_>>();
            let hunk = match matching.as_slice() {
                [] => return Err(ProtocolError::HunkNotFound(fingerprint)),
                [hunk] => *hunk,
                _ => return Err(ProtocolError::DuplicateHunkFingerprint(fingerprint)),
            };
            ContextPayload::Hunk { hunk }
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
        events: review.events(),
    };
    let mut bytes = serde_json::to_vec(&document).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Serializes deterministic JSON review status for scripts and agents.
pub fn review_status_json(review: &Review) -> Result<Vec<u8>> {
    let status = review_status(review);
    let document = ReviewStatusDocument {
        schema_version: CURRENT_PROTOCOL_SCHEMA_VERSION,
        review_revision: review.revision().get(),
        source: review.changeset().source(),
        changeset_fingerprint: review.changeset().fingerprint(),
        files: review.changeset().files().len(),
        additions: status.additions,
        deletions: status.deletions,
        findings: status.findings,
        reanchor: status.reanchor,
    };
    let mut bytes = serde_json::to_vec(&document).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Renders concise, deterministic review status for terminal use.
pub fn review_status_text(review: &Review) -> Vec<u8> {
    let status = review_status(review);
    format!(
        "source: {}\nreview revision: {}\nchangeset: {}\nfiles: {}\nchanges: +{} -{}\nfindings: {} (open: {}, resolved: {}, dismissed: {}, accepted-risk: {})\nre-anchor: original: {}, exact: {}, moved: {}, stale: {}, ambiguous: {}\n",
        source_label(review.changeset().source()),
        review.revision().get(),
        review.changeset().fingerprint(),
        review.changeset().files().len(),
        status.additions,
        status.deletions,
        status.findings.total,
        status.findings.open,
        status.findings.resolved,
        status.findings.dismissed,
        status.findings.accepted_risk,
        status.reanchor.original,
        status.reanchor.exact,
        status.reanchor.moved,
        status.reanchor.stale,
        status.reanchor.ambiguous,
    )
    .into_bytes()
}

struct ReviewStatus {
    additions: usize,
    deletions: usize,
    findings: FindingTotals,
    reanchor: ReanchorTotals,
}

fn review_status(review: &Review) -> ReviewStatus {
    let mut status = ReviewStatus {
        additions: 0,
        deletions: 0,
        findings: FindingTotals::default(),
        reanchor: ReanchorTotals::default(),
    };
    for file in review.changeset().files() {
        if let FileContent::Text { hunks } = file.content() {
            for hunk in hunks {
                for line in hunk.lines() {
                    match line.kind() {
                        LineKind::Addition => status.additions += 1,
                        LineKind::Deletion => status.deletions += 1,
                        LineKind::Context => {}
                    }
                }
            }
        }
    }
    for note in review.notes() {
        status.findings.total += 1;
        match note.status() {
            NoteStatus::Open => status.findings.open += 1,
            NoteStatus::Resolved => status.findings.resolved += 1,
            NoteStatus::Dismissed => status.findings.dismissed += 1,
            NoteStatus::AcceptedRisk => status.findings.accepted_risk += 1,
        }
        match note.reanchor_outcome() {
            None => status.reanchor.original += 1,
            Some(ReanchorOutcome::Exact { .. }) => status.reanchor.exact += 1,
            Some(ReanchorOutcome::Moved { .. }) => status.reanchor.moved += 1,
            Some(ReanchorOutcome::Stale { .. }) => status.reanchor.stale += 1,
            Some(ReanchorOutcome::Ambiguous { .. }) => status.reanchor.ambiguous += 1,
        }
    }
    status
}

fn source_label(source: &ChangesetSource) -> &'static str {
    match source {
        ChangesetSource::Patch { .. } => "patch",
        ChangesetSource::DirectFiles => "direct files",
        ChangesetSource::Git { operation } => match operation {
            mire_core::GitOperation::Worktree { .. } => "git worktree",
            mire_core::GitOperation::Diff { .. } => "git diff",
            mire_core::GitOperation::Show { .. } => "git show",
        },
    }
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
pub fn protocol_error_json(error: &ProtocolError) -> Result<Vec<u8>> {
    let value = json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": "rejected",
        "failures": [{
            "code": error.error_code(),
            "error": error.to_string(),
        }],
    });
    let mut bytes = serde_json::to_vec(&value).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Serializes every rejected location-based input.
pub fn apply_error_json(error: &NoteApplyError) -> Result<Vec<u8>> {
    let failures = error
        .failures()
        .iter()
        .map(|failure| {
            json!({
                "input_index": failure.input_index(),
                "code": failure.error().error_code(),
                "error": failure.error().to_string(),
            })
        })
        .collect::<Vec<_>>();
    rejection_json(failures)
}

/// Serializes every rejected full-note import.
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
    rejection_json(failures)
}

fn rejection_json(failures: Vec<serde_json::Value>) -> Result<Vec<u8>> {
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
        let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
        output.push_str(&format!(
            "\n## {}: {}\n\n- Kind: `{}`\n- Status: `{}`\n- Author: {}\n- Provenance: {}\n- Re-anchor: `{}`\n- Original location: {}\n- Current location: {}\n{}\n",
            note.severity(),
            note.id().as_str(),
            note.annotation_kind(),
            note.status(),
            note.author().display_name().unwrap_or(note.author().id()),
            note.provenance(),
            reanchor_label(note),
            anchor_location(note.anchor()),
            anchor_location(anchor),
            reanchor_details(note)
        ));
        markdown_events(&mut output, review.events(), note);
    }
    output.into_bytes()
}

fn reanchor_label(note: &ReviewNote) -> &'static str {
    match note.reanchor_outcome() {
        None => "original",
        Some(mire_core::ReanchorOutcome::Exact { .. }) => "exact",
        Some(mire_core::ReanchorOutcome::Moved { .. }) => "moved",
        Some(mire_core::ReanchorOutcome::Stale { .. }) => "stale",
        Some(mire_core::ReanchorOutcome::Ambiguous { .. }) => "ambiguous",
    }
}

fn reanchor_details(note: &ReviewNote) -> String {
    match note.reanchor_outcome() {
        None => String::new(),
        Some(ReanchorOutcome::Exact { candidate, .. } | ReanchorOutcome::Moved { candidate, .. }) => {
            format!("- Match evidence: {}\n", evidence_markdown(candidate.evidence()))
        }
        Some(ReanchorOutcome::Stale { evidence, .. }) => {
            format!("- Match evidence: {}\n", evidence_markdown(*evidence))
        }
        Some(ReanchorOutcome::Ambiguous { candidates, .. }) => {
            let candidates = candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{} ({})",
                        anchor_location(candidate.anchor()),
                        evidence_markdown(candidate.evidence())
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!("- Candidates: {candidates}\n")
        }
    }
}

fn markdown_events(output: &mut String, events: &[NoteEvent], note: &ReviewNote) {
    let events = events
        .iter()
        .filter(|event| event.note_id() == note.id())
        .collect::<Vec<_>>();
    if events.is_empty() {
        return;
    }
    output.push_str("- Events:\n");
    for event in events {
        let author = event.author().display_name().unwrap_or(event.author().id());
        output.push_str(&format!(
            "  - {}. {} {}\n",
            event.sequence(),
            author,
            event_description(event.event())
        ));
    }
}

fn event_description(event: &NoteEventKind) -> String {
    match event {
        NoteEventKind::Created { status } => format!("created the note as `{status}`"),
        NoteEventKind::StatusChanged { from, to } => format!("changed the status from `{from}` to `{to}`"),
    }
}

fn evidence_markdown(evidence: ReanchorEvidence) -> String {
    format!(
        "path match: {}; content match: {}; nearby context: {} before, {} after",
        evidence.path_match(),
        evidence.content_match(),
        evidence.context_before(),
        evidence.context_after()
    )
}

fn anchor_location(anchor: &mire_core::Anchor) -> String {
    format!(
        "`{}` ({}, lines {}–{})",
        String::from_utf8_lossy(anchor.path().as_bytes()),
        anchor.side(),
        anchor.range().start().get(),
        anchor.range().end().get()
    )
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
