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
    match position {
        Ok(index) => {
            // Keep fields a newer Mire stored on the mark.
            marks[index].fingerprint = fingerprint;
            marks[index].revision = revision;
        }
        Err(index) => marks.insert(
            index,
            ViewedMark { path, fingerprint, revision, extensions: BTreeMap::new() },
        ),
    }
    Ok(updated)
}

/// Returns `review` without `reader`'s mark for `path`, whether or not the
/// path is still in the changeset. A missing mark leaves the review unchanged.
/// Removing a reader's last mark drops the reader, freeing its slot.
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

/// Returns `review` without any of `reader`'s marks, and the number removed.
/// The reader is dropped, freeing its slot.
pub fn unmark_all(review: &Review, reader: &str) -> Result<(Review, usize)> {
    validate_reader(reader)?;
    let Some(count) = review.viewed.get(reader).map(Vec::len) else {
        return Ok((review.clone(), 0));
    };
    let mut updated = review.clone();
    updated.viewed.remove(reader);
    Ok((updated, count))
}

/// Sorts each reader's marks by path.
pub fn canonicalize(viewed: &mut ViewedMap) {
    for marks in viewed.values_mut() {
        marks.sort_by(|left, right| left.path.cmp(&right.path));
    }
}

/// Checks reader ids and that no reader holds two marks for one path. Marks
/// must already be sorted by path.
///
/// The reader and mark limits are not checked here: [`mark`] enforces them
/// when it adds a mark or a reader, so a file past them still loads and its
/// marks can still be cleared. The review file size limit bounds what a load
/// reads. A duplicate path is still rejected, because lookups assume one mark
/// per path and Mire never writes two.
pub fn validate(viewed: &ViewedMap) -> Result<()> {
    for (reader, marks) in viewed {
        validate_reader(reader)?;
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
    // The checkpoint lookup gives the file as it was at the mark's revision,
    // so a reader that loaded an older revision can still mark what it saw.
    match review.file_at_revision(path, revision) {
        Some((_, row)) if row.fingerprint() == Some(fingerprint) => Ok(()),
        _ => Err(ReviewError::ViewedMismatch),
    }
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
    fn duplicates_are_rejected_on_load() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let mark = ViewedMark {
            path: file.canonical_path().clone(),
            fingerprint: file.fingerprint(),
            revision: review.revision(),
            extensions: BTreeMap::new(),
        };
        let mut viewed = ViewedMap::new();
        viewed.insert("ana".to_owned(), vec![mark.clone(), mark]);
        assert!(matches!(
            validate(&viewed),
            Err(ReviewError::DuplicateViewedPath { .. })
        ));
    }

    #[test]
    fn clearing_a_readers_last_mark_frees_its_slot() {
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
        let one_more = |review: &Review| {
            review.mark_viewed(
                "one-more",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
        };
        assert_eq!(
            one_more(&crowded),
            Err(ReviewError::TooManyViewedReaders { actual: MAX_VIEWED_READERS + 1, limit: MAX_VIEWED_READERS })
        );
        let freed = crowded.unmark_viewed("reader-0", file.canonical_path()).unwrap();
        assert!(!freed.viewed().contains_key("reader-0"));
        assert_eq!(freed.viewed().len(), MAX_VIEWED_READERS - 1);
        assert!(one_more(&freed).is_ok());
    }

    #[test]
    fn a_mark_on_a_departed_path_stays_until_cleared() {
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
        let without_a = parse_patch(
            b"--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-one\n+two\n",
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let refreshed = marked.reanchor(without_a).unwrap();
        let path = file.canonical_path();
        assert!(
            refreshed
                .changeset()
                .files()
                .iter()
                .all(|file| file.canonical_path() != path)
        );
        assert!(refreshed.viewed_mark("ana", path).is_some());

        let cleared = refreshed.unmark_viewed("ana", path).unwrap();
        assert!(cleared.viewed().is_empty());
    }

    #[test]
    fn clearing_all_of_a_readers_marks_drops_the_reader() {
        let review = review(PATCH);
        let mut marked = review.clone();
        for reader in ["ana", "ben"] {
            for file in review.changeset().files() {
                marked = marked
                    .mark_viewed(
                        reader,
                        file.canonical_path().clone(),
                        file.fingerprint(),
                        review.revision(),
                    )
                    .unwrap();
            }
        }
        let (cleared, count) = marked.unmark_all_viewed("ana").unwrap();
        assert_eq!(count, 2);
        assert_eq!(cleared.revision(), review.revision());
        assert!(!cleared.viewed().contains_key("ana"));
        assert_eq!(cleared.viewed_marks("ben").len(), 2);
        assert_eq!(cleared.unmark_all_viewed("ana").unwrap(), (cleared.clone(), 0));
        assert!(matches!(
            cleared.unmark_all_viewed(""),
            Err(ReviewError::InvalidTextField { field: "reader identifier", .. })
        ));
    }

    #[test]
    fn a_review_past_the_reader_limit_loads_and_only_shrinks() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let mark = ViewedMark {
            path: file.canonical_path().clone(),
            fingerprint: file.fingerprint(),
            revision: review.revision(),
            extensions: BTreeMap::new(),
        };
        let mut crowded = review.clone();
        for index in 0..=MAX_VIEWED_READERS {
            crowded.viewed.insert(format!("reader-{index}"), vec![mark.clone()]);
        }
        let json = serde_json::to_string(&crowded).unwrap();
        let loaded: Review = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.viewed().len(), MAX_VIEWED_READERS + 1);

        let other = &review.changeset().files()[1];
        assert!(
            loaded
                .mark_viewed(
                    "reader-0",
                    other.canonical_path().clone(),
                    other.fingerprint(),
                    review.revision()
                )
                .is_ok()
        );
        assert_eq!(
            loaded.mark_viewed(
                "new",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision()
            ),
            Err(ReviewError::TooManyViewedReaders { actual: MAX_VIEWED_READERS + 2, limit: MAX_VIEWED_READERS })
        );
        let shrunk = loaded.unmark_viewed("reader-0", file.canonical_path()).unwrap();
        assert_eq!(shrunk.viewed().len(), MAX_VIEWED_READERS);
        let (shrunk, _) = shrunk.unmark_all_viewed("reader-1").unwrap();
        assert_eq!(shrunk.viewed().len(), MAX_VIEWED_READERS - 1);
    }

    #[test]
    fn a_reader_past_the_mark_limit_loads_and_only_shrinks() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let marks = (0..=MAX_VIEWED_MARKS)
            .map(|index| ViewedMark {
                path: BytePath::new(format!("z/{index:05}.txt").into_bytes()).unwrap(),
                fingerprint: file.fingerprint(),
                revision: review.revision(),
                extensions: BTreeMap::new(),
            })
            .collect();
        let mut crowded = review.clone();
        crowded.viewed.insert("ana".to_owned(), marks);
        let json = serde_json::to_string(&crowded).unwrap();
        let loaded: Review = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.viewed_marks("ana").len(), MAX_VIEWED_MARKS + 1);

        assert_eq!(
            loaded.mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision()
            ),
            Err(ReviewError::TooManyViewedMarks { actual: MAX_VIEWED_MARKS + 2, limit: MAX_VIEWED_MARKS })
        );
        let departed = BytePath::new(b"z/00000.txt".to_vec()).unwrap();
        let shrunk = loaded.unmark_viewed("ana", &departed).unwrap();
        assert_eq!(shrunk.viewed_marks("ana").len(), MAX_VIEWED_MARKS);
        assert!(
            shrunk
                .mark_viewed(
                    "ana",
                    file.canonical_path().clone(),
                    file.fingerprint(),
                    review.revision()
                )
                .is_err()
        );
    }

    #[test]
    fn re_marking_keeps_fields_a_newer_mire_stored() {
        let review = review(PATCH);
        let file = &review.changeset().files()[0];
        let mut marked = review
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                review.revision(),
            )
            .unwrap();
        let extension = (String::from("seen_by"), Value::String(String::from("tui")));
        marked.viewed.get_mut("ana").unwrap()[0]
            .extensions
            .extend([extension.clone()]);
        let changed = parse_patch(
            b"--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+newer\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-one\n+two\n",
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let refreshed = marked.reanchor(changed).unwrap();
        let file = &refreshed.changeset().files()[0];
        let remarked = refreshed
            .mark_viewed(
                "ana",
                file.canonical_path().clone(),
                file.fingerprint(),
                refreshed.revision(),
            )
            .unwrap();
        let mark = &remarked.viewed_marks("ana")[0];
        assert_eq!(mark.fingerprint(), file.fingerprint());
        assert_eq!(mark.revision(), refreshed.revision());
        assert_eq!(mark.extensions.get(&extension.0), Some(&extension.1));
        let json = serde_json::to_string(&remarked).unwrap();
        assert!(json.contains("\"seen_by\":\"tui\""));
    }

    fn review(patch: &[u8]) -> Review {
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
    }
}
