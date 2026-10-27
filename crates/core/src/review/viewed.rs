//! Viewed marks: the file fingerprint each reader last marked viewed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BytePath, Fingerprint};

use super::{CURRENT_REVIEW_SCHEMA_VERSION, Result, Review, ReviewError, ReviewRevision, validate_text};

/// Maximum number of viewed marks one reader may hold.
pub const MAX_VIEWED_MARKS: usize = 10_000;
/// Maximum number of readers a review may hold viewed marks for.
pub const MAX_VIEWED_READERS: usize = 100;

const MAX_READER_ID_BYTES: usize = 256;

/// Viewed marks by reader id, each reader's marks sorted by path.
pub type ViewedMap = BTreeMap<String, Vec<ViewedMark>>;

/// The file fingerprint a reader marked viewed for one path, and the review
/// revision the reader read it at.
///
/// A file is viewed for the reader while its current fingerprint equals the
/// mark's. When they differ, the file changed after the reader viewed it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewedMark {
    path: BytePath,
    fingerprint: Fingerprint,
    revision: ReviewRevision,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

impl ViewedMark {
    /// Returns the file's canonical path: its new path, or its old path when
    /// the file was deleted.
    pub const fn path(&self) -> &BytePath {
        &self.path
    }

    /// Returns the file fingerprint the reader saw.
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Returns the review revision the reader read the file at.
    pub const fn revision(&self) -> ReviewRevision {
        self.revision
    }
}

/// Checks a reader id with the author id rule: 1 to 256 UTF-8 bytes.
pub fn validate_reader(reader: &str) -> Result<()> {
    validate_text("reader identifier", reader, MAX_READER_ID_BYTES)
}

/// Returns `review` with `reader`'s mark for `path` set, after checking that
/// the file had `fingerprint` at `revision`.
pub fn mark(
    review: &Review, reader: &str, path: BytePath, fingerprint: Fingerprint, revision: ReviewRevision,
) -> Result<Review> {
    validate_reader(reader)?;
    check_mark(review, &path, fingerprint, revision)?;
    let marks = review.viewed.get(reader).map_or(&[][..], Vec::as_slice);
    let position = marks.binary_search_by(|mark| mark.path.cmp(&path));
    if let Ok(index) = position
        && marks[index].fingerprint == fingerprint
        && marks[index].revision == revision
    {
        return Ok(review.clone());
    }
    if !review.viewed.contains_key(reader) && review.viewed.len() >= MAX_VIEWED_READERS {
        return Err(ReviewError::TooManyViewedReaders {
            actual: review.viewed.len().saturating_add(1),
            limit: MAX_VIEWED_READERS,
        });
    }
    if position.is_err() && marks.len() >= MAX_VIEWED_MARKS {
        return Err(ReviewError::TooManyViewedMarks { actual: marks.len().saturating_add(1), limit: MAX_VIEWED_MARKS });
    }

    let mut updated = review.clone();
    if updated.schema_version.minor < CURRENT_REVIEW_SCHEMA_VERSION.minor {
        updated.schema_version = CURRENT_REVIEW_SCHEMA_VERSION;
    }
    let marks = updated.viewed.entry(reader.to_owned()).or_default();
    let mark = ViewedMark { path, fingerprint, revision, extensions: BTreeMap::new() };
    match position {
        Ok(index) => marks[index] = mark,
        Err(index) => marks.insert(index, mark),
    }
    Ok(updated)
}

/// Returns `review` without `reader`'s mark for `path`. A missing mark leaves
/// the review unchanged.
pub fn unmark(review: &Review, reader: &str, path: &BytePath) -> Result<Review> {
    validate_reader(reader)?;
    let Some(index) = review
        .viewed
        .get(reader)
        .and_then(|marks| marks.binary_search_by(|mark| mark.path.cmp(path)).ok())
    else {
        return Ok(review.clone());
    };
    let mut updated = review.clone();
    if let Some(marks) = updated.viewed.get_mut(reader) {
        marks.remove(index);
        if marks.is_empty() {
            updated.viewed.remove(reader);
        }
    }
    Ok(updated)
}

/// Sorts each reader's marks by path.
pub fn canonicalize(viewed: &mut ViewedMap) {
    for marks in viewed.values_mut() {
        marks.sort_by(|left, right| left.path.cmp(&right.path));
    }
}

/// Checks reader ids, the reader and mark limits, and that no reader holds two
/// marks for one path. Marks must already be sorted by path.
pub fn validate(viewed: &ViewedMap) -> Result<()> {
    if viewed.len() > MAX_VIEWED_READERS {
        return Err(ReviewError::TooManyViewedReaders { actual: viewed.len(), limit: MAX_VIEWED_READERS });
    }
    for (reader, marks) in viewed {
        validate_reader(reader)?;
        if marks.len() > MAX_VIEWED_MARKS {
            return Err(ReviewError::TooManyViewedMarks { actual: marks.len(), limit: MAX_VIEWED_MARKS });
        }
        if let Some(pair) = marks.windows(2).find(|pair| pair[0].path == pair[1].path) {
            return Err(ReviewError::DuplicateViewedPath {
                reader: reader.clone(),
                path: String::from_utf8_lossy(pair[1].path.as_bytes()).into_owned(),
            });
        }
    }
    Ok(())
}

fn check_mark(review: &Review, path: &BytePath, fingerprint: Fingerprint, revision: ReviewRevision) -> Result<()> {
    // Only the current revision can be checked against the stored changeset.
    // A mark read at an older revision needs the file as it was then, which
    // the checkpoint lookup answers once the review records checkpoints.
    if revision != review.revision {
        return Err(ReviewError::ViewedMismatch);
    }
    let matches = review
        .changeset
        .files()
        .iter()
        .any(|file| file.canonical_path() == path && file.fingerprint() == fingerprint);
    if matches { Ok(()) } else { Err(ReviewError::ViewedMismatch) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangesetSource, PatchLimits, parse_patch};

    const PATCH: &[u8] =
        b"--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-one\n+two\n";

    #[test]
    fn marking_and_unmarking_keep_the_revision() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let marked = review
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        assert_eq!(marked.revision(), review.revision());
        assert!(marked.is_viewed("ana", file));
        assert!(!marked.is_viewed("ben", file));
        assert!(!marked.is_viewed("ana", &review.changeset().files()[1]));

        let again = marked
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        assert_eq!(again, marked);

        let cleared = marked.unmark_viewed("ana", file.canonical_path()).unwrap();
        assert_eq!(cleared.revision(), review.revision());
        assert!(cleared.viewed().is_empty());
        assert_eq!(cleared.unmark_viewed("ana", file.canonical_path()).unwrap(), cleared);
    }

    #[test]
    fn a_mark_must_name_the_current_fingerprint_and_revision() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let other = review.changeset().files()[1].fingerprint();
        let path = file.canonical_path().clone();
        assert_eq!(
            review.mark_viewed("ana", path.clone(), other, review.revision()),
            Err(ReviewError::ViewedMismatch)
        );
        let later = ReviewRevision::new(2).unwrap();
        assert_eq!(
            review.mark_viewed("ana", path, file.fingerprint(), later),
            Err(ReviewError::ViewedMismatch)
        );
        let absent = BytePath::new(b"c.txt".to_vec()).unwrap();
        assert_eq!(
            review.mark_viewed("ana", absent, file.fingerprint(), review.revision()),
            Err(ReviewError::ViewedMismatch)
        );
        assert!(matches!(
            review.mark_viewed("", file.canonical_path().clone(), file.fingerprint(), review.revision()),
            Err(ReviewError::InvalidTextField { field: "reader identifier", .. })
        ));
    }

    #[test]
    fn a_fingerprint_change_leaves_the_mark_but_not_viewed() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let marked = review
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        let changed = parse_patch(
            b"--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+newer\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-one\n+two\n",
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let refreshed = marked.reanchor(changed).unwrap();
        assert_eq!(refreshed.revision().get(), 2);
        let file = &refreshed.changeset().files()[0];
        let mark = refreshed.viewed_mark("ana", file.canonical_path()).unwrap();
        assert_eq!(mark.revision().get(), 1);
        assert_ne!(mark.fingerprint(), file.fingerprint());
        assert!(!refreshed.is_viewed("ana", file));

        let remarked = refreshed
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                refreshed.revision(),
            )
            .unwrap();
        assert!(remarked.is_viewed("ana", file));
        assert_eq!(remarked.viewed_marks("ana").len(), 1);
    }

    #[test]
    fn marks_round_trip_and_older_files_load_without_marks() {
        let review = review(PATCH);
        let file = &review.changeset().files()[1];
        let marked = review
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        let json = serde_json::to_string(&marked).unwrap();
        assert!(json.contains("\"viewed\":{\"ana\":[{\"path\":"));
        let decoded: Review = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, marked);

        let older = serde_json::to_string(&review).unwrap().replacen(
            &format!("\"minor\":{}", CURRENT_REVIEW_SCHEMA_VERSION.minor),
            "\"minor\":5",
            1,
        );
        assert!(older.contains("\"minor\":5") && !older.contains("\"viewed\""));
        let decoded: Review = serde_json::from_str(&older).unwrap();
        assert!(decoded.viewed().is_empty());
        let upgraded = decoded
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        assert_eq!(upgraded.schema_version(), CURRENT_REVIEW_SCHEMA_VERSION);
    }

    #[test]
    fn note_writes_and_carried_reviews_keep_marks() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let marked = review
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        let titled = marked.clone().with_metadata(Some("Title".to_owned()), None).unwrap();
        assert_eq!(titled.viewed(), marked.viewed());
        assert_eq!(review.clone().with_viewed_from(&marked).viewed(), marked.viewed());
    }

    #[test]
    fn limits_and_duplicates_are_rejected() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let mut crowded = review.clone();
        for index in 0..MAX_VIEWED_READERS {
            crowded = crowded
                .mark_viewed(
                    &format!("reader-{index}"),
                    file.canonical_path().clone(),
                    file.fingerprint(),
                    review.revision(),
                )
                .unwrap();
        }
        assert_eq!(
            crowded.mark_viewed(
                "one-more",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision()
            ),
            Err(ReviewError::TooManyViewedReaders { actual: MAX_VIEWED_READERS + 1, limit: MAX_VIEWED_READERS })
        );

        let mark = ViewedMark {
            path: file.canonical_path().clone(),
            fingerprint: file.fingerprint(),
            revision: review.revision(),
            extensions: BTreeMap::new(),
        };
        let mut viewed = ViewedMap::new();
        viewed.insert("ana".to_owned(), vec![mark.clone(), mark.clone()]);
        assert!(matches!(
            validate(&viewed),
            Err(ReviewError::DuplicateViewedPath { .. })
        ));
        viewed.insert("ana".to_owned(), vec![mark; MAX_VIEWED_MARKS + 1]);
        assert_eq!(
            validate(&viewed),
            Err(ReviewError::TooManyViewedMarks { actual: MAX_VIEWED_MARKS + 1, limit: MAX_VIEWED_MARKS })
        );
    }

    fn review(patch: &[u8]) -> Review {
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
    }
}
