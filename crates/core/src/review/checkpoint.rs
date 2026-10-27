//! Checkpoints: the revisions where each file's fingerprint changed, with the
//! blob ids and bounded patch copies Mire rebuilds old text from.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::{BytePath, Changeset, FileContent, FileDiff, FileSide, Fingerprint, Hunk, ObjectId};

use super::{Result, ReviewError, ReviewRevision};

/// Maximum serialized JSON bytes of one patch copy: 1 MiB.
pub const MAX_PATCH_COPY_BYTES: usize = 1024 * 1024;
/// Maximum serialized JSON bytes of a review's history, its checkpoints and
/// pinned rows together: 16 MiB.
pub const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;

const SKIPPED_COPY_BYTES: usize = "\"skipped\"".len();

/// A file's hunks kept with a checkpoint row, or a marker that none were kept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PatchCopy {
    /// The file's hunks at the checkpoint's revision.
    Kept(Vec<Hunk>),
    /// No copy was kept: Git held the blobs, the copy was over its bound, or
    /// history was at its budget.
    #[default]
    Skipped,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PatchCopyWire {
    Marker(String),
    Kept { hunks: Vec<Hunk> },
}

/// One revision where at least one file's fingerprint changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    revision: ReviewRevision,
    files: Vec<CheckpointRow>,
    #[serde(default, skip_serializing_if = "is_false")]
    baseline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    gap: bool,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

/// One file's state at a checkpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckpointRow {
    path: BytePath,
    fingerprint: Option<Fingerprint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    old_object: Option<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new_object: Option<ObjectId>,
    #[serde(default)]
    copy: PatchCopy,
    #[serde(default, skip_serializing_if = "is_false")]
    carried: bool,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

/// The checkpoint row a note's anchor was written on: the row's checkpoint
/// revision and its path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckpointRef {
    revision: ReviewRevision,
    path: BytePath,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Serialize)]
struct KeptCopy<'a> {
    hunks: &'a [Hunk],
}

#[derive(Default)]
struct ByteCounter(usize);

/// A copy waiting for room in the history budget.
struct Candidate {
    path: BytePath,
    copy: PatchCopy,
    added: usize,
}

/// Serialized sizes of each checkpoint and of the pinned list, kept up to
/// date across folds so history is not serialized again for every step.
struct History {
    sizes: Vec<usize>,
    pinned: usize,
}

impl Serialize for PatchCopy {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Kept(hunks) => KeptCopy { hunks }.serialize(serializer),
            Self::Skipped => serializer.serialize_str("skipped"),
        }
    }
}

impl<'de> Deserialize<'de> for PatchCopy {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match PatchCopyWire::deserialize(deserializer)? {
            PatchCopyWire::Marker(marker) if marker == "skipped" => Ok(Self::Skipped),
            PatchCopyWire::Marker(_) => Err(de::Error::custom("copy must be \"skipped\" or an object with hunks")),
            PatchCopyWire::Kept { hunks } => Ok(Self::Kept(hunks)),
        }
    }
}

impl PatchCopy {
    /// Returns the kept hunks, or `None` when no copy was kept.
    pub fn hunks(&self) -> Option<&[Hunk]> {
        match self {
            Self::Kept(hunks) => Some(hunks),
            Self::Skipped => None,
        }
    }
}

impl Checkpoint {
    /// Returns the review revision this checkpoint was recorded at.
    pub const fn revision(&self) -> ReviewRevision {
        self.revision
    }

    /// Returns the rows, sorted by path.
    pub fn files(&self) -> &[CheckpointRow] {
        &self.files
    }

    /// Reports whether Mire recorded this checkpoint for a review file
    /// written before checkpoints: history before it is unknown.
    pub const fn is_baseline(&self) -> bool {
        self.baseline
    }

    /// Reports whether the files changed somewhere between the previous
    /// checkpoint and this one, at revisions Mire did not record.
    pub const fn is_gap(&self) -> bool {
        self.gap
    }

    /// Returns the row for a path, if this checkpoint names it.
    pub fn row(&self, path: &BytePath) -> Option<&CheckpointRow> {
        self.files
            .binary_search_by(|row| row.path.cmp(path))
            .ok()
            .map(|index| &self.files[index])
    }
}

impl CheckpointRow {
    /// Returns the file's canonical path.
    pub const fn path(&self) -> &BytePath {
        &self.path
    }

    /// Returns the file's fingerprint at this checkpoint, or `None` when the
    /// file left the changeset here.
    pub const fn fingerprint(&self) -> Option<Fingerprint> {
        self.fingerprint
    }

    /// Returns the old side's Git blob id, if the patch carried a full one.
    pub const fn old_object(&self) -> Option<&ObjectId> {
        self.old_object.as_ref()
    }

    /// Returns the new side's Git blob id, if the patch carried a full one.
    pub const fn new_object(&self) -> Option<&ObjectId> {
        self.new_object.as_ref()
    }

    /// Returns the patch copy, kept or skipped.
    pub const fn copy(&self) -> &PatchCopy {
        &self.copy
    }

    /// Reports whether compaction moved this row forward: the file had this
    /// state at the checkpoint but did not change there.
    pub const fn is_carried(&self) -> bool {
        self.carried
    }
}

impl CheckpointRef {
    /// Points at the row for `path` in the checkpoint at `revision`.
    pub const fn new(revision: ReviewRevision, path: BytePath) -> Self {
        Self { revision, path, extensions: BTreeMap::new() }
    }

    /// Returns the revision of the checkpoint that holds the row.
    pub const fn revision(&self) -> ReviewRevision {
        self.revision
    }

    /// Returns the row's path.
    pub const fn path(&self) -> &BytePath {
        &self.path
    }
}

impl io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl History {
    fn measure(checkpoints: &[Checkpoint], pinned: &[Checkpoint]) -> Self {
        Self { sizes: checkpoints.iter().map(serialized_bytes).collect(), pinned: serialized_bytes(pinned) }
    }

    /// Serialized bytes of the checkpoints list and the pinned list.
    fn total(&self) -> usize {
        self.sizes.iter().sum::<usize>() + self.sizes.len().saturating_sub(1) + 2 + self.pinned
    }

    /// The least history can take after folding every older checkpoint: the
    /// pinned rows and the newest checkpoint as it is now.
    fn floor(&self) -> usize {
        self.pinned + self.sizes.last().copied().unwrap_or(0) + 2
    }

    /// Keeps each candidate copy that fits the budget on the newest
    /// checkpoint, in order, and leaves the rest. Reports whether any was
    /// kept.
    fn keep_fitting(&mut self, checkpoints: &mut [Checkpoint], candidates: &mut Vec<Candidate>) -> bool {
        let Some(current) = checkpoints.last_mut() else {
            return false;
        };
        let before = candidates.len();
        let mut used = self.total();
        let mut grown = 0;
        candidates.retain_mut(|candidate| {
            if used.saturating_add(candidate.added) > MAX_HISTORY_BYTES {
                return true;
            }
            let Ok(index) = current.files.binary_search_by(|row| row.path.cmp(&candidate.path)) else {
                return false;
            };
            current.files[index].copy = std::mem::take(&mut candidate.copy);
            used += candidate.added;
            grown += candidate.added;
            false
        });
        if let Some(size) = self.sizes.last_mut() {
            *size += grown;
        }
        candidates.len() < before
    }

    /// Folds the oldest checkpoint into the next one. Rows the next
    /// checkpoint does not name move forward with `carried` set and without
    /// their copies; rows a note pins are first copied, with their copies,
    /// into `pinned`. The next checkpoint takes the oldest one's `baseline`
    /// and `gap` flags and any extension field it does not set itself.
    fn fold_oldest(
        &mut self, checkpoints: &mut Vec<Checkpoint>, pinned: &mut Vec<Checkpoint>,
        pins: &BTreeSet<(ReviewRevision, BytePath)>,
    ) {
        if checkpoints.len() < 2 {
            return;
        }
        let oldest = checkpoints.remove(0);
        self.sizes.remove(0);
        let kept = oldest
            .files
            .iter()
            .filter(|row| pins.contains(&(oldest.revision, row.path.clone())))
            .cloned()
            .collect::<Vec<_>>();
        if !kept.is_empty() {
            pin_rows(pinned, oldest.revision, kept);
            self.pinned = serialized_bytes(pinned);
        }
        let next = &mut checkpoints[0];
        let named = next.files.iter().map(|row| row.path.clone()).collect::<BTreeSet<_>>();
        for mut row in oldest.files {
            if named.contains(&row.path) {
                continue;
            }
            row.copy = PatchCopy::Skipped;
            row.carried = true;
            next.files.push(row);
        }
        sort_rows(&mut next.files);
        next.baseline |= oldest.baseline;
        next.gap |= oldest.gap;
        for (key, value) in oldest.extensions {
            next.extensions.entry(key).or_insert(value);
        }
        self.sizes[0] = serialized_bytes(&*next);
    }
}

/// Builds a checkpoint naming every file in `changeset`, with no copies.
pub fn full_checkpoint(changeset: &Changeset, revision: ReviewRevision, baseline: bool) -> Checkpoint {
    let files = changeset.files().iter().map(row_for).collect();
    let mut checkpoint = Checkpoint { revision, files, baseline, gap: false, extensions: BTreeMap::new() };
    sort_rows(&mut checkpoint.files);
    checkpoint
}

/// Finds a file at a revision: the latest checkpoint at or before `revision`
/// that names `path`. Returns that checkpoint's revision and the row.
pub fn lookup<'a>(
    checkpoints: &'a [Checkpoint], path: &BytePath, revision: ReviewRevision,
) -> Option<(ReviewRevision, &'a CheckpointRow)> {
    let end = checkpoints.partition_point(|checkpoint| checkpoint.revision <= revision);
    checkpoints[..end]
        .iter()
        .rev()
        .find_map(|checkpoint| checkpoint.row(path).map(|row| (checkpoint.revision, row)))
}

/// Appends a checkpoint at `revision` holding a row for each file whose
/// fingerprint differs between `previous` and `current`. Records nothing when
/// no file changed.
pub fn record(checkpoints: &mut Vec<Checkpoint>, previous: &Changeset, current: &Changeset, revision: ReviewRevision) {
    let before = previous
        .files()
        .iter()
        .map(|file| (file.canonical_path(), file.fingerprint()))
        .collect::<BTreeMap<_, _>>();
    let now = current
        .files()
        .iter()
        .map(FileDiff::canonical_path)
        .collect::<BTreeSet<_>>();
    let mut files = current
        .files()
        .iter()
        .filter(|file| before.get(file.canonical_path()) != Some(&file.fingerprint()))
        .map(row_for)
        .collect::<Vec<_>>();
    files.extend(
        before
            .keys()
            .filter(|path| !now.contains(*path))
            .map(|path| removed_row((*path).clone())),
    );
    push_rows(checkpoints, revision, files, false);
}

/// Prepares history loaded from a review file.
///
/// A file without checkpoints gets a baseline at `revision` with every
/// current file. A file whose current fingerprints differ from the lookup at
/// its latest checkpoint, because a release without checkpoints refreshed it,
/// gets a gap checkpoint at `revision` holding the files that differ.
pub fn settle_loaded(checkpoints: &mut Vec<Checkpoint>, changeset: &Changeset, revision: ReviewRevision) {
    let Some(latest) = checkpoints.last().map(|checkpoint| checkpoint.revision) else {
        checkpoints.push(full_checkpoint(changeset, revision, true));
        return;
    };
    if latest > revision {
        // Validation rejects a checkpoint after the review's revision.
        return;
    }
    let current = changeset
        .files()
        .iter()
        .map(FileDiff::canonical_path)
        .collect::<BTreeSet<_>>();
    let mut files = changeset
        .files()
        .iter()
        .filter(|file| {
            lookup(checkpoints, file.canonical_path(), latest).and_then(|(_, row)| row.fingerprint)
                != Some(file.fingerprint())
        })
        .map(row_for)
        .collect::<Vec<_>>();
    let named = checkpoints
        .iter()
        .flat_map(|checkpoint| checkpoint.files.iter().map(|row| &row.path))
        .filter(|path| !current.contains(path))
        .collect::<BTreeSet<_>>();
    files.extend(
        named
            .into_iter()
            .filter(|path| lookup(checkpoints, path, latest).is_some_and(|(_, row)| row.fingerprint.is_some()))
            .map(|path| removed_row(path.clone())),
    );
    push_rows(checkpoints, revision, files, true);
}

/// Keeps patch copies on the rows of the checkpoint at `revision` that
/// `needs_copy` selects, within [`MAX_PATCH_COPY_BYTES`] per copy and
/// [`MAX_HISTORY_BYTES`] in all.
///
/// Copies are kept in path order while they fit. When one does not, the
/// oldest checkpoint is folded to make room and the remaining copies are
/// tried again, until every copy fits or no older checkpoint is left. Folds
/// that made room for no copy are undone, and the copies this call keeps are
/// never dropped. When the pinned rows and the current checkpoint alone leave
/// no room, nothing is folded and the copies stay `"skipped"`.
///
/// Pinned rows that `pins` no longer names are removed first.
pub fn keep_copies<F>(
    checkpoints: &mut Vec<Checkpoint>, pinned: &mut Vec<Checkpoint>, pins: &BTreeSet<(ReviewRevision, BytePath)>,
    changeset: &Changeset, revision: ReviewRevision, mut needs_copy: F,
) where
    F: FnMut(&FileDiff) -> bool,
{
    prune_pinned(pinned, pins);
    let Some(current) = checkpoints.last().filter(|checkpoint| checkpoint.revision == revision) else {
        return;
    };
    let mut candidates = Vec::new();
    for row in &current.files {
        if row.fingerprint.is_none() || row.carried || row.copy != PatchCopy::Skipped {
            continue;
        }
        let Some(file) = changeset.files().iter().find(|file| *file.canonical_path() == row.path) else {
            continue;
        };
        let FileContent::Text { hunks } = file.content() else {
            continue;
        };
        if !needs_copy(file) {
            continue;
        }
        let copy = PatchCopy::Kept(hunks.clone());
        let size = serialized_bytes(&copy);
        if size > MAX_PATCH_COPY_BYTES {
            continue;
        }
        candidates.push(Candidate { path: row.path.clone(), copy, added: size.saturating_sub(SKIPPED_COPY_BYTES) });
    }
    if candidates.is_empty() {
        return;
    }
    let mut history = History::measure(checkpoints, pinned);
    let mut saved = None;
    loop {
        if history.keep_fitting(checkpoints, &mut candidates) {
            saved = None;
        }
        let Some(smallest) = candidates.iter().map(|candidate| candidate.added).min() else {
            return;
        };
        // Folding cannot shrink history below the pinned rows and the current
        // checkpoint, so it cannot make room for a copy past that floor.
        if checkpoints.len() == 1 || history.floor().saturating_add(smallest) > MAX_HISTORY_BYTES {
            break;
        }
        if saved.is_none() {
            saved = Some((checkpoints.clone(), pinned.clone()));
        }
        history.fold_oldest(checkpoints, pinned, pins);
    }
    if let Some((kept_checkpoints, kept_pinned)) = saved {
        *checkpoints = kept_checkpoints;
        *pinned = kept_pinned;
    }
}

/// Returns the serialized JSON bytes of `checkpoints` and `pinned` together.
pub fn history_bytes(checkpoints: &[Checkpoint], pinned: &[Checkpoint]) -> usize {
    serialized_bytes(checkpoints).saturating_add(serialized_bytes(pinned))
}

/// Keeps a refresh's history within [`MAX_HISTORY_BYTES`] by folding the
/// oldest checkpoint into the next one until history fits or one checkpoint
/// remains. No copy of the remaining checkpoint is dropped.
///
/// Pinned rows that `pins` no longer names are removed first. When the
/// pinned rows alone reach the budget, nothing is folded: folding could not
/// bring history under it.
pub fn fit_budget(
    checkpoints: &mut Vec<Checkpoint>, pinned: &mut Vec<Checkpoint>, pins: &BTreeSet<(ReviewRevision, BytePath)>,
) {
    prune_pinned(pinned, pins);
    let mut history = History::measure(checkpoints, pinned);
    if history.pinned.saturating_add(2) >= MAX_HISTORY_BYTES {
        return;
    }
    while checkpoints.len() > 1 && history.total() > MAX_HISTORY_BYTES {
        history.fold_oldest(checkpoints, pinned, pins);
    }
}

/// Folds the oldest checkpoint into the next one until history fits in
/// `target` bytes or one checkpoint remains, and then drops that
/// checkpoint's copies if history still does not fit. A write uses this to
/// fit a review file under its size limit.
///
/// Rows the next checkpoint does not name move forward with `carried` set and
/// without their copies. Rows a note pins are first copied, with their
/// copies, into `pinned` under their own revision. Pinned rows that `pins`
/// no longer names are removed first.
pub fn compact(
    checkpoints: &mut Vec<Checkpoint>, pinned: &mut Vec<Checkpoint>, pins: &BTreeSet<(ReviewRevision, BytePath)>,
    target: usize,
) {
    prune_pinned(pinned, pins);
    let mut history = History::measure(checkpoints, pinned);
    while checkpoints.len() > 1 && history.total() > target {
        history.fold_oldest(checkpoints, pinned, pins);
    }
    if history.total() > target {
        for checkpoint in checkpoints.iter_mut() {
            for row in &mut checkpoint.files {
                if pins.contains(&(checkpoint.revision, row.path.clone())) && row.copy != PatchCopy::Skipped {
                    let mut kept = row.clone();
                    kept.carried = false;
                    pin_rows(pinned, checkpoint.revision, vec![kept]);
                }
                row.copy = PatchCopy::Skipped;
            }
        }
    }
}

/// Sorts rows by path and checkpoints by revision.
pub fn canonicalize(checkpoints: &mut [Checkpoint]) {
    checkpoints.sort_by_key(|checkpoint| checkpoint.revision);
    for checkpoint in checkpoints {
        sort_rows(&mut checkpoint.files);
    }
}

/// Checks that revisions strictly increase and do not pass `revision`, and
/// that no checkpoint names a path twice. Rows must already be sorted.
pub fn validate(checkpoints: &[Checkpoint], revision: ReviewRevision) -> Result<()> {
    if let Some(pair) = checkpoints.windows(2).find(|pair| pair[0].revision >= pair[1].revision) {
        return Err(ReviewError::InvalidCheckpoints {
            reason: format!(
                "revision {} is listed more than once or out of order",
                pair[1].revision.get()
            ),
        });
    }
    if let Some(checkpoint) = checkpoints.iter().find(|checkpoint| checkpoint.revision > revision) {
        return Err(ReviewError::InvalidCheckpoints {
            reason: format!(
                "revision {} is after the review revision {}",
                checkpoint.revision.get(),
                revision.get()
            ),
        });
    }
    for checkpoint in checkpoints {
        if checkpoint.files.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(ReviewError::InvalidCheckpoints {
                reason: format!("revision {} names a path more than once", checkpoint.revision.get()),
            });
        }
    }
    Ok(())
}

fn row_for(file: &FileDiff) -> CheckpointRow {
    CheckpointRow {
        path: file.canonical_path().clone(),
        fingerprint: Some(file.fingerprint()),
        old_object: file.old_side().and_then(side_object),
        new_object: file.new_side().and_then(side_object),
        copy: PatchCopy::Skipped,
        carried: false,
        extensions: BTreeMap::new(),
    }
}

fn removed_row(path: BytePath) -> CheckpointRow {
    CheckpointRow {
        path,
        fingerprint: None,
        old_object: None,
        new_object: None,
        copy: PatchCopy::Skipped,
        carried: false,
        extensions: BTreeMap::new(),
    }
}

fn side_object(side: &FileSide) -> Option<ObjectId> {
    side.object.clone()
}

fn push_rows(checkpoints: &mut Vec<Checkpoint>, revision: ReviewRevision, mut files: Vec<CheckpointRow>, gap: bool) {
    if files.is_empty() {
        return;
    }
    sort_rows(&mut files);
    if let Some(latest) = checkpoints
        .last_mut()
        .filter(|checkpoint| checkpoint.revision == revision)
    {
        for row in files {
            match latest.files.binary_search_by(|existing| existing.path.cmp(&row.path)) {
                Ok(index) => latest.files[index] = row,
                Err(index) => latest.files.insert(index, row),
            }
        }
        latest.gap |= gap;
        return;
    }
    checkpoints.push(Checkpoint { revision, files, baseline: false, gap, extensions: BTreeMap::new() });
}

fn pin_rows(pinned: &mut Vec<Checkpoint>, revision: ReviewRevision, rows: Vec<CheckpointRow>) {
    match pinned.binary_search_by_key(&revision, |checkpoint| checkpoint.revision) {
        Ok(index) => {
            let checkpoint = &mut pinned[index];
            for row in rows {
                if checkpoint.row(&row.path).is_none() {
                    checkpoint.files.push(row);
                }
            }
            sort_rows(&mut checkpoint.files);
        }
        Err(index) => {
            let mut files = rows;
            sort_rows(&mut files);
            pinned.insert(
                index,
                Checkpoint { revision, files, baseline: false, gap: false, extensions: BTreeMap::new() },
            );
        }
    }
}

/// Removes pinned rows that no note references any more, and the pinned
/// checkpoints left empty.
fn prune_pinned(pinned: &mut Vec<Checkpoint>, pins: &BTreeSet<(ReviewRevision, BytePath)>) {
    for checkpoint in pinned.iter_mut() {
        let revision = checkpoint.revision;
        checkpoint
            .files
            .retain(|row| pins.contains(&(revision, row.path.clone())));
    }
    pinned.retain(|checkpoint| !checkpoint.files.is_empty());
}

fn sort_rows(rows: &mut [CheckpointRow]) {
    rows.sort_by(|left, right| left.path.cmp(&right.path));
}

fn serialized_bytes<T: Serialize + ?Sized>(value: &T) -> usize {
    let mut counter = ByteCounter::default();
    serde_json::to_writer(&mut counter, value).map_or(usize::MAX, |()| counter.0)
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}
