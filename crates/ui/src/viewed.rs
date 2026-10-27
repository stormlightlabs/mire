//! Viewed marks in the terminal: toggling the selected file and the write the
//! persistence boundary makes for it.

use mire_core::{BytePath, FileDiff, Fingerprint, Review, ReviewError, ReviewRevision};

/// Whether the reader has viewed a changed file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileViewed {
    /// The reader has no mark for the file's path.
    #[default]
    NotViewed,
    /// The reader's mark holds the file's current fingerprint.
    Viewed,
    /// The reader marked the file viewed, and it has changed since.
    Changed,
}

/// One change the terminal asks the persistence boundary to write.
#[derive(Clone, Copy, Debug)]
pub enum ReviewWrite<'a> {
    /// The review after a note or walkthrough change that advanced its
    /// revision.
    Revision(&'a Review),
    /// A viewed mark the reader set or cleared. The revision stays the same,
    /// so the boundary applies it to the review file as it is on disk.
    Viewed(&'a ViewedChange),
}

/// A viewed mark the reader set or cleared for one path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewedChange {
    /// Reader the mark belongs to.
    pub reader: String,
    /// Canonical path of the changed file.
    pub path: BytePath,
    /// The fingerprint and revision the reader saw, or `None` to clear the
    /// mark.
    pub mark: Option<(Fingerprint, ReviewRevision)>,
}

impl ViewedChange {
    /// Applies the change to `review`, such as the review file as stored.
    pub fn apply(&self, review: &Review) -> Result<Review, ReviewError> {
        match self.mark {
            Some((fingerprint, revision)) => review.mark_viewed(&self.reader, self.path.clone(), fingerprint, revision),
            None => review.unmark_viewed(&self.reader, &self.path),
        }
    }
}

/// Reports whether `reader` has viewed `file` as it is now.
pub fn file_viewed(review: &Review, reader: &str, file: &FileDiff) -> FileViewed {
    match review.viewed_mark(reader, file.canonical_path()) {
        None => FileViewed::NotViewed,
        Some(mark) if mark.fingerprint() == file.fingerprint() => FileViewed::Viewed,
        Some(_) => FileViewed::Changed,
    }
}

/// Builds the change that toggles `file`: a viewed file is cleared, and any
/// other file is marked viewed at its current fingerprint.
pub fn toggle(review: &Review, reader: &str, file: &FileDiff) -> ViewedChange {
    let mark = match file_viewed(review, reader, file) {
        FileViewed::Viewed => None,
        FileViewed::NotViewed | FileViewed::Changed => Some((file.fingerprint(), review.revision())),
    };
    ViewedChange { reader: reader.to_owned(), path: file.canonical_path().clone(), mark }
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    fn review(content: &str) -> Review {
        let patch = format!("--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+{content}\n");
        let changeset = parse_patch(
            patch.as_bytes(),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
    }

    #[test]
    fn toggling_marks_clears_and_remarks_a_changed_file() {
        let review = review("new");
        let file = &review.changeset().files()[0];
        assert_eq!(file_viewed(&review, "ana", file), FileViewed::NotViewed);

        let change = toggle(&review, "ana", file);
        assert_eq!(change.mark, Some((file.fingerprint(), review.revision())));
        let marked = change.apply(&review).unwrap();
        assert_eq!(file_viewed(&marked, "ana", file), FileViewed::Viewed);

        let clear = toggle(&marked, "ana", file);
        assert_eq!(clear.mark, None);
        assert_eq!(
            file_viewed(&clear.apply(&marked).unwrap(), "ana", file),
            FileViewed::NotViewed
        );

        let refreshed = marked.reanchor(review_changeset("newer")).unwrap();
        let file = &refreshed.changeset().files()[0];
        assert_eq!(file_viewed(&refreshed, "ana", file), FileViewed::Changed);
        let remark = toggle(&refreshed, "ana", file);
        assert_eq!(remark.mark, Some((file.fingerprint(), refreshed.revision())));
    }

    fn review_changeset(content: &str) -> mire_core::Changeset {
        review(content).changeset().clone()
    }
}
