//! Folds the per-commit diffs of a patch series into one net diff.
//!
//! No commit diff carries the whole file, so each file is tracked as a list of
//! cells: a known line, or a placeholder for a base line no diff has shown yet.
//! Each commit's hunks apply to that list and fill in placeholders as their
//! context and deletions reveal lines. Placeholders never change, so the net
//! diff compares base and result between them and treats them as unchanged.

use std::collections::HashMap;

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::{
    ByteString, ChangesetSource, DiffLine, FileContent, FileDiff, FileSide, FileStatus, Fingerprint, Hunk, LineKind,
    LineNumber, MissingNewline,
};

use super::mbox::CommitDiff;
use super::{PatchError, Result, parser, writer};

const CONTEXT: usize = 3;

/// One source line and whether a line terminator follows it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Line {
    content: Vec<u8>,
    newline: bool,
}

/// A line of the file after some commits: known, or the base line with this
/// placeholder index that no diff has shown.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Cell {
    Known(Line),
    Unknown(usize),
}

/// One base or result line as the net diff compares it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Item<'a> {
    Known(&'a Line),
    Unknown(usize),
}

/// One row of the net diff: an unchanged, deleted, or added line.
#[derive(Clone, Copy)]
enum Row<'a> {
    Equal { old: usize, new: usize, item: Item<'a> },
    Delete { old: usize, line: &'a Line },
    Insert { new: usize, line: &'a Line },
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
    /// Base lines by placeholder index, filled in as diffs reveal them.
    base: Vec<Option<Line>>,
    cells: Vec<Cell>,
    /// Whether the file may hold unseen lines after the last cell.
    open_end: bool,
    /// The only commit diff for this file, kept while there is exactly one.
    single: Option<FileDiff>,
    touches: usize,
}

impl Chain {
    fn from_origin(side: &FileSide) -> Self {
        Self {
            origin: Some(side.clone()),
            current: Some(side.clone()),
            last_path: side.path.as_bytes().to_vec(),
            copied: false,
            binary: false,
            base: Vec::new(),
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
            base: Vec::new(),
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
        let mut result = Vec::with_capacity(self.cells.len());
        let mut cursor = 0;
        for hunk in hunks {
            let start = if hunk.old_line_count() == 0 {
                usize::try_from(hunk.old_start()).map_err(|_| ())?
            } else {
                usize::try_from(hunk.old_start() - 1).map_err(|_| ())?
            };
            let needed = start + usize::try_from(hunk.old_line_count()).map_err(|_| ())?;
            self.extend_to(needed)?;
            if start < cursor {
                return Err(());
            }
            result.extend(self.cells[cursor..start].iter().cloned());
            cursor = start;
            for line in hunk.lines() {
                match line.kind() {
                    LineKind::Context | LineKind::Deletion => {
                        let old = old_side(line);
                        let last = cursor + 1 == self.cells.len();
                        if !old.newline && !last {
                            return Err(());
                        }
                        self.reveal(cursor, old.clone())?;
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
        result.extend(self.cells[cursor..].iter().cloned());

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

    /// Adds placeholders at the end until the file has `length` lines.
    fn extend_to(&mut self, length: usize) -> std::result::Result<(), ()> {
        if self.cells.len() >= length {
            return Ok(());
        }
        if !self.open_end {
            return Err(());
        }
        while self.cells.len() < length {
            self.cells.push(Cell::Unknown(self.base.len()));
            self.base.push(None);
        }
        Ok(())
    }

    /// Checks a line a diff shows against the cell, and fills in a placeholder.
    fn reveal(&mut self, index: usize, line: Line) -> std::result::Result<(), ()> {
        match &self.cells[index] {
            Cell::Known(known) if *known == line => Ok(()),
            Cell::Known(_) => Err(()),
            Cell::Unknown(id) => {
                self.base[*id] = Some(line.clone());
                self.cells[index] = Cell::Known(line);
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

    fn hunks(&self) -> Vec<Hunk> {
        let old = self
            .base
            .iter()
            .enumerate()
            .map(|(id, line)| line.as_ref().map_or(Item::Unknown(id), Item::Known))
            .collect::<Vec<_>>();
        let new = self
            .cells
            .iter()
            .map(|cell| match cell {
                Cell::Known(line) => Item::Known(line),
                Cell::Unknown(id) => Item::Unknown(*id),
            })
            .collect::<Vec<_>>();
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
                    let mut copy = match old_path.and_then(|path| live.get(&path).copied()) {
                        Some(source) => chains[source].clone(),
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

/// Diffs base and result between placeholders, which always line up.
fn rows<'a>(old: &[Item<'a>], new: &[Item<'a>]) -> Vec<Row<'a>> {
    let anchors = |items: &[Item<'a>]| {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item, Item::Unknown(_)))
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let old_anchors = anchors(old);
    let new_anchors = anchors(new);

    let mut rows = Vec::with_capacity(old.len().max(new.len()));
    let (mut old_start, mut new_start) = (0, 0);
    for (old_anchor, new_anchor) in old_anchors
        .iter()
        .copied()
        .zip(new_anchors.iter().copied())
        .chain(std::iter::once((old.len(), new.len())))
    {
        segment_rows(old, new, old_start..old_anchor, new_start..new_anchor, &mut rows);
        if old_anchor < old.len() {
            rows.push(Row::Equal { old: old_anchor, new: new_anchor, item: old[old_anchor] });
        }
        old_start = old_anchor + 1;
        new_start = new_anchor + 1;
    }
    rows
}

fn segment_rows<'a>(
    old: &[Item<'a>], new: &[Item<'a>], old_range: std::ops::Range<usize>, new_range: std::ops::Range<usize>,
    rows: &mut Vec<Row<'a>>,
) {
    let (old_offset, new_offset) = (old_range.start, new_range.start);
    let known = |item: Item<'a>| match item {
        Item::Known(line) => line,
        Item::Unknown(_) => unreachable!("segments hold no placeholders"),
    };
    for op in capture_diff_slices(Algorithm::Myers, &old[old_range], &new[new_range]) {
        let (_, old_span, new_span) = op.as_tag_tuple();
        match op {
            DiffOp::Equal { .. } => {
                for (old_index, new_index) in old_span.zip(new_span) {
                    let (old_index, new_index) = (old_index + old_offset, new_index + new_offset);
                    rows.push(Row::Equal { old: old_index, new: new_index, item: old[old_index] });
                }
            }
            DiffOp::Delete { .. } | DiffOp::Insert { .. } | DiffOp::Replace { .. } => {
                for old_index in old_span {
                    let old_index = old_index + old_offset;
                    rows.push(Row::Delete { old: old_index, line: known(old[old_index]) });
                }
                for new_index in new_span {
                    let new_index = new_index + new_offset;
                    rows.push(Row::Insert { new: new_index, line: known(new[new_index]) });
                }
            }
        }
    }
}

/// Groups changed rows into hunks with up to three known lines of context.
/// Context stops at placeholders, whose content is unknown.
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

fn hunk(rows: &[Row<'_>], start: usize, end: usize) -> Hunk {
    let (old_before, new_before) = rows[..start].iter().fold((0, 0), |(old, new), row| match row {
        Row::Equal { .. } => (old + 1, new + 1),
        Row::Delete { .. } => (old + 1, new),
        Row::Insert { .. } => (old, new + 1),
    });
    let number = |index: usize| LineNumber::new(index as u64 + 1);
    let lines = rows[start..end]
        .iter()
        .map(|row| {
            let (kind, old_line, new_line, line, missing) = match *row {
                Row::Equal { old, new, item: Item::Known(line) } => {
                    let missing = if line.newline { MissingNewline::None } else { MissingNewline::Both };
                    (LineKind::Context, number(old), number(new), line, missing)
                }
                Row::Equal { item: Item::Unknown(_), .. } => unreachable!("hunks hold no placeholders"),
                Row::Delete { old, line } => {
                    let missing = if line.newline { MissingNewline::None } else { MissingNewline::Old };
                    (LineKind::Deletion, number(old), None, line, missing)
                }
                Row::Insert { new, line } => {
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
