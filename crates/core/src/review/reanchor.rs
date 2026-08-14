use crate::{BytePath, FileDiff};

use super::{
    Anchor, AnchorSide, Changeset, DiffLine, FileContent, LineNumber, LineRange, ReanchorCandidate, ReanchorEvidence,
    ReanchorOutcome, Result, Review, ReviewNote,
};

const NEARBY_CONTEXT_LINES: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
struct LineSnapshot {
    content: Vec<u8>,
    missing_newline: u8,
}

#[derive(Clone, Debug)]
struct AnchorSnapshot {
    selected: Vec<LineSnapshot>,
    before: Vec<LineSnapshot>,
    after: Vec<LineSnapshot>,
}

pub fn reanchor_review(review: &Review, changeset: Changeset) -> Result<Review> {
    if review.changeset.fingerprint() == changeset.fingerprint() {
        return Ok(review.clone());
    }

    let mut notes = review.notes.clone();
    for note in &mut notes {
        note.reanchor_outcome = Some(classify_note(note, &review.changeset, &changeset));
    }
    let mut refreshed = Review::new(review.next_revision()?, changeset, notes, review.events.clone())?;
    refreshed.source_binding = review.source_binding.clone();
    refreshed.extensions = review.extensions.clone();
    refreshed.validate()?;
    Ok(refreshed)
}

fn classify_note(note: &ReviewNote, previous: &Changeset, current: &Changeset) -> ReanchorOutcome {
    let original_anchor = note.anchor.clone();
    let source_anchors = source_anchors(note);

    for anchor in &source_anchors {
        if anchor.validate(current).is_ok() {
            return ReanchorOutcome::Exact {
                original_anchor,
                candidate: ReanchorCandidate {
                    anchor: anchor.clone(),
                    evidence: ReanchorEvidence {
                        path_match: true,
                        content_match: true,
                        context_before: 0,
                        context_after: 0,
                    },
                },
            };
        }
    }

    let mut candidates = Vec::new();
    for source_anchor in &source_anchors {
        let Some(snapshot) = snapshot_anchor(previous, source_anchor) else {
            continue;
        };
        candidates.extend(find_candidates(current, source_anchor, &snapshot));
    }
    deduplicate_candidates(&mut candidates);
    retain_best_candidates(&mut candidates);

    match candidates.len() {
        0 => ReanchorOutcome::Stale { original_anchor, evidence: stale_evidence(current, &source_anchors) },
        1 => {
            ReanchorOutcome::Moved { original_anchor, candidate: candidates.pop().expect("one candidate was counted") }
        }
        _ => ReanchorOutcome::Ambiguous { original_anchor, candidates },
    }
}

fn stale_evidence(changeset: &Changeset, source_anchors: &[Anchor]) -> ReanchorEvidence {
    let path_match = source_anchors.iter().any(|anchor| {
        changeset
            .files()
            .iter()
            .any(|file| side_path(file, anchor.side()) == Some(anchor.path()))
    });
    ReanchorEvidence { path_match, ..ReanchorEvidence::default() }
}

fn source_anchors(note: &ReviewNote) -> Vec<Anchor> {
    match &note.reanchor_outcome {
        None => vec![note.anchor.clone()],
        Some(ReanchorOutcome::Exact { candidate, .. } | ReanchorOutcome::Moved { candidate, .. }) => {
            vec![candidate.anchor.clone()]
        }
        Some(ReanchorOutcome::Ambiguous { candidates, .. }) => {
            candidates.iter().map(|candidate| candidate.anchor.clone()).collect()
        }
        Some(ReanchorOutcome::Stale { original_anchor, .. }) => vec![original_anchor.clone()],
    }
}

fn snapshot_anchor(changeset: &Changeset, anchor: &Anchor) -> Option<AnchorSnapshot> {
    let file = changeset
        .files()
        .iter()
        .find(|file| side_path(file, anchor.side()) == Some(anchor.path()))?;
    let FileContent::Text { hunks } = file.content() else {
        return None;
    };
    let hunk = hunks
        .iter()
        .find(|hunk| hunk.fingerprint() == anchor.hunk_fingerprint())?;
    let lines = side_lines(hunk.lines(), anchor.side());
    let start = lines.iter().position(|(number, _)| *number == anchor.range().start())?;
    let end = lines.iter().position(|(number, _)| *number == anchor.range().end())?;
    if end < start {
        return None;
    }
    Some(AnchorSnapshot {
        selected: snapshots(&lines[start..=end]),
        before: snapshots(&lines[start.saturating_sub(NEARBY_CONTEXT_LINES)..start]),
        after: snapshots(&lines[end + 1..(end + 1 + NEARBY_CONTEXT_LINES).min(lines.len())]),
    })
}

fn find_candidates(changeset: &Changeset, source: &Anchor, snapshot: &AnchorSnapshot) -> Vec<ReanchorCandidate> {
    let mut candidates = Vec::new();
    for file in changeset.files() {
        let Some(path) = side_path(file, source.side()) else {
            continue;
        };
        if path != source.path() && !file_has_path(file, source.path()) {
            continue;
        }
        let FileContent::Text { hunks } = file.content() else {
            continue;
        };
        for hunk in hunks {
            let lines = side_lines(hunk.lines(), source.side());
            if lines.len() < snapshot.selected.len() {
                continue;
            }
            for start in 0..=lines.len() - snapshot.selected.len() {
                let end = start + snapshot.selected.len();
                if snapshots(&lines[start..end]) != snapshot.selected {
                    continue;
                }
                let range = LineRange::new(lines[start].0, lines[end - 1].0)
                    .expect("candidate lines retain increasing source numbers");
                let Ok(anchor) = Anchor::new(changeset, path.clone(), source.side(), range, hunk.fingerprint()) else {
                    continue;
                };
                let before = snapshots(&lines[start.saturating_sub(NEARBY_CONTEXT_LINES)..start]);
                let after = snapshots(&lines[end..(end + NEARBY_CONTEXT_LINES).min(lines.len())]);
                candidates.push(ReanchorCandidate {
                    anchor,
                    evidence: ReanchorEvidence {
                        path_match: path == source.path(),
                        content_match: true,
                        context_before: matching_suffix(&snapshot.before, &before) as u32,
                        context_after: matching_prefix(&snapshot.after, &after) as u32,
                    },
                });
            }
        }
    }
    candidates
}

fn side_path(file: &FileDiff, side: AnchorSide) -> Option<&BytePath> {
    match side {
        AnchorSide::Old => file.old_side().map(|side| &side.path),
        AnchorSide::New => file.new_side().map(|side| &side.path),
    }
}

fn file_has_path(file: &FileDiff, path: &BytePath) -> bool {
    file.old_side().is_some_and(|side| side.path == *path) || file.new_side().is_some_and(|side| side.path == *path)
}

fn side_lines(lines: &[DiffLine], side: AnchorSide) -> Vec<(LineNumber, &DiffLine)> {
    lines
        .iter()
        .filter_map(|line| super::line_on_side(line, side).map(|number| (number, line)))
        .collect()
}

fn snapshots(lines: &[(LineNumber, &DiffLine)]) -> Vec<LineSnapshot> {
    lines
        .iter()
        .map(|(_, line)| LineSnapshot {
            content: line.content().to_vec(),
            missing_newline: super::missing_newline_tag(line),
        })
        .collect()
}

fn matching_prefix(left: &[LineSnapshot], right: &[LineSnapshot]) -> usize {
    left.iter().zip(right).take_while(|(left, right)| left == right).count()
}

fn matching_suffix(left: &[LineSnapshot], right: &[LineSnapshot]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

fn deduplicate_candidates(candidates: &mut Vec<ReanchorCandidate>) {
    candidates.sort_by(|left, right| {
        left.anchor
            .path()
            .cmp(right.anchor.path())
            .then_with(|| left.anchor.side().cmp(&right.anchor.side()))
            .then_with(|| left.anchor.range().start().cmp(&right.anchor.range().start()))
            .then_with(|| left.anchor.range().end().cmp(&right.anchor.range().end()))
            .then_with(|| left.anchor.hunk_fingerprint().cmp(&right.anchor.hunk_fingerprint()))
    });
    candidates.dedup_by(|left, right| left.anchor == right.anchor);
}

fn retain_best_candidates(candidates: &mut Vec<ReanchorCandidate>) {
    let Some(best) = candidates
        .iter()
        .map(|candidate| candidate_rank(candidate.evidence))
        .max()
    else {
        return;
    };
    candidates.retain(|candidate| candidate_rank(candidate.evidence) == best);
}

const fn candidate_rank(evidence: ReanchorEvidence) -> (u8, u32) {
    (
        evidence.path_match as u8,
        evidence.context_before.saturating_add(evidence.context_after),
    )
}
