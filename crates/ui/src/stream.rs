use mire_core::{Changeset, FileContent, FileDiff, Hunk, LineKind};

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
    },
    SplitLine {
        file: usize,
        hunk: usize,
        old: Option<usize>,
        new: Option<usize>,
    },
    MissingNewline {
        file: usize,
        hunk: usize,
        line: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowAnchor {
    File { file: usize },
    Binary { file: usize },
    Hunk { file: usize, hunk: usize },
    Line { file: usize, hunk: usize, line: usize },
    MissingNewline { file: usize, hunk: usize, line: usize },
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
        let mut rows = Vec::new();
        for (file_index, file) in changeset.files().iter().enumerate() {
            rows.push(RowKey::File { file: file_index });
            match file.content() {
                FileContent::Binary => rows.push(RowKey::Binary { file: file_index }),
                FileContent::Text { hunks } => index_hunks(&mut rows, file_index, hunks, layout),
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
        }
    }

    pub const fn file(self) -> usize {
        match self {
            Self::File { file }
            | Self::Binary { file }
            | Self::Hunk { file, .. }
            | Self::UnifiedLine { file, .. }
            | Self::SplitLine { file, .. }
            | Self::MissingNewline { file, .. } => file,
        }
    }

    pub fn anchor(self) -> RowAnchor {
        match self {
            Self::File { file } => RowAnchor::File { file },
            Self::Binary { file } => RowAnchor::Binary { file },
            Self::Hunk { file, hunk } => RowAnchor::Hunk { file, hunk },
            Self::UnifiedLine { file, hunk, line } => RowAnchor::Line { file, hunk, line },
            Self::SplitLine { file, hunk, old: Some(line), .. }
            | Self::SplitLine { file, hunk, old: None, new: Some(line) } => RowAnchor::Line { file, hunk, line },
            Self::SplitLine { old: None, new: None, .. } => {
                unreachable!("split rows always contain an old or new source line")
            }
            Self::MissingNewline { file, hunk, line } => RowAnchor::MissingNewline { file, hunk, line },
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
                Self::UnifiedLine { file, hunk, line },
                RowAnchor::Line { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && line == target_line,
            (
                Self::SplitLine { file, hunk, old, new },
                RowAnchor::Line { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && (old == Some(target_line) || new == Some(target_line)),
            (
                Self::MissingNewline { file, hunk, line },
                RowAnchor::MissingNewline { file: target_file, hunk: target_hunk, line: target_line },
            ) => file == target_file && hunk == target_hunk && line == target_line,
            _ => false,
        }
    }
}

fn index_hunks(rows: &mut Vec<RowKey>, file: usize, hunks: &[Hunk], layout: ResolvedLayout) {
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        rows.push(RowKey::Hunk { file, hunk: hunk_index });
        match layout {
            ResolvedLayout::Unified => index_unified_hunk(rows, file, hunk_index, hunk),
            ResolvedLayout::Split => index_split_hunk(rows, file, hunk_index, hunk),
        }
    }
}

fn index_unified_hunk(rows: &mut Vec<RowKey>, file: usize, hunk: usize, source: &Hunk) {
    for (line_index, line) in source.lines().iter().enumerate() {
        rows.push(RowKey::UnifiedLine { file, hunk, line: line_index });
        if !matches!(line.missing_newline(), mire_core::MissingNewline::None) {
            rows.push(RowKey::MissingNewline { file, hunk, line: line_index });
        }
    }
}

fn index_split_hunk(rows: &mut Vec<RowKey>, file: usize, hunk: usize, source: &Hunk) {
    let lines = source.lines();
    let mut index = 0;
    while index < lines.len() {
        if matches!(lines[index].kind(), LineKind::Context) {
            rows.push(RowKey::SplitLine { file, hunk, old: Some(index), new: Some(index) });
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
            rows.push(RowKey::SplitLine { file, hunk, old, new });
            if let Some(line) = old {
                push_missing_newline(rows, file, hunk, line, lines[line].missing_newline());
            }
            if let Some(line) = new {
                push_missing_newline(rows, file, hunk, line, lines[line].missing_newline());
            }
        }
    }
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
        let stream = ReviewStream::new(&changeset, ResolvedLayout::Unified);

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
        assert!(
            split
                .rows
                .contains(&RowKey::SplitLine { file: 0, hunk: 0, old: Some(1), new: Some(3) })
        );
        assert!(
            split
                .rows
                .contains(&RowKey::SplitLine { file: 0, hunk: 0, old: Some(2), new: None })
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
}
