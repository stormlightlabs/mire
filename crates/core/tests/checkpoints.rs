use mire_core::{
    AnchorSide, AnnotationKind, Author, BytePath, Changeset, ChangesetSource, LineNumber, LineRange, MAX_HISTORY_BYTES,
    MAX_PATCH_COPY_BYTES, NoteInput, NoteSeverity, PatchCopy, PatchLimits, Provenance, Review, ReviewError,
    ReviewRevision, parse_patch,
};
use serde_json::Value;

#[test]
fn the_first_revision_names_every_file() {
    let review = review(&[("a.txt", "new"), ("b.txt", "two")]);
    let checkpoints = review.checkpoints();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].revision(), revision(1));
    assert!(!checkpoints[0].is_baseline());
    let paths = checkpoints[0]
        .files()
        .iter()
        .map(|row| row.path().clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [path("a.txt"), path("b.txt")]);
    for (row, file) in checkpoints[0].files().iter().zip(review.changeset().files()) {
        assert_eq!(row.fingerprint(), Some(file.fingerprint()));
        assert_eq!(row.copy(), &PatchCopy::Skipped);
    }
}

#[test]
fn a_refresh_records_only_the_files_that_changed() {
    let first = review(&[("a.txt", "new"), ("b.txt", "two")]);
    let second = first
        .reanchor(changeset(&[("a.txt", "newer"), ("b.txt", "two")]))
        .unwrap();
    assert_eq!(second.revision(), revision(2));
    let latest = second.checkpoints().last().unwrap();
    assert_eq!(latest.revision(), revision(2));
    assert_eq!(latest.files().len(), 1);
    assert_eq!(latest.files()[0].path(), &path("a.txt"));

    let (at, row) = second.file_at_revision(&path("a.txt"), revision(1)).unwrap();
    assert_eq!(
        (at, row.fingerprint()),
        (revision(1), Some(first.changeset().files()[0].fingerprint()))
    );
    let (at, row) = second.file_at_revision(&path("a.txt"), revision(2)).unwrap();
    assert_eq!(
        (at, row.fingerprint()),
        (revision(2), Some(second.changeset().files()[0].fingerprint()))
    );
    let (at, _) = second.file_at_revision(&path("b.txt"), revision(2)).unwrap();
    assert_eq!(at, revision(1));
    assert!(second.file_at_revision(&path("a.txt"), revision(3)).is_none());

    let third = second.reanchor(changeset(&[("a.txt", "newer")])).unwrap();
    let latest = third.checkpoints().last().unwrap();
    assert_eq!(latest.files().len(), 1);
    assert_eq!(latest.files()[0].path(), &path("b.txt"));
    assert_eq!(latest.files()[0].fingerprint(), None);

    let note_only = third.apply_notes(vec![note_on("a.txt")]).unwrap();
    assert_eq!(note_only.checkpoints(), third.checkpoints());
}

#[test]
fn a_revert_records_the_earlier_fingerprint_again() {
    let first = review(&[("a.txt", "new")]);
    let second = first.reanchor(changeset(&[("a.txt", "newer")])).unwrap();
    let third = second.reanchor(changeset(&[("a.txt", "new")])).unwrap();
    let original = first.changeset().files()[0].fingerprint();
    let changed = second.changeset().files()[0].fingerprint();
    assert_eq!(third.changeset().files()[0].fingerprint(), original);

    let history = third
        .file_history(&path("a.txt"))
        .into_iter()
        .map(|(checkpoint, row)| (checkpoint.revision().get(), row.fingerprint()))
        .collect::<Vec<_>>();
    assert_eq!(history, [(3, Some(original)), (2, Some(changed)), (1, Some(original))]);

    // A viewed mark is checked against the file at the mark's revision.
    let a = path("a.txt");
    assert!(third.mark_viewed("ana", a.clone(), original, revision(1)).is_ok());
    assert!(third.mark_viewed("ana", a.clone(), changed, revision(2)).is_ok());
    assert!(third.mark_viewed("ana", a.clone(), original, revision(3)).is_ok());
    assert_eq!(
        third.mark_viewed("ana", a.clone(), changed, revision(1)),
        Err(ReviewError::ViewedMismatch)
    );
    assert_eq!(
        third.mark_viewed("ana", a, original, revision(2)),
        Err(ReviewError::ViewedMismatch)
    );
}

#[test]
fn copies_are_kept_within_their_bound_and_round_trip() {
    let large = "x".repeat(MAX_PATCH_COPY_BYTES / 3);
    let review = review(&[("a.txt", "new"), ("b.txt", &large), ("c.txt", "kept")]);
    let kept = review
        .clone()
        .keep_checkpoint_copies(|file| file.canonical_path() != &path("c.txt"));
    let rows = kept.checkpoints()[0].files();
    assert!(rows[0].copy().hunks().is_some(), "a small copy is kept");
    assert_eq!(rows[1].copy(), &PatchCopy::Skipped, "a copy over 1 MiB is skipped");
    assert_eq!(rows[2].copy(), &PatchCopy::Skipped, "a file Git holds needs no copy");
    assert_eq!(
        rows[0].copy().hunks().unwrap(),
        match kept.changeset().files()[0].content() {
            mire_core::FileContent::Text { hunks } => hunks.as_slice(),
            mire_core::FileContent::Binary => unreachable!(),
        }
    );

    let json = serde_json::to_string(&kept).unwrap();
    assert!(json.contains("\"copy\":\"skipped\""));
    assert!(json.contains("\"copy\":{\"hunks\":["));
    let decoded: Review = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, kept);
}

#[test]
fn history_stays_within_its_budget_by_folding_the_oldest_checkpoint() {
    // Each copy serializes to about 920 KiB, so seventeen fit in 16 MiB.
    let line = |index: usize| format!("{index:04}{}", "x".repeat(235_000));
    let mut review = review(&[("a.txt", &line(0)), ("b.txt", "steady")]).keep_checkpoint_copies(|_| true);
    let mut skipped_at_budget = None;
    let mut kept_after_skip = false;
    for index in 1..22 {
        review = review
            .reanchor(changeset(&[("a.txt", &line(index)), ("b.txt", "steady")]))
            .unwrap()
            .keep_checkpoint_copies(|_| true);
        assert!(
            review.history_bytes() <= MAX_HISTORY_BYTES,
            "history over budget at refresh {index}"
        );
        let newest = review.checkpoints().last().unwrap();
        assert_eq!(
            newest.revision(),
            review.revision(),
            "every refresh records its checkpoint"
        );
        match (newest.files()[0].copy(), skipped_at_budget) {
            (PatchCopy::Skipped, None) => skipped_at_budget = Some(index),
            (PatchCopy::Kept(_), Some(_)) => kept_after_skip = true,
            _ => {}
        }
    }
    assert!(skipped_at_budget.is_some(), "the budget skipped a copy");
    assert!(kept_after_skip, "compaction freed space for a later copy");

    let oldest = review.checkpoints()[0].revision();
    assert!(oldest > revision(1), "the oldest checkpoints were folded");
    assert!(review.file_at_revision(&path("b.txt"), revision(1)).is_none());
    let (at, row) = review.file_at_revision(&path("b.txt"), oldest).unwrap();
    assert_eq!(at, oldest);
    assert!(row.is_carried());
    assert_eq!(row.fingerprint(), Some(review.changeset().files()[1].fingerprint()));
    assert!(
        review.file_history(&path("b.txt")).is_empty(),
        "carried rows are not history"
    );
    let (_, row) = review.file_at_revision(&path("a.txt"), review.revision()).unwrap();
    assert_eq!(row.fingerprint(), Some(review.changeset().files()[0].fingerprint()));
}

#[test]
fn compaction_keeps_pinned_rows_and_the_lookup() {
    let first = review(&[("a.txt", "new"), ("b.txt", "two"), ("c.txt", "three")]).keep_checkpoint_copies(|_| true);
    let noted = first.apply_notes(vec![note_on("a.txt")]).unwrap();
    let reference = noted.notes()[0].anchor().checkpoint().cloned().unwrap();
    assert_eq!((reference.revision(), reference.path()), (revision(1), &path("a.txt")));

    let third = noted
        .reanchor(changeset(&[("a.txt", "newer"), ("b.txt", "two"), ("c.txt", "three")]))
        .unwrap();
    let fourth = third
        .reanchor(changeset(&[
            ("a.txt", "newer"),
            ("b.txt", "changed"),
            ("c.txt", "three"),
        ]))
        .unwrap();
    assert_eq!(
        fourth.notes()[0].anchor().checkpoint(),
        Some(&reference),
        "re-anchoring keeps the checkpoint"
    );
    let expected = (1..=4)
        .map(|revision| {
            ["a.txt", "b.txt", "c.txt"].map(|name| {
                fourth
                    .file_at_revision(&path(name), self::revision(revision))
                    .map(|(_, row)| row.fingerprint())
            })
        })
        .collect::<Vec<_>>();

    // Fold only the oldest checkpoint: revision 3 takes its unchanged rows.
    let once = fourth.clone().compact_history(fourth.history_bytes() - 1);
    assert_eq!(once.checkpoints()[0].revision(), revision(3));
    for revision in 3..=4 {
        for (index, name) in ["a.txt", "b.txt", "c.txt"].iter().enumerate() {
            let found = once
                .file_at_revision(&path(name), self::revision(revision))
                .map(|(_, row)| row.fingerprint());
            assert_eq!(found, expected[revision as usize - 1][index], "{name} at {revision}");
        }
    }
    let pinned = once.checkpoint_row(&reference).unwrap();
    assert!(pinned.copy().hunks().is_some(), "the pinned row keeps its copy");
    assert_eq!(once.pinned_checkpoints().len(), 1);
    assert_eq!(once.pinned_checkpoints()[0].files().len(), 1);
    let carried = once.checkpoints()[0].row(&path("c.txt")).unwrap();
    assert!(carried.is_carried());
    assert_eq!(carried.copy(), &PatchCopy::Skipped);

    // Compacting to nothing keeps one checkpoint and the pinned rows.
    let all = fourth.compact_history(0);
    assert_eq!(all.checkpoints().len(), 1);
    assert!(
        all.checkpoints()[0]
            .files()
            .iter()
            .all(|row| row.copy() == &PatchCopy::Skipped)
    );
    assert!(all.checkpoint_row(&reference).unwrap().copy().hunks().is_some());
    let decoded: Review = serde_json::from_str(&serde_json::to_string(&all).unwrap()).unwrap();
    assert_eq!(decoded, all);
}

#[test]
fn a_review_file_without_checkpoints_loads_with_a_baseline() {
    let first = review(&[("a.txt", "new")]);
    let second = first.reanchor(changeset(&[("a.txt", "newer")])).unwrap();
    let mut json = serde_json::to_value(&second).unwrap();
    json.as_object_mut().unwrap().remove("checkpoints");
    let loaded: Review = serde_json::from_value(json).unwrap();

    assert_eq!(loaded.checkpoints().len(), 1);
    let baseline = &loaded.checkpoints()[0];
    assert!(baseline.is_baseline());
    assert_eq!(baseline.revision(), revision(2));
    assert_eq!(baseline.files().len(), 1);
    let a = path("a.txt");
    let current = loaded.changeset().files()[0].fingerprint();
    let earlier = first.changeset().files()[0].fingerprint();
    assert!(loaded.mark_viewed("ana", a.clone(), current, revision(2)).is_ok());
    assert_eq!(
        loaded.mark_viewed("ana", a, earlier, revision(1)),
        Err(ReviewError::ViewedMismatch),
        "history before the baseline is unknown"
    );
    let saved = serde_json::to_value(&loaded).unwrap();
    assert_eq!(saved["checkpoints"][0]["baseline"], Value::Bool(true));
}

#[test]
fn a_refresh_by_an_older_release_loads_as_a_gap() {
    let first = review(&[("a.txt", "new"), ("b.txt", "two"), ("c.txt", "three")]);
    let second = first
        .reanchor(changeset(&[("a.txt", "newer"), ("c.txt", "three")]))
        .unwrap();
    // An older release refreshed the file and kept checkpoints unchanged.
    let mut json = serde_json::to_value(&second).unwrap();
    json["checkpoints"].as_array_mut().unwrap().truncate(1);
    let loaded: Review = serde_json::from_value(json).unwrap();

    assert_eq!(loaded.checkpoints().len(), 2);
    let gap = &loaded.checkpoints()[1];
    assert!(gap.is_gap());
    assert_eq!(gap.revision(), revision(2));
    let rows = gap
        .files()
        .iter()
        .map(|row| (row.path().clone(), row.fingerprint()))
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (path("a.txt"), Some(loaded.changeset().files()[0].fingerprint())),
            (path("b.txt"), None),
        ]
    );

    let unchanged: Review = serde_json::from_str(&serde_json::to_string(&second).unwrap()).unwrap();
    assert!(unchanged.checkpoints().iter().all(|checkpoint| !checkpoint.is_gap()));
}

#[test]
fn checkpoints_after_the_review_revision_are_rejected() {
    let review = review(&[("a.txt", "new")]);
    let mut json = serde_json::to_value(&review).unwrap();
    json["checkpoints"][0]["revision"] = Value::from(2);
    let error = serde_json::from_value::<Review>(json).unwrap_err();
    assert!(error.to_string().contains("invalid checkpoints"), "{error}");
}

#[test]
fn notes_record_the_checkpoint_they_were_written_on() {
    let first = review(&[("a.txt", "new"), ("b.txt", "two")]);
    let second = first
        .reanchor(changeset(&[("a.txt", "new"), ("b.txt", "changed")]))
        .unwrap();
    let noted = second.apply_notes(vec![note_on("a.txt"), note_on("b.txt")]).unwrap();
    let references = noted
        .notes()
        .iter()
        .map(|note| {
            let reference = note.anchor().checkpoint().unwrap();
            (reference.revision().get(), reference.path().clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(references, [(1, path("a.txt")), (2, path("b.txt"))]);

    let mut json = serde_json::to_value(&noted).unwrap();
    for note in json["notes"].as_array_mut().unwrap() {
        note["anchor"].as_object_mut().unwrap().remove("checkpoint");
    }
    let older: Review = serde_json::from_value(json).unwrap();
    assert!(older.notes().iter().all(|note| note.anchor().checkpoint().is_none()));
}

fn review(files: &[(&str, &str)]) -> Review {
    Review::new(revision(1), changeset(files), Vec::new(), Vec::new()).unwrap()
}

fn changeset(files: &[(&str, &str)]) -> Changeset {
    let mut patch = String::new();
    for (name, content) in files {
        patch.push_str(&format!("--- a/{name}\n+++ b/{name}\n@@ -1 +1 @@\n-old\n+{content}\n"));
    }
    parse_patch(
        patch.as_bytes(),
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .unwrap()
}

fn note_on(name: &str) -> NoteInput {
    let line = LineNumber::new(1).unwrap();
    NoteInput::new(
        path(name),
        AnchorSide::New,
        LineRange::new(line, line).unwrap(),
        Author::new("agent", None).unwrap(),
        Provenance::Agent { producer: "test".to_owned() },
        NoteSeverity::Low,
        AnnotationKind::Comment,
        "Check this".to_owned(),
    )
    .unwrap()
}

fn path(name: &str) -> BytePath {
    BytePath::new(name.as_bytes().to_vec()).unwrap()
}

fn revision(value: u64) -> ReviewRevision {
    ReviewRevision::new(value).unwrap()
}
