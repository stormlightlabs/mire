//! Walkthroughs: an agent's ordered explanation of a change, anchored to hunks.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BytePath, Changeset, FileContent, FileDiff};

use super::{
    Anchor, AnchorSide, Author, LineRange, Provenance, ReanchorOutcome, Result, ReviewError, anchor_is_unmatched,
    reanchor, resolve_range, validate_anchor_state, validate_author, validate_provenance, validate_text,
};

/// Maximum number of steps in one walkthrough.
pub const MAX_WALKTHROUGH_STEPS: usize = 10_000;
/// Maximum number of anchors across all steps of one walkthrough.
pub const MAX_WALKTHROUGH_ANCHORS: usize = 10_000;
/// Maximum UTF-8 byte length of a walkthrough overview, and of each step's
/// narrative and diagram.
pub const MAX_WALKTHROUGH_TEXT_BYTES: usize = 1024 * 1024;
/// Maximum UTF-8 byte length of a step title.
pub const MAX_STEP_TITLE_BYTES: usize = 1024;

const MAX_STEP_ID_BYTES: usize = 128;

/// A step identifier, unique within one walkthrough.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct StepId(String);

impl StepId {
    /// Creates a step identifier: one line of at most 128 bytes, not blank,
    /// with no control characters.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_line("step identifier", &value, MAX_STEP_ID_BYTES)?;
        Ok(Self(value))
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for StepId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// A path, diff side, and line range that a step points at, before Mire
/// resolves it to an anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepLocation {
    path: BytePath,
    side: AnchorSide,
    range: LineRange,
}

impl StepLocation {
    /// Creates a location request.
    pub const fn new(path: BytePath, side: AnchorSide, range: LineRange) -> Self {
        Self { path, side, range }
    }
}

/// One step of a walkthrough write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepInput {
    id: Option<StepId>,
    title: String,
    narrative: String,
    locations: Vec<StepLocation>,
    depends_on: Vec<StepId>,
    diagram: Option<String>,
}

impl StepInput {
    /// Creates a step request. Mire generates an identifier when `id` is `None`.
    pub const fn new(
        id: Option<StepId>, title: String, narrative: String, locations: Vec<StepLocation>, depends_on: Vec<StepId>,
        diagram: Option<String>,
    ) -> Self {
        Self { id, title, narrative, locations, depends_on, diagram }
    }
}

/// A whole walkthrough to store, replacing any walkthrough the review holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WalkthroughInput {
    overview: Option<String>,
    steps: Vec<StepInput>,
    author: Author,
    provenance: Provenance,
}

impl WalkthroughInput {
    /// Creates a walkthrough request. Steps keep the order given here.
    pub const fn new(overview: Option<String>, steps: Vec<StepInput>, author: Author, provenance: Provenance) -> Self {
        Self { overview, steps, author, provenance }
    }
}

/// One step anchor and its latest re-anchor result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StepAnchor {
    anchor: Anchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reanchor_outcome: Option<ReanchorOutcome>,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl StepAnchor {
    /// Returns the anchor recorded when the walkthrough was written.
    pub const fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    /// Returns the latest match result, if this anchor has crossed a capture.
    pub const fn reanchor_outcome(&self) -> Option<&ReanchorOutcome> {
        self.reanchor_outcome.as_ref()
    }

    /// Returns the accepted location in the current capture. Stale and
    /// ambiguous anchors have none.
    pub fn current_anchor(&self) -> Option<&Anchor> {
        self.reanchor_outcome
            .as_ref()
            .map_or(Some(&self.anchor), ReanchorOutcome::current_anchor)
    }
}

/// One stored walkthrough step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalkthroughStep {
    id: StepId,
    title: String,
    narrative: String,
    anchors: Vec<StepAnchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<StepId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diagram: Option<String>,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl WalkthroughStep {
    /// Returns the step identifier.
    pub const fn id(&self) -> &StepId {
        &self.id
    }

    /// Returns the one-line title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the Markdown narrative.
    pub fn narrative(&self) -> &str {
        &self.narrative
    }

    /// Returns the step's anchors in the order they were written.
    pub fn anchors(&self) -> &[StepAnchor] {
        &self.anchors
    }

    /// Returns the earlier steps this step depends on.
    pub fn depends_on(&self) -> &[StepId] {
        &self.depends_on
    }

    /// Returns the Mermaid diagram source, when the step has one.
    pub fn diagram(&self) -> Option<&str> {
        self.diagram.as_deref()
    }

    /// Reports whether any anchor is stale or ambiguous after the latest refresh.
    pub fn has_unsettled_anchor(&self) -> bool {
        self.anchors.iter().any(|anchor| anchor.current_anchor().is_none())
    }
}

/// The ordered steps that explain a change, stored at most once per review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Walkthrough {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overview: Option<String>,
    steps: Vec<WalkthroughStep>,
    author: Author,
    provenance: Provenance,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl Walkthrough {
    /// Returns the Markdown overview read before the first step.
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }

    /// Returns the steps in reading order.
    pub fn steps(&self) -> &[WalkthroughStep] {
        &self.steps
    }

    /// Returns who wrote the walkthrough.
    pub const fn author(&self) -> &Author {
        &self.author
    }

    /// Returns how the walkthrough entered the review.
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A hunk named by its position in the captured changeset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HunkLocation {
    /// Index into [`Changeset::files`].
    pub file_index: usize,
    /// Index into that file's hunks.
    pub hunk_index: usize,
}

/// What a walkthrough leaves out of the captured change.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WalkthroughCoverage {
    /// Hunks no exact or moved step anchor overlaps, in changeset order.
    pub uncovered_hunks: Vec<HunkLocation>,
    /// Indexes of files with no hunks, such as binary, rename-only, and
    /// mode-only files, which no anchor can point at.
    pub uncovered_files: Vec<usize>,
}

/// Resolves and validates a walkthrough write against the current capture.
pub fn build_walkthrough(changeset: &Changeset, input: WalkthroughInput) -> Result<Walkthrough> {
    if matches!(input.provenance, Provenance::Human) {
        return Err(ReviewError::HumanProvenanceForbidden);
    }
    check_counts(
        input.steps.len(),
        input.steps.iter().map(|step| step.locations.len()).sum(),
    )?;

    let mut supplied = BTreeSet::new();
    for (index, step) in input.steps.iter().enumerate() {
        if let Some(dependency) = step.depends_on.iter().find(|id| !supplied.contains(*id)) {
            return Err(step_error(
                index,
                ReviewError::ForwardStepDependency(dependency.0.clone()),
            ));
        }
        check_unique_dependencies(&step.depends_on).map_err(|error| step_error(index, error))?;
        if let Some(id) = &step.id
            && !supplied.insert(id.clone())
        {
            return Err(step_error(index, ReviewError::DuplicateStepId(id.0.clone())));
        }
    }

    let mut used = supplied;
    let mut steps = Vec::with_capacity(input.steps.len());
    for (index, step) in input.steps.into_iter().enumerate() {
        let id = match step.id {
            Some(id) => id,
            None => generate_id(index, &mut used),
        };
        let anchors = step
            .locations
            .iter()
            .map(|location| {
                resolve_range(changeset, &location.path, location.side, location.range).map(|anchor| StepAnchor {
                    anchor,
                    reanchor_outcome: None,
                    extensions: BTreeMap::new(),
                })
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|error| step_error(index, error))?;
        steps.push(WalkthroughStep {
            id,
            title: step.title,
            narrative: step.narrative,
            anchors,
            depends_on: step.depends_on,
            diagram: step.diagram,
            extensions: BTreeMap::new(),
        });
    }
    let walkthrough = Walkthrough {
        overview: input.overview,
        steps,
        author: input.author,
        provenance: input.provenance,
        extensions: BTreeMap::new(),
    };
    validate_walkthrough(changeset, &walkthrough)?;
    Ok(walkthrough)
}

/// Checks bounds, identifiers, dependencies, text, and anchors of a stored walkthrough.
pub fn validate_walkthrough(changeset: &Changeset, walkthrough: &Walkthrough) -> Result<()> {
    check_counts(
        walkthrough.steps.len(),
        walkthrough.steps.iter().map(|step| step.anchors.len()).sum(),
    )?;
    if let Some(overview) = &walkthrough.overview {
        validate_markdown("walkthrough overview", overview)?;
    }
    validate_author(&walkthrough.author)?;
    validate_provenance(&walkthrough.provenance)?;

    let mut earlier = BTreeSet::new();
    for (index, step) in walkthrough.steps.iter().enumerate() {
        validate_step(changeset, step, &earlier).map_err(|error| step_error(index, error))?;
        earlier.insert(&step.id);
    }
    Ok(())
}

/// Re-anchors every step anchor against a later capture with the note rules.
pub fn reanchor_walkthrough(walkthrough: &Walkthrough, previous: &Changeset, current: &Changeset) -> Walkthrough {
    let mut refreshed = walkthrough.clone();
    for anchor in refreshed.steps.iter_mut().flat_map(|step| step.anchors.iter_mut()) {
        anchor.reanchor_outcome = Some(reanchor::classify_anchor(
            &anchor.anchor,
            anchor.reanchor_outcome.as_ref(),
            previous,
            current,
        ));
    }
    refreshed
}

/// Settles every step anchor whose location is gone from `changeset`.
///
/// A capture can change under a walkthrough without a re-anchor, as when a
/// Mire that predates walkthroughs refreshes the review and keeps the
/// walkthrough as an unknown field. An anchor, or a candidate, that no longer
/// resolves cannot be matched again because the capture it was recorded
/// against is gone. Such an anchor becomes stale, and an ambiguous anchor
/// keeps the candidates that still resolve while at least two remain.
///
/// Every other inconsistency is left in place for [`validate_walkthrough`] to
/// reject: an outcome whose original anchor differs from the anchor, an
/// ambiguous outcome with fewer than two candidates, and an anchor that
/// resolves to content with a different fingerprint. A walkthrough whose
/// anchors all resolve is not changed.
pub fn settle_unmatched_anchors(changeset: &Changeset, walkthrough: &mut Walkthrough) {
    for anchor in walkthrough.steps.iter_mut().flat_map(|step| step.anchors.iter_mut()) {
        if let Some(outcome) = settled_outcome(changeset, anchor) {
            anchor.reanchor_outcome = Some(outcome);
        }
    }
}

/// Lists the hunks no exact or moved step anchor overlaps, and the files that have no hunks.
pub fn coverage(changeset: &Changeset, walkthrough: &Walkthrough) -> WalkthroughCoverage {
    let anchors = walkthrough
        .steps
        .iter()
        .flat_map(|step| step.anchors.iter())
        .filter_map(StepAnchor::current_anchor)
        .collect::<Vec<_>>();
    let mut coverage = WalkthroughCoverage::default();
    for (file_index, file) in changeset.files().iter().enumerate() {
        let hunks = match file.content() {
            FileContent::Text { hunks } if !hunks.is_empty() => hunks,
            _ => {
                coverage.uncovered_files.push(file_index);
                continue;
            }
        };
        for (hunk_index, hunk) in hunks.iter().enumerate() {
            let covered = anchors.iter().any(|anchor| {
                anchor.hunk_fingerprint() == hunk.fingerprint() && side_path(file, anchor.side()) == Some(anchor.path())
            });
            if !covered {
                coverage.uncovered_hunks.push(HunkLocation { file_index, hunk_index });
            }
        }
    }
    coverage
}

/// Returns the outcome an unmatched step anchor settles to, or `None` when
/// the anchor still resolves or is inconsistent in a way validation rejects.
fn settled_outcome(changeset: &Changeset, step_anchor: &StepAnchor) -> Option<ReanchorOutcome> {
    let anchor = &step_anchor.anchor;
    let outcome = step_anchor.reanchor_outcome.as_ref();
    let unmatched = |candidate: &Anchor| anchor_is_unmatched(changeset, candidate);
    match outcome {
        Some(outcome) if outcome.original_anchor() != anchor => None,
        None => unmatched(anchor)
            .ok()?
            .then(|| reanchor::stale_outcome(anchor, None, changeset)),
        Some(ReanchorOutcome::Exact { candidate, .. } | ReanchorOutcome::Moved { candidate, .. }) => {
            unmatched(candidate.anchor())
                .ok()?
                .then(|| reanchor::stale_outcome(anchor, outcome, changeset))
        }
        Some(ReanchorOutcome::Stale { .. }) => None,
        Some(ReanchorOutcome::Ambiguous { candidates, .. }) => {
            if candidates.len() < 2 {
                return None;
            }
            let flags = candidates
                .iter()
                .map(|candidate| unmatched(candidate.anchor()))
                .collect::<Result<Vec<_>>>()
                .ok()?;
            if !flags.contains(&true) {
                return None;
            }
            let kept = candidates
                .iter()
                .zip(flags)
                .filter(|(_, unmatched)| !unmatched)
                .map(|(candidate, _)| candidate.clone())
                .collect::<Vec<_>>();
            Some(if kept.len() >= 2 {
                ReanchorOutcome::Ambiguous { original_anchor: anchor.clone(), candidates: kept }
            } else {
                reanchor::stale_outcome(anchor, outcome, changeset)
            })
        }
    }
}

fn validate_step(changeset: &Changeset, step: &WalkthroughStep, earlier: &BTreeSet<&StepId>) -> Result<()> {
    if earlier.contains(&step.id) {
        return Err(ReviewError::DuplicateStepId(step.id.0.clone()));
    }
    validate_line("step title", &step.title, MAX_STEP_TITLE_BYTES)?;
    validate_markdown("step narrative", &step.narrative)?;
    if let Some(diagram) = &step.diagram {
        validate_markdown("step diagram", diagram)?;
    }
    if let Some(dependency) = step.depends_on.iter().find(|id| !earlier.contains(id)) {
        return Err(ReviewError::ForwardStepDependency(dependency.0.clone()));
    }
    check_unique_dependencies(&step.depends_on)?;
    if step.anchors.is_empty() {
        return Err(ReviewError::StepWithoutAnchors);
    }
    for anchor in &step.anchors {
        validate_anchor_state(changeset, &anchor.anchor, anchor.reanchor_outcome.as_ref())?;
    }
    Ok(())
}

/// Checks a one-line field: bounded, not blank, and free of control characters.
fn validate_line(field: &'static str, text: &str, max: usize) -> Result<()> {
    validate_text(field, text, max)?;
    if text.contains(['\n', '\r']) {
        return Err(ReviewError::MultilineField { field });
    }
    if text.chars().any(char::is_control) {
        return Err(ReviewError::ControlCharacter { field });
    }
    if text.trim().is_empty() {
        return Err(ReviewError::BlankField { field });
    }
    Ok(())
}

fn validate_markdown(field: &'static str, text: &str) -> Result<()> {
    if text.len() > MAX_WALKTHROUGH_TEXT_BYTES {
        return Err(ReviewError::TextTooLarge { field, actual: text.len(), limit: MAX_WALKTHROUGH_TEXT_BYTES });
    }
    if text.trim().is_empty() {
        return Err(ReviewError::BlankField { field });
    }
    Ok(())
}

/// Rejects a dependency list that names one step more than once. With the
/// earlier-steps rule, this bounds the list by the number of steps.
fn check_unique_dependencies(depends_on: &[StepId]) -> Result<()> {
    let mut seen = BTreeSet::new();
    match depends_on.iter().find(|id| !seen.insert(*id)) {
        Some(id) => Err(ReviewError::DuplicateStepDependency(id.0.clone())),
        None => Ok(()),
    }
}

fn check_counts(steps: usize, anchors: usize) -> Result<()> {
    if steps == 0 {
        return Err(ReviewError::EmptyWalkthrough);
    }
    if steps > MAX_WALKTHROUGH_STEPS {
        return Err(ReviewError::TooManySteps { actual: steps, limit: MAX_WALKTHROUGH_STEPS });
    }
    if anchors > MAX_WALKTHROUGH_ANCHORS {
        return Err(ReviewError::TooManyStepAnchors { actual: anchors, limit: MAX_WALKTHROUGH_ANCHORS });
    }
    Ok(())
}

/// Picks `step-<n>` for the step at `index`, counting up past taken identifiers.
fn generate_id(index: usize, used: &mut BTreeSet<StepId>) -> StepId {
    let mut number = index + 1;
    loop {
        let candidate = StepId(format!("step-{number}"));
        if used.insert(candidate.clone()) {
            return candidate;
        }
        number += 1;
    }
}

fn step_error(index: usize, reason: ReviewError) -> ReviewError {
    ReviewError::InvalidStep { index, reason: Box::new(reason) }
}

fn side_path(file: &FileDiff, side: AnchorSide) -> Option<&BytePath> {
    match side {
        AnchorSide::Old => file.old_side().map(|side| &side.path),
        AnchorSide::New => file.new_side().map(|side| &side.path),
    }
}
