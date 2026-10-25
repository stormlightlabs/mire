//! Folds the per-commit diffs of a patch series into one net diff.
//!
//! No commit diff carries the whole file, so each file is tracked as a list of
//! cells: a known line, or a run of base lines no diff has shown yet. Each
//! commit's hunks apply to that list, splitting runs and filling in lines as
//! their context and deletions reveal them. Runs never change, so the net diff
//! compares base and result between them and treats each run as unchanged.
//! Memory and time follow the size of the input, not its line numbers.
//!
//! Statuses can differ from `git diff` of the same range. A file renamed and
//! then re-added under its old path folds to a rename plus an addition. A
//! binary file changed and then reverted folds to a modification, because the
//! parser keeps no blob ids to show it is back to its base.

use std::collections::{BTreeMap, HashMap, VecDeque};

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::{
    ByteString, ChangesetSource, DiffLine, FileContent, FileDiff, FileSide, FileStatus, Fingerprint, Hunk, LineKind,
    LineNumber, MissingNewline,
};

use super::mbox::CommitDiff;
use super::{PatchError, Result, parser, writer};

const CONTEXT: usize = 3;

/// A line of the file after some commits: known, or a run of `len` base lines,
/// starting at base line index `id`, that no diff has shown.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Cell {
    Known(Line),
    Unknown { id: usize, len: usize },
}

/// One base or result line, or run of unseen base lines, as the net diff
/// compares them.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Item<'a> {
    Known(&'a Line),
    Unknown { id: usize, len: usize },
}

/// One row of the net diff: an unchanged, deleted, or added line, or an
/// unchanged run of unseen lines. `old` and `new` count the lines of each side
/// before the row.
#[derive(Clone, Copy)]
enum Row<'a> {
    Equal { old: usize, new: usize, item: Item<'a> },
    Delete { old: usize, new: usize, line: &'a Line },
    Insert { old: usize, new: usize, line: &'a Line },
}

/// One source line and whether a line terminator follows it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Line {
    content: Vec<u8>,
    newline: bool,
}

/// The cells a commit's hunks have not reached yet.
struct Pending {
    cells: VecDeque<Cell>,
    /// The number of lines the cells cover.
    lines: usize,
}

/// One file followed through the series, across renames and copies.
#[derive(Clone)]
struct Chain {
    /// The file before the series, or `None` when the series adds it.
    origin: Option<FileSide>,
    /// The file after the commits so far, or `None` once deleted.
    current: Option<FileSide>,
    /// The path the file had when it was last present.
    last_path: Vec<u8>,
    copied: bool,
    binary: bool,
    /// Base lines the diffs have revealed, by base line index. Every base line
    /// outside an unknown cell is here.
    base: BTreeMap<usize, Line>,
    /// The number of base lines the diffs have shown to exist.
    base_len: usize,
    cells: Vec<Cell>,
    /// Whether the file may hold unseen lines after the last cell.
    open_end: bool,
    /// The only commit diff for this file, kept while there is exactly one.
    single: Option<FileDiff>,
    touches: usize,
}

impl Cell {
    /// The number of lines the cell covers.
    const fn len(&self) -> usize {
        match self {
            Self::Known(_) => 1,
            Self::Unknown { len, .. } => *len,
        }
    }
}

impl Item<'_> {
    /// The number of lines the item covers.
    const fn len(&self) -> usize {
        match self {
            Self::Known(_) => 1,
            Self::Unknown { len, .. } => *len,
        }
    }
}

impl Row<'_> {
    /// The number of old lines before the row.
    const fn old_before(&self) -> usize {
        match self {
            Self::Equal { old, .. } | Self::Delete { old, .. } | Self::Insert { old, .. } => *old,
        }
    }

    /// The number of new lines before the row.
    const fn new_before(&self) -> usize {
        match self {
            Self::Equal { new, .. } | Self::Delete { new, .. } | Self::Insert { new, .. } => *new,
        }
    }
}

impl Pending {
    /// Moves the next `count` lines to `result`, splitting a run if needed.
    fn move_lines(&mut self, mut count: usize, result: &mut Vec<Cell>) -> std::result::Result<(), ()> {
        while count > 0 {
            let cell = self.cells.pop_front().ok_or(())?;
            let len = cell.len();
            if len <= count {
                result.push(cell);
                count -= len;
                self.lines -= len;
                continue;
            }
            let Cell::Unknown { id, len } = cell else {
                unreachable!("a known cell covers one line");
            };
            result.push(Cell::Unknown { id, len: count });
            self.cells
                .push_front(Cell::Unknown { id: id + count, len: len - count });
            self.lines -= count;
            count = 0;
        }
        Ok(())
    }

    /// Takes the next line, as a known cell or a one-line run.
    fn take_line(&mut self) -> Option<Cell> {
        let cell = self.cells.pop_front()?;
        self.lines -= 1;
        match cell {
            Cell::Unknown { id, len } if len > 1 => {
                self.cells.push_front(Cell::Unknown { id: id + 1, len: len - 1 });
                Some(Cell::Unknown { id, len: 1 })
            }
            other => Some(other),
        }
    }
}

impl Chain {
    fn from_origin(side: &FileSide) -> Self {
        Self {
            origin: Some(side.clone()),
            current: Some(side.clone()),
            last_path: side.path.as_bytes().to_vec(),
            copied: false,
            binary: false,
            base: BTreeMap::new(),
            base_len: 0,
            cells: Vec::new(),
            open_end: true,
            single: None,
            touches: 0,
        }
    }

    fn added() -> Self {
        Self {
            origin: None,
            current: None,
            last_path: Vec::new(),
            copied: false,
            binary: false,
            base: BTreeMap::new(),
            base_len: 0,
            cells: Vec::new(),
            open_end: false,
            single: None,
            touches: 0,
        }
    }

    fn apply(&mut self, file: &FileDiff) -> std::result::Result<(), ()> {
        self.touches += 1;
        self.single = (self.touches == 1).then(|| file.clone());
        self.current = file.new_side().cloned();
        if let Some(side) = file.new_side() {
            self.last_path = side.path.as_bytes().to_vec();
        }
        match file.content() {
            FileContent::Binary => self.binary = true,
            FileContent::Text { hunks } if !self.binary => self.apply_hunks(hunks)?,
            FileContent::Text { .. } => {}
        }
        if file.status() == FileStatus::Deleted && !self.binary {
            if !self.cells.is_empty() {
                return Err(());
            }
            self.open_end = false;
        }
        Ok(())
    }

    fn apply_hunks(&mut self, hunks: &[Hunk]) -> std::result::Result<(), ()> {
        let lines = self.cells.iter().map(Cell::len).sum();
        let mut pending = Pending { cells: std::mem::take(&mut self.cells).into(), lines };
        let mut result = Vec::with_capacity(pending.cells.len());
        let mut cursor = 0_usize;
        for hunk in hunks {
            let count = usize::try_from(hunk.old_line_count()).map_err(|_| ())?;
            let start = usize::try_from(hunk.old_start()).map_err(|_| ())?;
            let start = if count == 0 { start } else { start.checked_sub(1).ok_or(())? };
            let needed = start.checked_add(count).ok_or(())?;
            if start < cursor {
                return Err(());
            }
            let known_end = cursor + pending.lines;
            if needed > known_end {
                self.extend(&mut pending, needed - known_end)?;
            }
            pending.move_lines(start - cursor, &mut result)?;
            cursor = start;
            for line in hunk.lines() {
                match line.kind() {
                    LineKind::Context | LineKind::Deletion => {
                        let old = old_side(line);
                        let cell = pending.take_line().ok_or(())?;
                        if !old.newline && pending.lines > 0 {
                            return Err(());
                        }
                        self.reveal(&cell, &old)?;
                        if !old.newline {
                            self.open_end = false;
                        }
                        if line.kind() == LineKind::Context {
                            result.push(Cell::Known(new_side(line)));
                        }
                        cursor += 1;
                    }
                    LineKind::Addition => result.push(Cell::Known(new_side(line))),
                }
            }
        }
        result.extend(pending.cells);

        if let Some(position) = result
            .iter()
            .position(|cell| matches!(cell, Cell::Known(line) if !line.newline))
        {
            if position + 1 != result.len() {
                return Err(());
            }
            self.open_end = false;
        }
        self.cells = result;
        Ok(())
    }

    /// Adds a run of `count` unseen base lines after the last pending cell.
    fn extend(&mut self, pending: &mut Pending, count: usize) -> std::result::Result<(), ()> {
        if !self.open_end {
            return Err(());
        }
        pending.cells.push_back(Cell::Unknown { id: self.base_len, len: count });
        pending.lines = pending.lines.checked_add(count).ok_or(())?;
        self.base_len = self.base_len.checked_add(count).ok_or(())?;
        Ok(())
    }

    /// Checks a line a diff shows against its cell, and records a base line the
    /// diff reveals.
    fn reveal(&mut self, cell: &Cell, line: &Line) -> std::result::Result<(), ()> {
        match cell {
            Cell::Known(known) if known == line => Ok(()),
            Cell::Known(_) => Err(()),
            Cell::Unknown { id, .. } => {
                self.base.insert(*id, line.clone());
                Ok(())
            }
        }
    }

    /// Returns the net file diff, or `None` when the series leaves no change.
    fn net(&self) -> Option<FileDiff> {
        if self.touches == 1 {
            return self.single.clone();
        }
        let status = match (&self.origin, &self.current) {
            (None, None) => return None,
            (None, Some(_)) => FileStatus::Added,
            (Some(_), None) => FileStatus::Deleted,
            (Some(origin), Some(current)) if origin.path != current.path => {
                if self.copied {
                    FileStatus::Copied
                } else {
                    FileStatus::Renamed
                }
            }
            (Some(_), Some(_)) => FileStatus::Modified,
        };
        let content = if self.binary { FileContent::Binary } else { FileContent::Text { hunks: self.hunks() } };
        let unchanged = status == FileStatus::Modified
            && self.origin.as_ref().map(|side| side.mode) == self.current.as_ref().map(|side| side.mode)
            && matches!(&content, FileContent::Text { hunks } if hunks.is_empty());
        if unchanged {
            return None;
        }
        FileDiff::new(
            status,
            self.origin.clone(),
            self.current.clone(),
            None,
            content,
            Fingerprint::new([0; 32]),
        )
        .ok()
    }

    /// Diffs base and result. Both sides hold the same unknown runs in the
    /// same order, with the revealed base lines between them on the old side.
    fn hunks(&self) -> Vec<Hunk> {
        let mut old = Vec::new();
        let mut new = Vec::with_capacity(self.cells.len());
        let mut next = 0;
        for cell in &self.cells {
            match cell {
                Cell::Known(line) => new.push(Item::Known(line)),
                Cell::Unknown { id, len } => {
                    old.extend(self.base.range(next..*id).map(|(_, line)| Item::Known(line)));
                    let item = Item::Unknown { id: *id, len: *len };
                    old.push(item);
                    new.push(item);
                    next = id + len;
                }
            }
        }
        old.extend(self.base.range(next..).map(|(_, line)| Item::Known(line)));
        build_hunks(&rows(&old, &new))
    }
}

/// Folds the commit diffs of a series into one Git-style text diff.
pub fn fold(diffs: &[CommitDiff<'_>]) -> Result<String> {
    let mut chains: Vec<Chain> = Vec::new();
    for diff in diffs {
        let changeset = parse_at(*diff)?;
        let live = chains
            .iter()
            .enumerate()
            .filter_map(|(index, chain)| {
                chain
                    .current
                    .as_ref()
                    .map(|side| (side.path.as_bytes().to_vec(), index))
            })
            .collect::<HashMap<_, _>>();
        let deleted = chains
            .iter()
            .enumerate()
            .filter(|(_, chain)| chain.current.is_none())
            .map(|(index, chain)| (chain.last_path.clone(), index))
            .collect::<HashMap<_, _>>();

        // A copy starts from its source as the commit found it, before the
        // commit's own change to the source applies.
        let sources = changeset
            .files()
            .iter()
            .filter(|file| file.status() == FileStatus::Copied)
            .filter_map(|file| {
                let path = file.old_side()?.path.as_bytes().to_vec();
                let source = chains[*live.get(&path)?].clone();
                Some((path, source))
            })
            .collect::<HashMap<_, _>>();

        for file in changeset.files() {
            let old_path = file.old_side().map(|side| side.path.as_bytes().to_vec());
            let new_path = file.new_side().map(|side| side.path.as_bytes().to_vec());
            let index = match file.status() {
                FileStatus::Added => match new_path.and_then(|path| deleted.get(&path).copied()) {
                    Some(index) => index,
                    None => {
                        chains.push(Chain::added());
                        chains.len() - 1
                    }
                },
                FileStatus::Copied => {
                    let mut copy = match old_path.and_then(|path| sources.get(&path)) {
                        Some(source) => source.clone(),
                        None => Chain::from_origin(file.old_side().expect("a copy has an old side")),
                    };
                    copy.copied = true;
                    chains.push(copy);
                    chains.len() - 1
                }
                FileStatus::Modified | FileStatus::Renamed | FileStatus::Deleted => {
                    match old_path.and_then(|path| live.get(&path).copied()) {
                        Some(index) => index,
                        None => {
                            chains.push(Chain::from_origin(file.old_side().expect("the file has an old side")));
                            chains.len() - 1
                        }
                    }
                }
            };
            chains[index].apply(file).map_err(|()| PatchError::Malformed {
                offset: diff.offset,
                reason: "a commit in the series does not apply on top of the commits before it".to_owned(),
            })?;
        }
    }

    let files = chains.iter().filter_map(Chain::net).collect::<Vec<_>>();
    let bytes = writer::write_files(&files);
    String::from_utf8(bytes)
        .map_err(|error| PatchError::Encoding { encoding: "UTF-8", offset: error.utf8_error().valid_up_to() })
}

/// Parses one commit's diff, reporting errors at their offset in the series.
pub fn parse_at(diff: CommitDiff<'_>) -> Result<crate::Changeset> {
    parser::parse(diff.text, ChangesetSource::Patch { label: None }).map_err(|error| match error {
        PatchError::Malformed { offset, reason } => PatchError::Malformed { offset: offset + diff.offset, reason },
        PatchError::InvalidModel { offset, reason } => {
            PatchError::InvalidModel { offset: offset + diff.offset, reason }
        }
        other => other,
    })
}

fn old_side(line: &DiffLine) -> Line {
    Line {
        content: line.content().to_vec(),
        newline: !matches!(line.missing_newline(), MissingNewline::Old | MissingNewline::Both),
    }
}

fn new_side(line: &DiffLine) -> Line {
    Line {
        content: line.content().to_vec(),
        newline: !matches!(line.missing_newline(), MissingNewline::New | MissingNewline::Both),
    }
}

/// Diffs base and result between unknown runs, which always line up.
fn rows<'a>(old: &[Item<'a>], new: &[Item<'a>]) -> Vec<Row<'a>> {
    let anchors = |items: &[Item<'a>]| {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item, Item::Unknown { .. }))
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let old_anchors = anchors(old);
    let new_anchors = anchors(new);

    let mut rows = Vec::with_capacity(old.len().max(new.len()));
    let (mut old_start, mut new_start) = (0, 0);
    let (mut old_line, mut new_line) = (0, 0);
    for (old_anchor, new_anchor) in old_anchors
        .iter()
        .copied()
        .zip(new_anchors.iter().copied())
        .chain(std::iter::once((old.len(), new.len())))
    {
        segment_rows(
            &old[old_start..old_anchor],
            &new[new_start..new_anchor],
            old_line,
            new_line,
            &mut rows,
        );
        old_line += old_anchor - old_start;
        new_line += new_anchor - new_start;
        if let Some(&item) = old.get(old_anchor) {
            rows.push(Row::Equal { old: old_line, new: new_line, item });
            old_line += item.len();
            new_line += item.len();
        }
        old_start = old_anchor + 1;
        new_start = new_anchor + 1;
    }
    rows
}

/// Diffs one stretch of known lines that starts after `old_line` old lines and
/// `new_line` new lines.
fn segment_rows<'a>(old: &[Item<'a>], new: &[Item<'a>], old_line: usize, new_line: usize, rows: &mut Vec<Row<'a>>) {
    let known = |item: Item<'a>| match item {
        Item::Known(line) => line,
        Item::Unknown { .. } => unreachable!("segments hold no unknown runs"),
    };
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        let (_, old_span, new_span) = op.as_tag_tuple();
        match op {
            DiffOp::Equal { .. } => {
                for (old_index, new_index) in old_span.zip(new_span) {
                    rows.push(Row::Equal {
                        old: old_line + old_index,
                        new: new_line + new_index,
                        item: old[old_index],
                    });
                }
            }
            DiffOp::Delete { .. } | DiffOp::Insert { .. } | DiffOp::Replace { .. } => {
                let (old_end, new_start) = (old_span.end, new_span.start);
                for old_index in old_span {
                    rows.push(Row::Delete {
                        old: old_line + old_index,
                        new: new_line + new_start,
                        line: known(old[old_index]),
                    });
                }
                for new_index in new_span {
                    rows.push(Row::Insert {
                        old: old_line + old_end,
                        new: new_line + new_index,
                        line: known(new[new_index]),
                    });
                }
            }
        }
    }
}

/// Groups changed rows into hunks with up to three known lines of context.
/// Context stops at unknown runs, whose content is unknown.
fn build_hunks(rows: &[Row<'_>]) -> Vec<Hunk> {
    let is_change = |row: &Row<'_>| !matches!(row, Row::Equal { .. });
    let is_known_context = |row: &Row<'_>| matches!(row, Row::Equal { item: Item::Known(_), .. });

    let mut hunks = Vec::new();
    let mut previous_end = 0;
    let mut index = 0;
    while index < rows.len() {
        if !is_change(&rows[index]) {
            index += 1;
            continue;
        }
        let mut start = index;
        while start > previous_end && index - start < CONTEXT && is_known_context(&rows[start - 1]) {
            start -= 1;
        }
        let mut end = index;
        loop {
            while end < rows.len() && is_change(&rows[end]) {
                end += 1;
            }
            let mut next = end;
            while next < rows.len() && is_known_context(&rows[next]) {
                next += 1;
            }
            if next < rows.len() && is_change(&rows[next]) && next - end <= 2 * CONTEXT {
                end = next;
                continue;
            }
            end += (next - end).min(CONTEXT);
            break;
        }
        hunks.push(hunk(rows, start, end));
        previous_end = end;
        index = end;
    }
    hunks
}

/// Builds the hunk for `rows[start..end]`, which hold no unknown runs.
fn hunk(rows: &[Row<'_>], start: usize, end: usize) -> Hunk {
    let (old_before, new_before) = (rows[start].old_before() as u64, rows[start].new_before() as u64);
    let number = |index: usize| LineNumber::new(index as u64 + 1);
    let lines = rows[start..end]
        .iter()
        .map(|row| {
            let (kind, old_line, new_line, line, missing) = match *row {
                Row::Equal { old, new, item: Item::Known(line) } => {
                    let missing = if line.newline { MissingNewline::None } else { MissingNewline::Both };
                    (LineKind::Context, number(old), number(new), line, missing)
                }
                Row::Equal { item: Item::Unknown { .. }, .. } => unreachable!("hunks hold no unknown runs"),
                Row::Delete { old, line, .. } => {
                    let missing = if line.newline { MissingNewline::None } else { MissingNewline::Old };
                    (LineKind::Deletion, number(old), None, line, missing)
                }
                Row::Insert { new, line, .. } => {
                    let missing = if line.newline { MissingNewline::None } else { MissingNewline::New };
                    (LineKind::Addition, None, number(new), line, missing)
                }
            };
            DiffLine::new(kind, old_line, new_line, ByteString::new(line.content.clone()), missing)
                .expect("folded lines have consistent sides")
        })
        .collect::<Vec<_>>();
    let old_count = lines.iter().filter(|line| line.kind() != LineKind::Addition).count() as u64;
    let new_count = lines.iter().filter(|line| line.kind() != LineKind::Deletion).count() as u64;
    let first_old = lines.iter().find_map(DiffLine::old_line).map(LineNumber::get);
    let first_new = lines.iter().find_map(DiffLine::new_line).map(LineNumber::get);
    Hunk::new(
        first_old.unwrap_or(old_before),
        old_count,
        first_new.unwrap_or(new_before),
        new_count,
        ByteString::new(Vec::new()),
        lines,
        Fingerprint::new([0; 32]),
    )
    .expect("folded hunks have consistent counts")
}
