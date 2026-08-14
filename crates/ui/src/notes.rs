use mire_core::{
    Anchor, AnchorSide, AnnotationKind, BytePath, LineNumber, LineRange, NoteId, NoteSeverity, ReviewNote,
};

use crate::stream::{ReviewStream, RowKey};

/// Whether the editor creates a note or changes an existing note.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorTarget {
    /// A new note anchored to the selected source range.
    New(LineSelection),
    /// An edit to the note with this stable identifier.
    Existing(NoteId),
}

/// One contiguous source range selected for a new note.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineSelection {
    start: SelectionEndpoint,
    end: SelectionEndpoint,
}

/// Recoverable text and classification state for the note editor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteEditor {
    target: EditorTarget,
    body: String,
    severity: NoteSeverity,
    annotation_kind: AnnotationKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionEndpoint {
    file: usize,
    hunk: usize,
    side: AnchorSide,
    line: LineNumber,
}

impl LineSelection {
    pub fn from_row(stream: &ReviewStream<'_>, row: RowKey, prefer_old: bool) -> Option<Self> {
        endpoint(stream, row, prefer_old).map(|endpoint| Self { start: endpoint, end: endpoint })
    }

    pub fn extend_to_row(&mut self, stream: &ReviewStream<'_>, row: RowKey) {
        let Some(endpoint) = endpoint(stream, row, matches!(self.start.side, AnchorSide::Old)) else {
            return;
        };
        if endpoint.file == self.start.file && endpoint.hunk == self.start.hunk && endpoint.side == self.start.side {
            self.end = endpoint;
        }
    }

    pub fn anchor(self, stream: &ReviewStream<'_>) -> Result<Anchor, mire_core::ReviewError> {
        let file = stream.file(self.start.file);
        let path: BytePath = match self.start.side {
            AnchorSide::Old => file.old_side(),
            AnchorSide::New => file.new_side(),
        }
        .ok_or(mire_core::ReviewError::FileSideNotFound { side: self.start.side })?
        .path
        .clone();
        let start = self.start.line.min(self.end.line);
        let end = self.start.line.max(self.end.line);
        Anchor::new(
            stream.changeset(),
            path,
            self.start.side,
            LineRange::new(start, end)?,
            stream.hunk(self.start.file, self.start.hunk).fingerprint(),
        )
    }

    /// Returns this selection in Git's side-qualified file-location notation.
    pub fn label(self, stream: &ReviewStream<'_>) -> String {
        let path = match self.start.side {
            AnchorSide::Old => stream.file(self.start.file).old_side(),
            AnchorSide::New => stream.file(self.start.file).new_side(),
        }
        .map_or_else(
            || "<unknown>".to_owned(),
            |side| String::from_utf8_lossy(side.path.as_bytes()).into_owned(),
        );
        let prefix = match self.start.side {
            AnchorSide::Old => "a",
            AnchorSide::New => "b",
        };
        let start = self.start.line.min(self.end.line).get();
        let end = self.start.line.max(self.end.line).get();
        format!("{prefix}/{path}:{start}-{end}")
    }

    /// Reports whether a source location is contained by this active selection.
    pub fn contains(self, file: usize, hunk: usize, side: AnchorSide, line: LineNumber) -> bool {
        file == self.start.file
            && hunk == self.start.hunk
            && side == self.start.side
            && self.start.line.min(self.end.line) <= line
            && line <= self.start.line.max(self.end.line)
    }
}

impl NoteEditor {
    pub fn create(selection: LineSelection) -> Self {
        Self {
            target: EditorTarget::New(selection),
            body: String::new(),
            severity: NoteSeverity::Note,
            annotation_kind: AnnotationKind::Comment,
        }
    }

    pub fn edit(note: &ReviewNote) -> Self {
        Self {
            target: EditorTarget::Existing(note.id().clone()),
            body: note.body().to_owned(),
            severity: note.severity(),
            annotation_kind: note.annotation_kind(),
        }
    }

    /// Returns whether this buffer creates a note or edits an existing one.
    pub const fn target(&self) -> &EditorTarget {
        &self.target
    }

    /// Returns the complete unsaved note text.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the severity selected in the editor.
    pub const fn severity(&self) -> NoteSeverity {
        self.severity
    }

    /// Returns the annotation category selected in the editor.
    pub const fn annotation_kind(&self) -> AnnotationKind {
        self.annotation_kind
    }

    pub fn push(&mut self, character: char) {
        self.body.push(character);
    }

    pub fn backspace(&mut self) {
        self.body.pop();
    }

    pub fn cycle_annotation_kind(&mut self) {
        self.annotation_kind = match self.annotation_kind {
            AnnotationKind::Comment => AnnotationKind::Defect,
            AnnotationKind::Defect => AnnotationKind::Suggestion,
            AnnotationKind::Suggestion => AnnotationKind::Question,
            AnnotationKind::Question => AnnotationKind::Comment,
        };
    }

    pub fn cycle_severity(&mut self) {
        self.severity = match self.severity {
            NoteSeverity::Note => NoteSeverity::Low,
            NoteSeverity::Low => NoteSeverity::Medium,
            NoteSeverity::Medium => NoteSeverity::High,
            NoteSeverity::High => NoteSeverity::Critical,
            NoteSeverity::Critical => NoteSeverity::Note,
        };
    }

    pub fn set_existing(&mut self, note_id: NoteId) {
        self.target = EditorTarget::Existing(note_id);
    }
}

fn endpoint(stream: &ReviewStream<'_>, row: RowKey, prefer_old: bool) -> Option<SelectionEndpoint> {
    let (file, hunk, line) = match row {
        RowKey::UnifiedLine { file, hunk, line, .. } => (file, hunk, line),
        RowKey::SplitLine { file, hunk, old, new, .. } => {
            let line = if prefer_old { old.or(new) } else { new.or(old) }?;
            (file, hunk, line)
        }
        _ => return None,
    };
    let source = &stream.hunk(file, hunk).lines()[line];
    let (side, line) = if prefer_old {
        source
            .old_line()
            .map(|line| (AnchorSide::Old, line))
            .or_else(|| source.new_line().map(|line| (AnchorSide::New, line)))?
    } else {
        source
            .new_line()
            .map(|line| (AnchorSide::New, line))
            .or_else(|| source.old_line().map(|line| (AnchorSide::Old, line)))?
    };
    Some(SelectionEndpoint { file, hunk, side, line })
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;
    use crate::stream::ResolvedLayout;

    #[test]
    fn selection_labels_use_git_side_prefixes_and_relative_paths() {
        let patch = b"--- a/path/to/source.rs\n+++ b/path/to/target.rs\n@@ -10,2 +20,2 @@\n-old first\n-old second\n+new first\n+new second\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let stream = ReviewStream::new(&changeset, ResolvedLayout::Unified);
        let rows = stream.visible_keys(0, stream.len()).collect::<Vec<_>>();

        let old = LineSelection::from_row(&stream, rows[2], true).unwrap();
        let new = LineSelection::from_row(&stream, rows[4], false).unwrap();

        assert_eq!(old.label(&stream), "a/path/to/source.rs:10-10");
        assert_eq!(new.label(&stream), "b/path/to/target.rs:20-20");
    }
}
