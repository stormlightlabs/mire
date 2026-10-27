//! Walkthrough batches, reports, and renderings for the `mire walkthrough` commands.

use std::fmt::Write as _;

use mire_core::{
    Anchor, AnchorSide, Author, BytePath, FileContent, FileDiff, FileStatus, Fingerprint, Provenance, ReanchorOutcome,
    Review, ReviewError, SchemaVersion, StepAnchor, StepId, StepInput, StepLocation, Walkthrough, WalkthroughCoverage,
    WalkthroughInput, WalkthroughStep,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{CURRENT_PROTOCOL_SCHEMA_VERSION, LocationFields, ProtocolError};

type Result<T> = std::result::Result<T, ProtocolError>;

/// The re-anchor state of one step anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorState {
    /// Written against the current capture and never re-anchored.
    Original,
    /// The anchor identity still exists after a refresh.
    Exact,
    /// One supported new location was found after a refresh.
    Moved,
    /// No supported location was found after a refresh.
    Stale,
    /// More than one equally supported location was found after a refresh.
    Ambiguous,
}

impl AnchorState {
    fn of(anchor: &StepAnchor) -> Self {
        match anchor.reanchor_outcome() {
            None => Self::Original,
            Some(ReanchorOutcome::Exact { .. }) => Self::Exact,
            Some(ReanchorOutcome::Moved { .. }) => Self::Moved,
            Some(ReanchorOutcome::Stale { .. }) => Self::Stale,
            Some(ReanchorOutcome::Ambiguous { .. }) => Self::Ambiguous,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Exact => "exact",
            Self::Moved => "moved",
            Self::Stale => "stale",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// Why a walkthrough batch could not become a core write.
#[derive(Debug)]
pub enum BatchError {
    /// The batch breaks the protocol, such as a malformed location.
    Protocol(ProtocolError),
    /// The batch breaks a review rule, such as the step identifier rules.
    Review(ReviewError),
}

impl From<ProtocolError> for BatchError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

/// A schema-versioned walkthrough write read by `walkthrough apply`.
#[derive(Debug, Deserialize)]
pub struct WalkthroughBatch {
    schema_version: SchemaVersion,
    review_revision: u64,
    #[serde(default)]
    overview: Option<String>,
    author: Author,
    provenance: Provenance,
    steps: Vec<StepRequest>,
}

/// A walkthrough batch converted into a core write.
pub struct WalkthroughWrite {
    /// The revision the caller observed before building the batch.
    pub revision: u64,
    /// The walkthrough to store.
    pub input: WalkthroughInput,
}

#[derive(Debug, Deserialize)]
struct StepRequest {
    #[serde(default)]
    id: Option<String>,
    title: String,
    narrative: String,
    locations: Vec<LocationFields>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    diagram: Option<String>,
}

#[derive(Serialize)]
struct WalkthroughDocument<'a> {
    schema_version: SchemaVersion,
    review_revision: u64,
    changeset_fingerprint: Fingerprint,
    walkthrough: Option<WalkthroughView<'a>>,
    coverage: Option<CoverageView<'a>>,
}

#[derive(Serialize)]
struct WalkthroughView<'a> {
    overview: Option<&'a str>,
    author: &'a Author,
    provenance: &'a Provenance,
    steps: Vec<StepView<'a>>,
}

#[derive(Serialize)]
struct StepView<'a> {
    id: &'a str,
    title: &'a str,
    narrative: &'a str,
    depends_on: Vec<&'a str>,
    diagram: Option<&'a str>,
    anchors: Vec<AnchorView<'a>>,
}

#[derive(Serialize)]
struct AnchorView<'a> {
    state: AnchorState,
    anchor: &'a Anchor,
    current_anchor: Option<&'a Anchor>,
    reanchor_outcome: Option<&'a ReanchorOutcome>,
}

#[derive(Serialize)]
struct CoverageView<'a> {
    uncovered_hunks: Vec<UncoveredHunk<'a>>,
    uncovered_files: Vec<UncoveredFile<'a>>,
}

#[derive(Serialize)]
struct UncoveredHunk<'a> {
    old_path: Option<&'a BytePath>,
    new_path: Option<&'a BytePath>,
    old_start: u64,
    old_line_count: u64,
    new_start: u64,
    new_line_count: u64,
    fingerprint: Fingerprint,
}

#[derive(Serialize)]
struct UncoveredFile<'a> {
    old_path: Option<&'a BytePath>,
    new_path: Option<&'a BytePath>,
    status: FileStatus,
    fingerprint: Fingerprint,
}

impl WalkthroughBatch {
    /// Validates the batch schema and converts it into a core walkthrough write.
    ///
    /// Malformed locations fail with [`BatchError::Protocol`]. Step
    /// identifiers that break the identifier rules fail with
    /// [`BatchError::Review`] holding [`ReviewError::InvalidStep`].
    pub fn into_write(self) -> std::result::Result<WalkthroughWrite, BatchError> {
        if self.schema_version.major != CURRENT_PROTOCOL_SCHEMA_VERSION.major {
            return Err(BatchError::Protocol(ProtocolError::UnsupportedSchemaMajor {
                found: self.schema_version.major,
                supported: CURRENT_PROTOCOL_SCHEMA_VERSION.major,
            }));
        }
        let steps = self
            .steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| step.into_input(index))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(WalkthroughWrite {
            revision: self.review_revision,
            input: WalkthroughInput::new(self.overview, steps, self.author, self.provenance),
        })
    }
}

impl StepRequest {
    fn into_input(self, index: usize) -> std::result::Result<StepInput, BatchError> {
        let step_error = |reason| BatchError::Review(ReviewError::InvalidStep { index, reason: Box::new(reason) });
        let id = self.id.map(StepId::new).transpose().map_err(step_error)?;
        let depends_on = self
            .depends_on
            .into_iter()
            .map(StepId::new)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(step_error)?;
        let locations = self
            .locations
            .into_iter()
            .map(|location| {
                let (path, side, range) = location.into_parts().map_err(|error| match error {
                    ProtocolError::InvalidInput(message) => {
                        ProtocolError::InvalidInput(format!("walkthrough step {index}: {message}"))
                    }
                    other => other,
                })?;
                Ok(StepLocation::new(path, side, range))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(StepInput::new(
            id,
            self.title,
            self.narrative,
            locations,
            depends_on,
            self.diagram,
        ))
    }
}

/// Serializes the result of a stored walkthrough write.
pub fn apply_result_json(review: &Review) -> Result<Vec<u8>> {
    let steps = review
        .walkthrough()
        .map(|walkthrough| {
            walkthrough
                .steps()
                .iter()
                .map(|step| step.id().as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    to_json_line(&json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": "applied",
        "review_revision": review.revision().get(),
        "steps": steps,
    }))
}

/// Serializes the result of clearing a walkthrough.
pub fn clear_result_json(review: &Review, cleared: bool) -> Result<Vec<u8>> {
    to_json_line(&json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": if cleared { "cleared" } else { "unchanged" },
        "review_revision": review.revision().get(),
    }))
}

/// Serializes a rejected walkthrough write with its stable failure code.
///
/// A step failure names the step by `step_index` and reports the code of the
/// step's own error, such as `location_not_found`.
pub fn rejection_json(error: &ReviewError) -> Result<Vec<u8>> {
    let failure = match error {
        ReviewError::InvalidStep { index, reason } => json!({
            "step_index": index,
            "code": reason.cause_code(),
            "error": reason.to_string(),
        }),
        ReviewError::RevisionConflict { expected, actual } => json!({
            "code": error.error_code(),
            "error": error.to_string(),
            "expected": expected,
            "actual": actual,
        }),
        other => json!({
            "code": other.cause_code(),
            "error": other.to_string(),
        }),
    };
    to_json_line(&json!({
        "schema_version": CURRENT_PROTOCOL_SCHEMA_VERSION,
        "status": "rejected",
        "failures": [failure],
    }))
}

/// Serializes the walkthrough, each anchor's state, and the coverage.
pub fn walkthrough_json(review: &Review) -> Result<Vec<u8>> {
    let coverage = review.walkthrough_coverage();
    let document = WalkthroughDocument {
        schema_version: CURRENT_PROTOCOL_SCHEMA_VERSION,
        review_revision: review.revision().get(),
        changeset_fingerprint: review.changeset().fingerprint(),
        walkthrough: review.walkthrough().map(walkthrough_view),
        coverage: coverage.as_ref().map(|coverage| coverage_view(review, coverage)),
    };
    to_json_line(&document)
}

/// Renders the walkthrough as plain text for a terminal.
pub fn walkthrough_text(review: &Review) -> Vec<u8> {
    let mut output = format!("review revision: {}\n", review.revision().get());
    let Some(walkthrough) = review.walkthrough() else {
        output.push_str("walkthrough: none\n");
        return output.into_bytes();
    };
    let author = walkthrough.author();
    let _ = writeln!(
        output,
        "walkthrough: {} {} by {}, {}",
        walkthrough.steps().len(),
        if walkthrough.steps().len() == 1 { "step" } else { "steps" },
        author.display_name().unwrap_or(author.id()),
        walkthrough.provenance()
    );
    if let Some(overview) = walkthrough.overview() {
        output.push_str("\noverview:\n");
        push_indented(&mut output, overview, "  ");
    }
    for (index, step) in walkthrough.steps().iter().enumerate() {
        push_step(&mut output, index, step);
    }
    if let Some(coverage) = review.walkthrough_coverage() {
        push_coverage(&mut output, review, &coverage);
    }
    output.into_bytes()
}

fn walkthrough_view(walkthrough: &Walkthrough) -> WalkthroughView<'_> {
    WalkthroughView {
        overview: walkthrough.overview(),
        author: walkthrough.author(),
        provenance: walkthrough.provenance(),
        steps: walkthrough
            .steps()
            .iter()
            .map(|step| StepView {
                id: step.id().as_str(),
                title: step.title(),
                narrative: step.narrative(),
                depends_on: step.depends_on().iter().map(StepId::as_str).collect(),
                diagram: step.diagram(),
                anchors: step
                    .anchors()
                    .iter()
                    .map(|anchor| AnchorView {
                        state: AnchorState::of(anchor),
                        anchor: anchor.anchor(),
                        current_anchor: anchor.current_anchor(),
                        reanchor_outcome: anchor.reanchor_outcome(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn coverage_view<'a>(review: &'a Review, coverage: &WalkthroughCoverage) -> CoverageView<'a> {
    let files = review.changeset().files();
    CoverageView {
        uncovered_hunks: coverage
            .uncovered_hunks
            .iter()
            .filter_map(|location| {
                let file = files.get(location.file_index)?;
                let FileContent::Text { hunks } = file.content() else {
                    return None;
                };
                let hunk = hunks.get(location.hunk_index)?;
                Some(UncoveredHunk {
                    old_path: file.old_side().map(|side| &side.path),
                    new_path: file.new_side().map(|side| &side.path),
                    old_start: hunk.old_start(),
                    old_line_count: hunk.old_line_count(),
                    new_start: hunk.new_start(),
                    new_line_count: hunk.new_line_count(),
                    fingerprint: hunk.fingerprint(),
                })
            })
            .collect(),
        uncovered_files: coverage
            .uncovered_files
            .iter()
            .filter_map(|index| files.get(*index))
            .map(|file| UncoveredFile {
                old_path: file.old_side().map(|side| &side.path),
                new_path: file.new_side().map(|side| &side.path),
                status: file.status(),
                fingerprint: file.fingerprint(),
            })
            .collect(),
    }
}

fn push_step(output: &mut String, index: usize, step: &WalkthroughStep) {
    let _ = writeln!(output, "\n{}. {} [{}]", index + 1, step.title(), step.id().as_str());
    if !step.depends_on().is_empty() {
        let depends_on = step.depends_on().iter().map(StepId::as_str).collect::<Vec<_>>();
        let _ = writeln!(output, "   depends on: {}", depends_on.join(", "));
    }
    for anchor in step.anchors() {
        let shown = anchor.current_anchor().unwrap_or_else(|| anchor.anchor());
        let _ = writeln!(
            output,
            "   {} ({})",
            location_label(shown),
            AnchorState::of(anchor).label()
        );
    }
    output.push('\n');
    push_indented(output, step.narrative(), "   ");
    if let Some(diagram) = step.diagram() {
        output.push_str("\n   diagram:\n");
        push_indented(output, diagram, "     ");
    }
}

fn push_coverage(output: &mut String, review: &Review, coverage: &WalkthroughCoverage) {
    let files = review.changeset().files();
    if coverage.uncovered_hunks.is_empty() && coverage.uncovered_files.is_empty() {
        output.push_str("\nevery hunk is covered\n");
        return;
    }
    if !coverage.uncovered_hunks.is_empty() {
        output.push_str("\nuncovered hunks:\n");
        for location in &coverage.uncovered_hunks {
            let Some(file) = files.get(location.file_index) else {
                continue;
            };
            let FileContent::Text { hunks } = file.content() else {
                continue;
            };
            let Some(hunk) = hunks.get(location.hunk_index) else {
                continue;
            };
            let _ = writeln!(
                output,
                "  {} @@ -{},{} +{},{} @@",
                file_label(file),
                hunk.old_start(),
                hunk.old_line_count(),
                hunk.new_start(),
                hunk.new_line_count()
            );
        }
    }
    if !coverage.uncovered_files.is_empty() {
        output.push_str("\nfiles with no hunks:\n");
        for file in coverage.uncovered_files.iter().filter_map(|index| files.get(*index)) {
            let _ = writeln!(output, "  {} ({})", file_label(file), status_label(file.status()));
        }
    }
}

fn push_indented(output: &mut String, text: &str, indent: &str) {
    for line in text.lines() {
        if line.is_empty() {
            output.push('\n');
        } else {
            let _ = writeln!(output, "{indent}{line}");
        }
    }
}

fn location_label(anchor: &Anchor) -> String {
    let side = match anchor.side() {
        AnchorSide::Old => "old",
        AnchorSide::New => "new",
    };
    let start = anchor.range().start().get();
    let end = anchor.range().end().get();
    let lines = if start == end { format!("line {start}") } else { format!("lines {start}-{end}") };
    format!("{}, {side} {lines}", String::from_utf8_lossy(anchor.path().as_bytes()))
}

fn file_label(file: &FileDiff) -> String {
    let old = file
        .old_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    let new = file
        .new_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    match (old, new) {
        (Some(old), Some(new)) if old != new => format!("{old} -> {new}"),
        (_, Some(path)) | (Some(path), None) => path.into_owned(),
        (None, None) => String::from("(unnamed)"),
    }
}

fn status_label(status: FileStatus) -> String {
    match serde_json::to_value(status) {
        Ok(Value::String(label)) => label,
        _ => String::from("changed"),
    }
}

fn to_json_line(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).map_err(ProtocolError::Serialize)?;
    bytes.push(b'\n');
    Ok(bytes)
}
