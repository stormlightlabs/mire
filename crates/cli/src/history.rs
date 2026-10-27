//! Checkpoint reads and patch copies: which revisions changed a file, its
//! hunks at one of them, and which new checkpoint rows need a copy because Git
//! does not hold their blobs.

use std::collections::BTreeSet;

use mire_core::{BytePath, CheckpointRow, FileContent, FileDiff, ObjectId, Review, ReviewRevision};

use crate::git;

/// Where Mire found a file's hunks at a checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointSource {
    /// The current capture: the file has not changed since the checkpoint.
    Capture,
    /// The two blobs, read from the bound repository.
    Git,
    /// The patch copy kept with the row.
    Copy,
}

/// Why a file's hunks at a revision cannot be shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointFailure {
    /// History does not reach the revision, never named the path, or the file
    /// was not in the changeset at that revision.
    NotFound,
    /// The row exists, but Git no longer holds its blobs and no copy was kept.
    Unavailable,
}

/// A file's hunks at a checkpoint and where they came from.
#[derive(Clone, Debug)]
pub struct CheckpointContent {
    /// Where the hunks came from.
    pub source: CheckpointSource,
    /// The file's hunks, or the binary marker.
    pub content: FileContent,
}

impl CheckpointSource {
    /// Returns the name the CLI and the API print.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Git => "git",
            Self::Copy => "copy",
        }
    }
}

impl CheckpointFailure {
    /// Returns the stable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotFound => "checkpoint_not_found",
            Self::Unavailable => "checkpoint_unavailable",
        }
    }

    /// Returns a short description of the failure.
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotFound => "The file has no checkpoint at this revision.",
            Self::Unavailable => "Git no longer holds this file's blobs and Mire kept no patch copy.",
        }
    }
}

/// Keeps patch copies on the rows the current revision's checkpoint recorded
/// when the bound repository does not hold their blobs. A review without a
/// source binding keeps none.
///
/// A failed Git check returns the error rather than a guess. Reading it as
/// "every blob missing" would spend the history budget on copies Git does not
/// need and compact older copies to make room; reading it as "none missing"
/// would skip copies Git cannot replace.
pub fn keep_missing_copies(review: Review) -> Result<Review, git::GitError> {
    let Some(binding) = review.source_binding() else {
        return Ok(review);
    };
    let Some(checkpoint) = review
        .checkpoints()
        .last()
        .filter(|checkpoint| checkpoint.revision() == review.revision())
    else {
        return Ok(review);
    };
    let objects = checkpoint
        .files()
        .iter()
        .flat_map(|row| row.old_object().into_iter().chain(row.new_object()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing = git::missing_blobs(binding, &objects)?;
    Ok(review.keep_checkpoint_copies(|file| needs_copy(file, &missing)))
}

/// Returns the blob ids the review's checkpoint rows and pinned rows name.
pub fn history_objects(review: &Review) -> BTreeSet<ObjectId> {
    review
        .checkpoints()
        .iter()
        .chain(review.pinned_checkpoints())
        .flat_map(|checkpoint| checkpoint.files())
        .flat_map(|row| row.old_object().into_iter().chain(row.new_object()))
        .cloned()
        .collect()
}

/// Returns the ids among `objects` that the bound repository holds. A review
/// without a source binding holds none.
///
/// A failed Git check returns the error: the caller reports availability as
/// unknown rather than calling every row unavailable.
pub fn held_blobs(review: &Review, objects: &BTreeSet<ObjectId>) -> Result<BTreeSet<ObjectId>, git::GitError> {
    let Some(binding) = review.source_binding() else {
        return Ok(BTreeSet::new());
    };
    let missing = git::missing_blobs(binding, objects)?;
    Ok(objects.difference(&missing).cloned().collect())
}

/// Reports whether Mire can show a row's hunks: the review holds them, Git
/// holds every blob the row names, or a copy was kept. `held` names the blobs
/// the bound repository holds, or is `None` when the Git check failed; a row
/// only Git could rebuild is then `None`, unknown.
pub fn is_available(review: &Review, row: &CheckpointRow, held: Option<&BTreeSet<ObjectId>>) -> Option<bool> {
    if row.fingerprint().is_none() {
        return Some(false);
    }
    if current_file(review, row).is_some() || row.copy().hunks().is_some() {
        return Some(true);
    }
    let ids = row.old_object().into_iter().chain(row.new_object()).collect::<Vec<_>>();
    if ids.is_empty() {
        return Some(false);
    }
    held.map(|held| ids.iter().all(|id| held.contains(*id)))
}

/// Returns a file's hunks at a revision, with the revision of the checkpoint
/// row they come from.
pub fn file_at_revision<'a>(
    review: &'a Review, path: &BytePath, revision: ReviewRevision,
) -> Result<(ReviewRevision, &'a CheckpointRow, CheckpointContent), CheckpointFailure> {
    let (checkpoint_revision, row) = review
        .file_at_revision(path, revision)
        .filter(|(_, row)| row.fingerprint().is_some())
        .ok_or(CheckpointFailure::NotFound)?;
    let content = rebuild(review, row)?;
    Ok((checkpoint_revision, row, content))
}

/// Rebuilds a row's hunks: from the current capture when the file has not
/// changed since, then from Git blobs, then from the patch copy.
pub fn rebuild(review: &Review, row: &CheckpointRow) -> Result<CheckpointContent, CheckpointFailure> {
    if row.fingerprint().is_none() {
        return Err(CheckpointFailure::NotFound);
    }
    if let Some(file) = current_file(review, row) {
        return Ok(CheckpointContent { source: CheckpointSource::Capture, content: file.content().clone() });
    }
    if let Some(binding) = review.source_binding()
        && let Ok(Some(content)) = git::blob_content(binding, row.old_object(), row.new_object())
    {
        return Ok(CheckpointContent { source: CheckpointSource::Git, content });
    }
    match row.copy().hunks() {
        Some(hunks) => Ok(CheckpointContent {
            source: CheckpointSource::Copy,
            content: FileContent::Text { hunks: hunks.to_vec() },
        }),
        None => Err(CheckpointFailure::Unavailable),
    }
}

fn current_file<'a>(review: &'a Review, row: &CheckpointRow) -> Option<&'a FileDiff> {
    review
        .changeset()
        .files()
        .iter()
        .find(|file| file.canonical_path() == row.path() && Some(file.fingerprint()) == row.fingerprint())
}

fn needs_copy(file: &FileDiff, missing: &BTreeSet<ObjectId>) -> bool {
    file.old_side()
        .into_iter()
        .chain(file.new_side())
        .any(|side| side.object.as_ref().is_none_or(|id| missing.contains(id)))
}
