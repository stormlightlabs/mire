use mire_core::{
    AnchorSide, Author, BytePath, Changeset, ChangesetSource, HunkLocation, LineNumber, LineRange,
    MAX_WALKTHROUGH_TEXT_BYTES, PatchLimits, Provenance, ReanchorOutcome, Review, ReviewError, ReviewRevision, StepId,
    StepInput, StepLocation, WalkthroughInput, parse_patch,
};

const TWO_FILES: &[u8] =
    b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n--- a/other.txt\n+++ b/other.txt\n@@ -1 +1 @@\n-a\n+b\n";

#[test]
fn walkthrough_round_trips_with_generated_ids_and_advances_the_revision() {
    let review = review(changeset(TWO_FILES));
    let input = WalkthroughInput::new(
        Some("Fix the parser.".to_owned()),
        vec![
            step(Some("contract"), "Contract", "file.txt", &[]),
            step(None, "Implementation", "file.txt", &["contract"]),
            step(Some("tests"), "Tests", "other.txt", &["contract"]),
        ],
        agent(),
        Provenance::Agent { producer: "test".to_owned() },
    );
    let written = review.write_walkthrough(review.revision(), input.clone()).unwrap();
    assert_eq!(written.revision().get(), 2);

    let walkthrough = written.walkthrough().unwrap();
    let ids = walkthrough
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["contract", "step-2", "tests"]);
    assert_eq!(walkthrough.overview(), Some("Fix the parser."));
    assert_eq!(walkthrough.steps()[1].depends_on()[0].as_str(), "contract");

    let json = serde_json::to_string(&written).unwrap();
    assert!(json.contains("\"minor\":5"));
    let decoded: Review = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, written);

    let again = written.write_walkthrough(written.revision(), input).unwrap();
    assert_eq!(again, written);
}

#[test]
fn walkthrough_fields_from_newer_writers_survive_a_round_trip() {
    let written = written_review(changeset(TWO_FILES));
    let json =
        serde_json::to_string(&written)
            .unwrap()
            .replacen("\"title\":\"Step\"", "\"title\":\"Step\",\"future\":1", 1);
    let decoded: Review = serde_json::from_str(&json).unwrap();
    assert!(serde_json::to_string(&decoded).unwrap().contains("\"future\":1"));
}

#[test]
fn review_files_without_a_walkthrough_still_load() {
    let json = serde_json::to_string(&review(changeset(TWO_FILES)))
        .unwrap()
        .replacen("\"minor\":5", "\"minor\":4", 1);
    assert!(json.contains("\"minor\":4"));
    assert!(!json.contains("walkthrough"));
    let decoded: Review = serde_json::from_str(&json).unwrap();
    assert!(decoded.walkthrough().is_none());
    assert!(decoded.walkthrough_coverage().is_none());
}

#[test]
fn walkthrough_writes_need_the_current_revision() {
    let review = review(changeset(TWO_FILES));
    let written = review
        .write_walkthrough(review.revision(), input(vec![step(None, "Step", "file.txt", &[])]))
        .unwrap();
    let error = written
        .write_walkthrough(review.revision(), input(vec![step(None, "Other", "file.txt", &[])]))
        .unwrap_err();
    assert_eq!(error, ReviewError::RevisionConflict { expected: 1, actual: 2 });
    assert_eq!(error.error_code(), "revision_conflict");
    assert_eq!(written.walkthrough().unwrap().steps()[0].title(), "Step");
}

#[test]
fn walkthrough_writes_reject_invalid_steps() {
    let review = review(changeset(TWO_FILES));
    let write = |steps| review.write_walkthrough(review.revision(), input(steps));

    assert_eq!(write(Vec::new()), Err(ReviewError::EmptyWalkthrough));
    assert_eq!(
        write(vec![
            step(None, "First", "file.txt", &[]),
            step(Some("second"), "Second", "file.txt", &["step-1"])
        ]),
        Err(step_error(1, ReviewError::ForwardStepDependency("step-1".to_owned())))
    );
    assert_eq!(
        write(vec![
            step(Some("first"), "First", "file.txt", &["second"]),
            step(Some("second"), "Second", "file.txt", &[])
        ]),
        Err(step_error(0, ReviewError::ForwardStepDependency("second".to_owned())))
    );
    assert_eq!(
        write(vec![step(Some("self"), "Self", "file.txt", &["self"])]),
        Err(step_error(0, ReviewError::ForwardStepDependency("self".to_owned())))
    );
    assert_eq!(
        write(vec![
            step(Some("same"), "First", "file.txt", &[]),
            step(Some("same"), "Second", "file.txt", &[])
        ]),
        Err(step_error(1, ReviewError::DuplicateStepId("same".to_owned())))
    );
    assert_eq!(
        write(vec![step(None, "Two\nlines", "file.txt", &[])]),
        Err(step_error(0, ReviewError::MultilineStepTitle))
    );
    assert_eq!(
        write(vec![step(None, "Missing", "absent.txt", &[])]),
        Err(step_error(0, ReviewError::LocationNotFound))
    );
    assert_eq!(
        write(vec![StepInput::new(
            None,
            "No anchors".to_owned(),
            "Why".to_owned(),
            Vec::new(),
            Vec::new(),
            None
        )]),
        Err(step_error(0, ReviewError::StepWithoutAnchors))
    );
    assert_eq!(
        write(vec![StepInput::new(
            None,
            "Blank".to_owned(),
            " \n".to_owned(),
            vec![location("file.txt")],
            Vec::new(),
            None
        )]),
        Err(step_error(0, ReviewError::BlankField { field: "step narrative" }))
    );

    let large = WalkthroughInput::new(
        Some("o".repeat(MAX_WALKTHROUGH_TEXT_BYTES + 1)),
        vec![step(None, "Step", "file.txt", &[])],
        agent(),
        Provenance::Agent { producer: "test".to_owned() },
    );
    assert!(matches!(
        review.write_walkthrough(review.revision(), large),
        Err(ReviewError::TextTooLarge { field: "walkthrough overview", .. })
    ));

    let human = WalkthroughInput::new(
        None,
        vec![step(None, "Step", "file.txt", &[])],
        agent(),
        Provenance::Human,
    );
    assert_eq!(
        review.write_walkthrough(review.revision(), human),
        Err(ReviewError::HumanProvenanceForbidden)
    );
}

#[test]
fn refresh_reanchors_step_anchors_like_notes() {
    let original = changeset(TWO_FILES);
    let written = written_review(original.clone());

    let exact = written.reanchor(changeset(
        b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\n--- a/third.txt\n+++ b/third.txt\n@@ -1 +1 @@\n-x\n+y\n",
    ));
    assert!(matches!(
        first_outcome(&exact.unwrap()),
        Some(ReanchorOutcome::Exact { .. })
    ));

    let moved = written
        .reanchor(changeset(
            b"--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,3 @@\n context\n+inserted\n-old\n+bug\n",
        ))
        .unwrap();
    let Some(ReanchorOutcome::Moved { candidate, .. }) = first_outcome(&moved) else {
        panic!("the step anchor should move to one content-supported location");
    };
    assert_eq!(candidate.anchor().range().start().get(), 3);
    assert!(!moved.walkthrough().unwrap().steps()[0].has_unsettled_anchor());

    let stale = written
        .reanchor(changeset(
            b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+fixed\n",
        ))
        .unwrap();
    assert!(matches!(first_outcome(&stale), Some(ReanchorOutcome::Stale { .. })));
    assert!(stale.walkthrough().unwrap().steps()[0].has_unsettled_anchor());
    let restored = stale.reanchor(original).unwrap();
    assert!(matches!(first_outcome(&restored), Some(ReanchorOutcome::Exact { .. })));

    let ambiguous = written
        .reanchor(changeset(
            b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1,2 @@\n-old\n+bug\n+bug\n",
        ))
        .unwrap();
    let Some(ReanchorOutcome::Ambiguous { candidates, .. }) = first_outcome(&ambiguous) else {
        panic!("duplicate content should leave the step anchor ambiguous");
    };
    assert_eq!(candidates.len(), 2);

    let json = serde_json::to_string(&ambiguous).unwrap();
    assert_eq!(serde_json::from_str::<Review>(&json).unwrap(), ambiguous);
}

#[test]
fn coverage_counts_only_exact_and_moved_anchors() {
    let written = written_review(changeset(
        b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+bug\ndiff --git a/other.txt b/other.txt\n--- a/other.txt\n+++ b/other.txt\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/image.png b/image.png\nBinary files a/image.png and b/image.png differ\n",
    ));
    let coverage = written.walkthrough_coverage().unwrap();
    // Files sort by path: file.txt, image.png, other.txt.
    assert_eq!(
        coverage.uncovered_hunks,
        [HunkLocation { file_index: 2, hunk_index: 0 }]
    );
    assert_eq!(coverage.uncovered_files, [1]);

    let stale = written
        .reanchor(changeset(
            b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+fixed\n",
        ))
        .unwrap();
    let coverage = stale.walkthrough_coverage().unwrap();
    assert_eq!(
        coverage.uncovered_hunks,
        [HunkLocation { file_index: 0, hunk_index: 0 }]
    );
    assert!(coverage.uncovered_files.is_empty());
}

#[test]
fn note_writes_keep_the_walkthrough() {
    let written = written_review(changeset(TWO_FILES));
    let line = LineNumber::new(1).unwrap();
    let note = mire_core::NoteInput::new(
        BytePath::new(b"other.txt".to_vec()).unwrap(),
        AnchorSide::New,
        LineRange::new(line, line).unwrap(),
        agent(),
        Provenance::Agent { producer: "test".to_owned() },
        mire_core::NoteSeverity::Low,
        mire_core::AnnotationKind::Comment,
        "Finding".to_owned(),
    )
    .unwrap();
    let updated = written.apply_notes(vec![note]).unwrap();
    assert_eq!(updated.walkthrough(), written.walkthrough());
}

fn first_outcome(review: &Review) -> Option<&ReanchorOutcome> {
    review.walkthrough().unwrap().steps()[0].anchors()[0].reanchor_outcome()
}

fn step_error(index: usize, reason: ReviewError) -> ReviewError {
    ReviewError::InvalidStep { index, reason: Box::new(reason) }
}

fn written_review(changeset: Changeset) -> Review {
    let review = review(changeset);
    review
        .write_walkthrough(review.revision(), input(vec![step(None, "Step", "file.txt", &[])]))
        .unwrap()
}

fn input(steps: Vec<StepInput>) -> WalkthroughInput {
    WalkthroughInput::new(None, steps, agent(), Provenance::Agent { producer: "test".to_owned() })
}

fn step(id: Option<&str>, title: &str, path: &str, depends_on: &[&str]) -> StepInput {
    StepInput::new(
        id.map(|id| StepId::new(id).unwrap()),
        title.to_owned(),
        format!("Why {title} changed."),
        vec![location(path)],
        depends_on.iter().map(|id| StepId::new(*id).unwrap()).collect(),
        None,
    )
}

fn location(path: &str) -> StepLocation {
    let line = LineNumber::new(1).unwrap();
    StepLocation::new(
        BytePath::new(path.as_bytes()).unwrap(),
        AnchorSide::New,
        LineRange::new(line, line).unwrap(),
    )
}

fn agent() -> Author {
    Author::new("agent", None).unwrap()
}

fn review(changeset: Changeset) -> Review {
    Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
}

fn changeset(patch: &[u8]) -> Changeset {
    parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap()
}
