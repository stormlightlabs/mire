use std::collections::{BTreeMap, BTreeSet};

use mire_core::{AnchorSide, Changeset, FileContent, FileDiff, Fingerprint, Hunk, LineKind, ReviewNote};
use ratatui::text::Span;

const LINE_GUTTER_WIDTH: usize = 13;

/// The layout used to turn one changeset into visible review rows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LayoutMode {
    /// Show old and new lines in one continuous column.
    Unified,
    /// Align old and new source lines in side-by-side columns.
    Split,
    /// Choose split layout when the review pane is wide enough.
    #[default]
    Automatic,
}

/// The concrete row layout selected for the current review pane width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedLayout {
    /// One continuous diff column.
    Unified,
    /// Aligned old and new columns.
    Split,
}

/// The semantic kind of a row in the continuous review stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    /// A file heading.
    File,
    /// A binary-file marker.
    Binary,
    /// A hunk heading.
    Hunk,
    /// One unified or aligned source row.
    Line,
    /// A missing-final-newline marker attached to the preceding line.
    MissingNewline,
    /// Context omitted by the current context setting.
    ContextGap,
    /// A review note placed immediately after its anchored range.
    Note,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKey {
    File {
        file: usize,
    },
    Binary {
        file: usize,
    },
    Hunk {
        file: usize,
        hunk: usize,
    },
    UnifiedLine {
        file: usize,
        hunk: usize,
        line: usize,
        range: TextRange,
    },
    SplitLine {
        file: usize,
        hunk: usize,
        old: Option<usize>,
        new: Option<usize>,
        old_range: Option<TextRange>,
        new_range: Option<TextRange>,
    },
    MissingNewline {
        file: usize,
        hunk: usize,
        line: usize,
    },
    ContextGap {
        file: usize,
        hunk: usize,
        hidden: usize,
    },
    Note {
        file: usize,
        note: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowAnchor {
    File { file: usize },
    Binary { file: usize },
    Hunk { file: usize, hunk: usize },
    Line { file: usize, hunk: usize, line: usize },
    MissingNewline { file: usize, hunk: usize, line: usize },
    Note { file: usize, note: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

/// A lightweight row index over an immutable changeset.
///
/// The index stores source coordinates rather than rendered text or widgets.
/// Rendering resolves only the slice intersecting the current viewport.
#[derive(Debug)]
pub struct ReviewStream<'a> {
    changeset: &'a Changeset,
    layout: ResolvedLayout,
    rows: Vec<RowKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamOptions {
    context_lines: usize,
    wrap_width: Option<u16>,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self { context_lines: 3, wrap_width: None }
    }
}

/// Structural rows omitted from the visible review stream.
#[derive(Clone, Copy, Debug)]
pub struct CollapseState<'a> {
    collapsed_files: &'a BTreeSet<usize>,
    collapsed_hunks: &'a BTreeSet<(usize, usize)>,
}

impl<'a> CollapseState<'a> {
    /// Creates collapse state from file and hunk stream coordinates.
    pub const fn new(collapsed_files: &'a BTreeSet<usize>, collapsed_hunks: &'a BTreeSet<(usize, usize)>) -> Self {
        Self { collapsed_files, collapsed_hunks }
    }
}

impl LayoutMode {
    /// Resolves automatic layout from the width available to diff content.
    pub const fn resolve(self, content_width: u16) -> ResolvedLayout {
        match self {
            Self::Unified => ResolvedLayout::Unified,
            Self::Split => ResolvedLayout::Split,
            Self::Automatic if content_width >= 72 => ResolvedLayout::Split,
            Self::Automatic => ResolvedLayout::Unified,
        }
    }
}

impl ResolvedLayout {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Split => "split",
        }
    }
}

impl ReviewStream<'_> {
    /// Returns the concrete layout used to generate the current rows.
    pub const fn layout(&self) -> ResolvedLayout {
        self.layout
    }

    /// Returns the number of logical rows across every file and hunk.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Reports whether the stream contains no logical rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns semantic row kinds for a bounded viewport.
    pub fn visible_kinds(&self, offset: usize, height: usize) -> impl Iterator<Item = RowKind> + '_ {
        self.visible_keys(offset, height).map(RowKey::kind)
    }

    pub fn visible_keys(&self, offset: usize, height: usize) -> impl Iterator<Item = RowKey> + '_ {
        let end = offset.saturating_add(height).min(self.rows.len());
        self.rows
            .get(offset.min(self.rows.len())..end)
            .unwrap_or_default()
            .iter()
            .copied()
    }

    pub fn row(&self, index: usize) -> Option<RowKey> {
        self.rows.get(index).copied()
    }

    pub fn anchor_at(&self, index: usize) -> Option<RowAnchor> {
        self.row(index).map(RowKey::anchor)
    }

    pub fn index_of_anchor(&self, anchor: RowAnchor) -> Option<usize> {
        self.rows.iter().position(|row| row.contains(anchor))
    }

    pub fn file_at(&self, index: usize) -> Option<usize> {
        self.row(index).map(RowKey::file)
    }

    pub fn file_position(&self, file: usize) -> Option<usize> {
        self.index_of_anchor(RowAnchor::File { file })
    }

    pub fn hunk_positions(&self) -> impl DoubleEndedIterator<Item = (usize, usize, usize)> + '_ {
        self.rows.iter().enumerate().filter_map(|(row, key)| match key {
            RowKey::Hunk { file, hunk } => Some((row, *file, *hunk)),
            _ => None,
        })
    }
}

impl<'a> ReviewStream<'a> {
    /// Indexes every logical row without constructing off-screen widgets.
    pub fn new(changeset: &'a Changeset, layout: ResolvedLayout) -> Self {
        Self::with_options(changeset, layout, StreamOptions::default())
    }

    /// Indexes rows with explicit context and wrapping presentation options.
    pub fn with_presentation(
        changeset: &'a Changeset, layout: ResolvedLayout, context_lines: usize, wrap_width: Option<u16>,
    ) -> Self {
        Self::with_presentation_collapsed(
            changeset,
            layout,
            context_lines,
            wrap_width,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
    }

    /// Indexes code with file and hunk disclosure state applied before rendering.
    pub fn with_presentation_collapsed(
        changeset: &'a Changeset, layout: ResolvedLayout, context_lines: usize, wrap_width: Option<u16>,
        collapsed_files: &BTreeSet<usize>, collapsed_hunks: &BTreeSet<(usize, usize)>,
    ) -> Self {
        Self::with_options_and_expanded_hunks(
            changeset,
            layout,
            StreamOptions { context_lines, wrap_width },
            &BTreeSet::new(),
            collapsed_files,
            collapsed_hunks,
        )
    }

    /// Indexes code and the selected review notes as one adjacent stream.
    pub fn with_notes_presentation(
        changeset: &'a Changeset, notes: &[ReviewNote], visible_notes: &[usize], layout: ResolvedLayout,
        context_lines: usize, wrap_width: Option<u16>,
    ) -> Self {
        Self::with_notes_presentation_collapsed(
            changeset,
            notes,
            visible_notes,
            layout,
            context_lines,
            wrap_width,
            CollapseState::new(&BTreeSet::new(), &BTreeSet::new()),
        )
    }

    /// Indexes review notes while retaining collapsed file and hunk boundaries.
    pub fn with_notes_presentation_collapsed(
        changeset: &'a Changeset, notes: &[ReviewNote], visible_notes: &[usize], layout: ResolvedLayout,
        context_lines: usize, wrap_width: Option<u16>, collapse: CollapseState<'_>,
    ) -> Self {
        let expanded_hunks = note_hunks(changeset, notes, visible_notes);
        let mut stream = Self::with_options_and_expanded_hunks(
            changeset,
            layout,
            StreamOptions { context_lines, wrap_width },
            &expanded_hunks,
            collapse.collapsed_files,
            collapse.collapsed_hunks,
        );
        let rendered_notes = visible_notes
            .iter()
            .copied()
            .filter(|note| {
                !note_is_collapsed(
                    changeset,
                    notes,
                    *note,
                    collapse.collapsed_files,
                    collapse.collapsed_hunks,
                )
            })
            .collect::<Vec<_>>();
        attach_notes(&mut stream.rows, changeset, notes, &rendered_notes);
        stream
    }

    fn with_options(changeset: &'a Changeset, layout: ResolvedLayout, options: StreamOptions) -> Self {
        Self::with_options_and_expanded_hunks(
            changeset,
            layout,
            options,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
    }

    fn with_options_and_expanded_hunks(
        changeset: &'a Changeset, layout: ResolvedLayout, options: StreamOptions,
        expanded_hunks: &BTreeSet<(usize, usize)>, collapsed_files: &BTreeSet<usize>,
        collapsed_hunks: &BTreeSet<(usize, usize)>,
    ) -> Self {
        let mut rows = Vec::new();
        for (file_index, file) in changeset.files().iter().enumerate() {
            rows.push(RowKey::File { file: file_index });
            if collapsed_files.contains(&file_index) {
                continue;
            }
            match file.content() {
                FileContent::Binary => rows.push(RowKey::Binary { file: file_index }),
                FileContent::Text { hunks } => index_hunks(
                    &mut rows,
                    file_index,
                    hunks,
                    layout,
                    options,
                    expanded_hunks,
                    collapsed_hunks,
                ),
            }
        }
        Self { changeset, layout, rows }
    }

    pub const fn changeset(&self) -> &'a Changeset {
        self.changeset
    }

    pub fn file(&self, index: usize) -> &'a FileDiff {
        &self.changeset.files()[index]
    }

    pub fn hunk(&self, file: usize, hunk: usize) -> &'a Hunk {
        let FileContent::Text { hunks } = self.file(file).content() else {
            unreachable!("hunk row coordinates always refer to text content");
        };
        &hunks[hunk]
    }
}

impl RowKey {
    pub const fn kind(self) -> RowKind {
        match self {
            Self::File { .. } => RowKind::File,
            Self::Binary { .. } => RowKind::Binary,
            Self::Hunk { .. } => RowKind::Hunk,
            Self::UnifiedLine { .. } | Self::SplitLine { .. } => RowKind::Line,
            Self::MissingNewline { .. } => RowKind::MissingNewline,
            Self::ContextGap { .. } => RowKind::ContextGap,
            Self::Note { .. } => RowKind::Note,
        }
    }

    pub const fn file(self) -> usize {
        match self {
            Self::File { file }
            | Self::Binary { file }
            | Self::Hunk { file, .. }
            | Self::UnifiedLine { file, .. }
            | Self::SplitLine { file, .. }
            | Self::MissingNewline { file, .. }
            | Self::ContextGap { file, .. } => file,
            Self::Note { file, .. } => file,
        }
    }

    pub fn anchor(self) -> RowAnchor {
        match self {
            Self::File { file } => RowAnchor::File { file },
            Self::Binary { file } => RowAnchor::Binary { file },
            Self::Hunk { file, hunk } => RowAnchor::Hunk { file, hunk },
            Self::UnifiedLine { file, hunk, line, .. } => RowAnchor::Line { file, hunk, line },
            Self::SplitLine { file, hunk, old: Some(line), .. }
            | Self::SplitLine { file, hunk, old: None, new: Some(line), .. } => RowAnchor::Line { file, hunk, line },
            Self::SplitLine { old: None, new: None, .. } => {
                unreachable!("split rows always contain an old or new source line")
            }
            Self::MissingNewline { file, hunk, line } => RowAnchor::MissingNewline { file, hunk, line },
            Self::ContextGap { file, hunk, .. } => RowAnchor::Hunk { file, hunk },
            Self::Note { file, note } => RowAnchor::Note { file, note },
        }
    }

    fn contains(self, anchor: RowAnchor) -> bool {
        match (self, anchor) {
            (Self::File { file }, RowAnchor::File { file: target })
            | (Self::Binary { file }, RowAnchor::Binary { file: target }) => file == target,
            (Self::Hunk { file, hunk }, RowAnchor::Hunk { file: target_file, hunk: target_hunk }) => {
                file == target_file && hunk == target_hunk
            }
            (
                Self::UnifiedLine { file, hunk, line, .. },
                RowAnchor::Line { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && line == target_line,
            (
                Self::SplitLine { file, hunk, old, new, .. },
                RowAnchor::Line { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && (old == Some(target_line) || new == Some(target_line)),
            (
                Self::MissingNewline { file, hunk, line },
                RowAnchor::MissingNewline { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && line == target_line,
            (Self::ContextGap { file, hunk, .. }, RowAnchor::Hunk { file: target_file, hunk: target_hunk }) => {
                file == target_file && hunk == target_hunk
            }
            (Self::Note { file, note }, RowAnchor::Note { file: target_file, note: target_note }) => {
                file == target_file && note == target_note
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NotePlacement {
    path: Vec<u8>,
    side: AnchorSide,
    hunk: Fingerprint,
    line: u64,
}

fn attach_notes(rows: &mut Vec<RowKey>, changeset: &Changeset, notes: &[ReviewNote], visible_notes: &[usize]) {
    let mut pending = BTreeMap::<NotePlacement, Vec<usize>>::new();
    let mut unplaced = Vec::new();
    for note_index in visible_notes.iter().copied() {
        let Some(note) = notes.get(note_index) else {
            continue;
        };
        let Some(anchor) = note.current_anchor() else {
            unplaced.push(note_index);
            continue;
        };
        let placement = NotePlacement {
            path: anchor.path().as_bytes().to_vec(),
            side: anchor.side(),
            hunk: anchor.hunk_fingerprint(),
            line: anchor.range().end().get(),
        };
        pending.entry(placement).or_default().push(note_index);
    }
    if pending.is_empty() && unplaced.is_empty() {
        return;
    }

    let original = std::mem::take(rows);
    let mut combined = Vec::with_capacity(original.len().saturating_add(visible_notes.len()));
    for (index, row) in original.iter().copied().enumerate() {
        combined.push(row);
        let coordinates = row_note_placements(row, changeset);
        let next = original.get(index + 1).copied();
        for placement in coordinates {
            if next.is_some_and(|next_row| row_note_placements(next_row, changeset).contains(&placement)) {
                continue;
            }
            if let Some(note_indices) = pending.remove(&placement) {
                let file = row.file();
                combined.extend(note_indices.into_iter().map(|note| RowKey::Note { file, note }));
            }
        }
    }
    for note_indices in pending.into_values().chain(std::iter::once(unplaced)) {
        for note in note_indices {
            let file = notes.get(note).and_then(|note| note_file(changeset, note)).unwrap_or(0);
            combined.push(RowKey::Note { file, note });
        }
    }
    *rows = combined;
}

fn note_file(changeset: &Changeset, note: &ReviewNote) -> Option<usize> {
    let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
    changeset.files().iter().position(|file| {
        file.old_side().is_some_and(|side| side.path == *anchor.path())
            || file.new_side().is_some_and(|side| side.path == *anchor.path())
    })
}

fn row_note_placements(row: RowKey, changeset: &Changeset) -> Vec<NotePlacement> {
    let (file, hunk, old, new) = match row {
        RowKey::UnifiedLine { file, hunk, line, .. } => (file, hunk, Some(line), Some(line)),
        RowKey::SplitLine { file, hunk, old, new, .. } => (file, hunk, old, new),
        _ => return Vec::new(),
    };
    let diff = &changeset.files()[file];
    let FileContent::Text { hunks } = diff.content() else {
        return Vec::new();
    };
    let source = &hunks[hunk];
    let mut placements = Vec::with_capacity(2);
    if let (Some(line), Some(side)) = (old, diff.old_side()) {
        if let Some(number) = source.lines()[line].old_line() {
            placements.push(note_placement(
                side.path.as_bytes(),
                AnchorSide::Old,
                source,
                number.get(),
            ));
        }
    }
    if let (Some(line), Some(side)) = (new, diff.new_side()) {
        if let Some(number) = source.lines()[line].new_line() {
            placements.push(note_placement(
                side.path.as_bytes(),
                AnchorSide::New,
                source,
                number.get(),
            ));
        }
    }
    placements
}

fn note_placement(path: &[u8], side: AnchorSide, hunk: &Hunk, line: u64) -> NotePlacement {
    NotePlacement { path: path.to_vec(), side, hunk: hunk.fingerprint(), line }
}

fn index_hunks(
    rows: &mut Vec<RowKey>, file: usize, hunks: &[Hunk], layout: ResolvedLayout, options: StreamOptions,
    expanded_hunks: &BTreeSet<(usize, usize)>, collapsed_hunks: &BTreeSet<(usize, usize)>,
) {
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        rows.push(RowKey::Hunk { file, hunk: hunk_index });
        if collapsed_hunks.contains(&(file, hunk_index)) {
            continue;
        }
        let options = if expanded_hunks.contains(&(file, hunk_index)) {
            StreamOptions { context_lines: usize::MAX, ..options }
        } else {
            options
        };
        match layout {
            ResolvedLayout::Unified => index_unified_hunk(rows, file, hunk_index, hunk, options),
            ResolvedLayout::Split => index_split_hunk(rows, file, hunk_index, hunk, options),
        }
    }
}

fn note_is_collapsed(
    changeset: &Changeset, notes: &[ReviewNote], note_index: usize, collapsed_files: &BTreeSet<usize>,
    collapsed_hunks: &BTreeSet<(usize, usize)>,
) -> bool {
    let Some(note) = notes.get(note_index) else {
        return true;
    };
    let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
    changeset.files().iter().enumerate().any(|(file_index, file)| {
        let path_matches = file.old_side().is_some_and(|side| side.path == *anchor.path())
            || file.new_side().is_some_and(|side| side.path == *anchor.path());
        if !path_matches {
            return false;
        }
        if collapsed_files.contains(&file_index) {
            return true;
        }
        let FileContent::Text { hunks } = file.content() else {
            return false;
        };
        hunks.iter().enumerate().any(|(hunk_index, hunk)| {
            hunk.fingerprint() == anchor.hunk_fingerprint() && collapsed_hunks.contains(&(file_index, hunk_index))
        })
    })
}

fn note_hunks(changeset: &Changeset, notes: &[ReviewNote], visible_notes: &[usize]) -> BTreeSet<(usize, usize)> {
    let mut result = BTreeSet::new();
    for note_index in visible_notes {
        let Some(note) = notes.get(*note_index) else {
            continue;
        };
        let Some(anchor) = note.current_anchor() else {
            continue;
        };
        for (file_index, file) in changeset.files().iter().enumerate() {
            let path_matches = file.old_side().is_some_and(|side| side.path == *anchor.path())
                || file.new_side().is_some_and(|side| side.path == *anchor.path());
            if !path_matches {
                continue;
            }
            let FileContent::Text { hunks } = file.content() else {
                continue;
            };
            if let Some(hunk) = hunks
                .iter()
                .position(|hunk| hunk.fingerprint() == anchor.hunk_fingerprint())
            {
                result.insert((file_index, hunk));
            }
        }
    }
    result
}

fn index_unified_hunk(rows: &mut Vec<RowKey>, file: usize, hunk: usize, source: &Hunk, options: StreamOptions) {
    let mut hidden = 0;
    for (line_index, line) in source.lines().iter().enumerate() {
        if !context_visible(source.lines(), line_index, options.context_lines) {
            hidden += 1;
            continue;
        }
        push_context_gap(rows, file, hunk, &mut hidden);
        let width = options
            .wrap_width
            .map(|width| usize::from(width).saturating_sub(LINE_GUTTER_WIDTH));
        for range in text_ranges(line.content(), width) {
            rows.push(RowKey::UnifiedLine { file, hunk, line: line_index, range });
        }
        if !matches!(line.missing_newline(), mire_core::MissingNewline::None) {
            rows.push(RowKey::MissingNewline { file, hunk, line: line_index });
        }
    }
    push_context_gap(rows, file, hunk, &mut hidden);
}

fn index_split_hunk(rows: &mut Vec<RowKey>, file: usize, hunk: usize, source: &Hunk, options: StreamOptions) {
    let lines = source.lines();
    let mut index = 0;
    while index < lines.len() {
        if matches!(lines[index].kind(), LineKind::Context) {
            if !context_visible(lines, index, options.context_lines) {
                let mut hidden = 1;
                while index + hidden < lines.len()
                    && matches!(lines[index + hidden].kind(), LineKind::Context)
                    && !context_visible(lines, index + hidden, options.context_lines)
                {
                    hidden += 1;
                }
                rows.push(RowKey::ContextGap { file, hunk, hidden });
                index += hidden;
                continue;
            }
            push_split_rows(rows, file, hunk, lines, Some(index), Some(index), options.wrap_width);
            push_missing_newline(rows, file, hunk, index, lines[index].missing_newline());
            index += 1;
            continue;
        }

        let start = index;
        while index < lines.len() && !matches!(lines[index].kind(), LineKind::Context) {
            index += 1;
        }
        let deletions = (start..index)
            .filter(|line| matches!(lines[*line].kind(), LineKind::Deletion))
            .collect::<Vec<_>>();
        let additions = (start..index)
            .filter(|line| matches!(lines[*line].kind(), LineKind::Addition))
            .collect::<Vec<_>>();
        for pair in 0..deletions.len().max(additions.len()) {
            let old = deletions.get(pair).copied();
            let new = additions.get(pair).copied();
            push_split_rows(rows, file, hunk, lines, old, new, options.wrap_width);
            if let Some(line) = old {
                push_missing_newline(rows, file, hunk, line, lines[line].missing_newline());
            }
            if let Some(line) = new {
                push_missing_newline(rows, file, hunk, line, lines[line].missing_newline());
            }
        }
    }
}

fn push_split_rows(
    rows: &mut Vec<RowKey>, file: usize, hunk: usize, lines: &[mire_core::DiffLine], old: Option<usize>,
    new: Option<usize>, wrap_width: Option<u16>,
) {
    let width = wrap_width
        .map(|width| usize::from(width).saturating_sub(3) / 2)
        .map(|width| width.saturating_sub(7));
    let old_ranges = old.map_or_else(Vec::new, |line| text_ranges(lines[line].content(), width));
    let new_ranges = new.map_or_else(Vec::new, |line| text_ranges(lines[line].content(), width));
    for segment in 0..old_ranges.len().max(new_ranges.len()).max(1) {
        rows.push(RowKey::SplitLine {
            file,
            hunk,
            old,
            new,
            old_range: old_ranges.get(segment).copied(),
            new_range: new_ranges.get(segment).copied(),
        });
    }
}

fn context_visible(lines: &[mire_core::DiffLine], index: usize, limit: usize) -> bool {
    if !matches!(lines[index].kind(), LineKind::Context) {
        return true;
    }
    let before = lines[..index]
        .iter()
        .rev()
        .take_while(|line| matches!(line.kind(), LineKind::Context))
        .count();
    let after = lines[index + 1..]
        .iter()
        .take_while(|line| matches!(line.kind(), LineKind::Context))
        .count();
    before < limit || after < limit
}

fn push_context_gap(rows: &mut Vec<RowKey>, file: usize, hunk: usize, hidden: &mut usize) {
    if *hidden > 0 {
        rows.push(RowKey::ContextGap { file, hunk, hidden: *hidden });
        *hidden = 0;
    }
}

fn text_ranges(bytes: &[u8], width: Option<usize>) -> Vec<TextRange> {
    let Some(width) = width.filter(|width| *width > 0) else {
        return vec![TextRange { start: 0, end: bytes.len() }];
    };
    let source = String::from_utf8_lossy(bytes);
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut display_width = 0;
    for (index, character) in source.char_indices() {
        let character_width = Span::raw(character.to_string()).width().max(1);
        if display_width + character_width > width && index > start {
            ranges.push(TextRange { start, end: index });
            start = index;
            display_width = 0;
        }
        display_width += character_width;
    }
    ranges.push(TextRange { start, end: source.len() });
    ranges
}

fn push_missing_newline(
    rows: &mut Vec<RowKey>, file: usize, hunk: usize, line: usize, marker: mire_core::MissingNewline,
) {
    if !matches!(marker, mire_core::MissingNewline::None) {
        let key = RowKey::MissingNewline { file, hunk, line };
        if rows.last() != Some(&key) {
            rows.push(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    const ALIGNED_PATCH: &[u8] =
        b"--- a/file.txt\n+++ b/file.txt\n@@ -1,4 +1,3 @@\n same\n-old one\n-old two\n+new one\n last\n";

    #[test]
    fn viewport_iteration_never_resolves_off_screen_rows() {
        let mut patch = String::from("--- a/large.txt\n+++ b/large.txt\n@@ -1,100 +1,100 @@\n");
        for index in 0..100 {
            patch.push_str(&format!(" line {index}\n"));
        }
        let changeset = parse_patch(
            patch.as_bytes(),
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let stream = ReviewStream::with_options(
            &changeset,
            ResolvedLayout::Unified,
            StreamOptions { context_lines: 100, wrap_width: None },
        );

        assert_eq!(stream.visible_kinds(50, 7).count(), 7);
        assert_eq!(stream.visible_kinds(10_000, 7).count(), 0);
    }

    #[test]
    fn layout_split_rows_align_changes_and_preserve_source_identity() {
        let changeset = parse_patch(
            ALIGNED_PATCH,
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let unified = ReviewStream::new(&changeset, ResolvedLayout::Unified);
        let split = ReviewStream::new(&changeset, ResolvedLayout::Split);

        assert_eq!(unified.len(), 7);
        assert_eq!(split.len(), 6);
        assert!(split.rows.iter().any(|row| matches!(
            row,
            RowKey::SplitLine { file: 0, hunk: 0, old: Some(1), new: Some(3), .. }
        )));
        assert!(
            split
                .rows
                .iter()
                .any(|row| matches!(row, RowKey::SplitLine { file: 0, hunk: 0, old: Some(2), new: None, .. }))
        );
        for line in 0..5 {
            let anchor = RowAnchor::Line { file: 0, hunk: 0, line };
            assert!(unified.index_of_anchor(anchor).is_some());
            assert!(split.index_of_anchor(anchor).is_some());
        }
    }

    #[test]
    fn layout_automatic_uses_review_width() {
        assert_eq!(LayoutMode::Automatic.resolve(71), ResolvedLayout::Unified);
        assert_eq!(LayoutMode::Automatic.resolve(72), ResolvedLayout::Split);
    }

    #[test]
    fn wrapping_uses_terminal_display_width() {
        assert_eq!(
            text_ranges("界x".as_bytes(), Some(2)),
            vec![TextRange { start: 0, end: 3 }, TextRange { start: 3, end: 4 }]
        );
    }
}
