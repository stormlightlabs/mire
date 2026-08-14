use mire_core::{
    Anchor, AnchorSide, Author, BytePath, Changeset, ChangesetSource, FileContent, LineNumber, LineRange, NoteId,
    NoteSeverity, NoteStatus, PatchLimits, Provenance, ReanchorOutcome, Review, ReviewNote, ReviewRevision,
    parse_patch,
};

#[test]
fn reanchor_classifies_exact_moved_stale_and_ambiguous_notes() {
    let original = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n");
    let note = note(&original, "file.txt", 1);

    let exact_capture = changeset(
        b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n--- a/other.txt\n+++ b/other.txt\n@@ -1 +1 @@\n-a\n+b\n",
    );
    let exact = review(original.clone(), note.clone()).reanchor(exact_capture).unwrap();
    assert!(matches!(
        exact.notes()[0].reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));

    let moved_capture =
        changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,3 @@\n context\n+inserted\n-old\n+bug\n");
    let moved = review(original.clone(), note.clone()).reanchor(moved_capture).unwrap();
    let Some(ReanchorOutcome::Moved { candidate, .. }) = moved.notes()[0].reanchor_outcome() else {
        panic!("note should move to one content-supported location");
    };
    assert_eq!(candidate.anchor().range().start().get(), 3);

    let stale_capture = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+fixed\n");
    let stale = review(original.clone(), note.clone()).reanchor(stale_capture).unwrap();
    let Some(ReanchorOutcome::Stale { evidence, .. }) = stale.notes()[0].reanchor_outcome() else {
        panic!("changed selected content should become stale");
    };
    assert!(evidence.path_match());
    assert!(!evidence.content_match());
    assert!(stale.notes()[0].current_anchor().is_none());
    let restored = stale.reanchor(original.clone()).unwrap();
    assert!(matches!(
        restored.notes()[0].reanchor_outcome(),
        Some(ReanchorOutcome::Exact { .. })
    ));

    let ambiguous_capture = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1,2 @@\n-old\n+bug\n+bug\n");
    let ambiguous = review(original, note).reanchor(ambiguous_capture).unwrap();
    let Some(ReanchorOutcome::Ambiguous { candidates, .. }) = ambiguous.notes()[0].reanchor_outcome() else {
        panic!("duplicate content should remain ambiguous");
    };
    assert_eq!(candidates.len(), 2);
}

#[test]
fn reanchor_handles_renames_whitespace_edits_deleted_lines_and_filtered_captures() {
    let original = changeset(b"--- a/old.txt\n+++ b/old.txt\n@@ -1 +1 @@\n-old\n+bug\n");
    let note = note(&original, "old.txt", 1);

    let renamed_capture = changeset(
        b"diff --git a/old.txt b/new.txt\nsimilarity index 80%\nrename from old.txt\nrename to new.txt\n--- a/old.txt\n+++ b/new.txt\n@@ -1,2 +1,2 @@\n-old\n+bug\n context\n",
    );
    let renamed = review(original.clone(), note.clone())
        .reanchor(renamed_capture)
        .unwrap();
    let Some(ReanchorOutcome::Moved { candidate, .. }) = renamed.notes()[0].reanchor_outcome() else {
        panic!("a unique candidate across an explicit rename should move");
    };
    assert_eq!(candidate.anchor().path().as_bytes(), b"new.txt");
    assert!(!candidate.evidence().path_match());

    for capture in [
        changeset(b"--- a/old.txt\n+++ b/old.txt\n@@ -1 +1 @@\n-old\n+bug \n"),
        changeset(b"--- a/old.txt\n+++ b/old.txt\n@@ -1 +1 @@\n-old\n+fixed\n"),
        changeset(b"--- a/other.txt\n+++ b/other.txt\n@@ -1 +1 @@\n-a\n+b\n"),
    ] {
        let refreshed = review(original.clone(), note.clone()).reanchor(capture).unwrap();
        assert!(matches!(
            refreshed.notes()[0].reanchor_outcome(),
            Some(ReanchorOutcome::Stale { .. })
        ));
    }
}

#[test]
fn reanchor_unique_content_moves_to_every_generated_line_offset() {
    let original = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n");
    let note = note(&original, "file.txt", 1);
    for offset in 1..32_u64 {
        let mut patch = format!("--- a/file.txt\n+++ b/file.txt\n@@ -1 +1,{} @@\n-old\n", offset + 1);
        for line in 0..offset {
            patch.push_str(&format!("+padding-{line}\n"));
        }
        patch.push_str("+bug\n");
        let refreshed = review(original.clone(), note.clone())
            .reanchor(changeset(patch.as_bytes()))
            .unwrap();
        let Some(ReanchorOutcome::Moved { candidate, .. }) = refreshed.notes()[0].reanchor_outcome() else {
            panic!("unique content should move at offset {offset}");
        };
        assert_eq!(candidate.anchor().range().start().get(), offset + 1);
    }
}

#[test]
fn reanchor_unchanged_capture_is_a_noop_and_overflow_leaves_the_review_unchanged() {
    let original = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n");
    let note = note(&original, "file.txt", 1);
    let review = review(original.clone(), note);
    assert_eq!(review.reanchor(original).unwrap(), review);

    let overflow = Review::new(
        ReviewRevision::new(u64::MAX).unwrap(),
        review.changeset().clone(),
        review.notes().to_vec(),
        review.events().to_vec(),
    )
    .unwrap();
    let changed = changeset(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+fixed\n");
    assert!(overflow.reanchor(changed).is_err());
    assert_eq!(overflow.revision().get(), u64::MAX);
}

fn changeset(patch: &[u8]) -> Changeset {
    parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap()
}

fn note(changeset: &Changeset, path: &str, line: u64) -> ReviewNote {
    let file = changeset
        .files()
        .iter()
        .find(|file| {
            file.new_side()
                .is_some_and(|side| side.path.as_bytes() == path.as_bytes())
        })
        .unwrap();
    let FileContent::Text { hunks } = file.content() else {
        panic!("fixture is textual");
    };
    let number = LineNumber::new(line).unwrap();
    let anchor = Anchor::new(
        changeset,
        BytePath::new(path.as_bytes()).unwrap(),
        AnchorSide::New,
        LineRange::new(number, number).unwrap(),
        hunks[0].fingerprint(),
    )
    .unwrap();
    ReviewNote::new(
        NoteId::new("finding").unwrap(),
        anchor,
        Author::new("agent", None).unwrap(),
        NoteSeverity::High,
        NoteStatus::Open,
        "evidence".to_owned(),
        Provenance::Agent { producer: "test".to_owned() },
    )
    .unwrap()
}

fn review(changeset: Changeset, note: ReviewNote) -> Review {
    let author = note.author().clone();
    let id = note.id().clone();
    Review::new(
        ReviewRevision::new(1).unwrap(),
        changeset,
        vec![note],
        vec![
            mire_core::NoteEvent::new(
                1,
                id,
                author,
                mire_core::NoteEventKind::Created { status: NoteStatus::Open },
            )
            .unwrap(),
        ],
    )
    .unwrap()
}
