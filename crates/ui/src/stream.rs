use mire_core::{Changeset, FileContent, FileDiff, Hunk};

/// The semantic kind of a row in the continuous unified review stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    /// A file heading.
    File,
    /// A binary-file marker.
    Binary,
    /// A unified hunk heading.
    Hunk,
    /// A source line from a hunk.
    Line,
    /// A missing-final-newline marker attached to the preceding line.
    MissingNewline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKey {
    File { file: usize },
    Binary { file: usize },
    Hunk { file: usize, hunk: usize },
    Line { file: usize, hunk: usize, line: usize },
    MissingNewline { file: usize, hunk: usize, line: usize },
}

/// A lightweight row index over an immutable changeset.
///
/// The index stores coordinates rather than rendered text or widgets. Rendering
/// resolves only the slice intersecting the current viewport.
#[derive(Debug)]
pub struct ReviewStream<'a> {
    changeset: &'a Changeset,
    rows: Vec<RowKey>,
}

impl ReviewStream<'_> {
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
}

impl<'a> ReviewStream<'a> {
    /// Indexes every logical row without constructing off-screen widgets.
    pub fn new(changeset: &'a Changeset) -> Self {
        let mut rows = Vec::new();
        for (file_index, file) in changeset.files().iter().enumerate() {
            rows.push(RowKey::File { file: file_index });
            match file.content() {
                FileContent::Binary => rows.push(RowKey::Binary { file: file_index }),
                FileContent::Text { hunks } => index_hunks(&mut rows, file_index, hunks),
            }
        }
        Self { changeset, rows }
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
    const fn kind(self) -> RowKind {
        match self {
            Self::File { .. } => RowKind::File,
            Self::Binary { .. } => RowKind::Binary,
            Self::Hunk { .. } => RowKind::Hunk,
            Self::Line { .. } => RowKind::Line,
            Self::MissingNewline { .. } => RowKind::MissingNewline,
        }
    }
}

fn index_hunks(rows: &mut Vec<RowKey>, file: usize, hunks: &[Hunk]) {
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        rows.push(RowKey::Hunk { file, hunk: hunk_index });
        for (line_index, line) in hunk.lines().iter().enumerate() {
            rows.push(RowKey::Line { file, hunk: hunk_index, line: line_index });
            if !matches!(line.missing_newline(), mire_core::MissingNewline::None) {
                rows.push(RowKey::MissingNewline { file, hunk: hunk_index, line: line_index });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

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
        let stream = ReviewStream::new(&changeset);

        assert_eq!(stream.visible_kinds(50, 7).count(), 7);
        assert_eq!(stream.visible_kinds(10_000, 7).count(), 0);
    }
}
