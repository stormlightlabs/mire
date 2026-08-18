//! Source-backed review refresh orchestration shared by the CLI and web server.

use std::path::Path;

use mire_core::{Review, ReviewError};
use thiserror::Error;

use crate::git::{self, GitError};
use crate::review_file::{ReviewFileError, read_review, write_review_atomic_if_revision};

const MAX_REFRESH_ATTEMPTS: usize = 8;

/// The result of refreshing a review from its bound source.
#[derive(Debug)]
pub struct RefreshResult {
    review: Review,
    changed: bool,
}

impl RefreshResult {
    /// Returns the current review, whether or not its capture changed.
    pub const fn review(&self) -> &Review {
        &self.review
    }

    /// Reports whether the refresh wrote a new review revision.
    pub const fn changed(&self) -> bool {
        self.changed
    }
}

/// A source refresh failure.
#[derive(Debug, Error)]
pub enum RefreshError {
    /// The durable review could not be read or atomically updated.
    #[error(transparent)]
    Review(#[from] ReviewFileError),
    /// The review was not created from a reloadable source binding.
    #[error("review has no reloadable source binding")]
    UnavailableSource,
    /// The bound Git source could not be read.
    #[error(transparent)]
    Git(#[from] GitError),
    /// The refreshed capture could not be re-anchored safely.
    #[error(transparent)]
    Reanchor(#[from] ReviewError),
}

/// Repeats a bound Git comparison, re-anchors its notes, and atomically saves it.
///
/// Concurrent edits are retried from the latest durable revision. After repeated
/// lock contention, the final lock error is returned without overwriting data.
pub fn refresh_review(path: &Path) -> Result<RefreshResult, RefreshError> {
    for _ in 0..MAX_REFRESH_ATTEMPTS {
        let review = read_review(path)?;
        let binding = review.source_binding().ok_or(RefreshError::UnavailableSource)?;
        let changeset = git::load_bound_diff(binding)?;
        let refreshed = review.reanchor(changeset)?;
        if refreshed == review {
            return Ok(RefreshResult { review, changed: false });
        }
        match write_review_atomic_if_revision(path, review.revision(), &refreshed) {
            Ok(()) => return Ok(RefreshResult { review: refreshed, changed: true }),
            Err(ReviewFileError::RevisionConflict { .. } | ReviewFileError::Locked { .. }) => continue,
            Err(error) => return Err(RefreshError::Review(error)),
        }
    }
    Err(RefreshError::Review(ReviewFileError::Locked { path: path.to_owned() }))
}
