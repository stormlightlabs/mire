use mire_core::{AnnotationKind, AuthorKind, NoteSeverity, NoteStatus, ReviewNote};

/// Optional note facets applied together to the review stream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoteFilter {
    author_kind: Option<AuthorKind>,
    status: Option<NoteStatus>,
    severity: Option<NoteSeverity>,
    annotation_kind: Option<AnnotationKind>,
    file: Option<usize>,
}

impl NoteFilter {
    /// Reports whether a note belongs in the current stream.
    pub fn includes(self, note: &ReviewNote, file: usize) -> bool {
        self.author_kind
            .is_none_or(|value| value == note.provenance().author_kind())
            && self.status.is_none_or(|value| value == note.status())
            && self.severity.is_none_or(|value| value == note.severity())
            && self.annotation_kind.is_none_or(|value| value == note.annotation_kind())
            && self.file.is_none_or(|value| value == file)
    }

    /// Removes every active facet.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn cycle_annotation_kind(&mut self) {
        self.annotation_kind = cycle(
            self.annotation_kind,
            &[
                AnnotationKind::Comment,
                AnnotationKind::Defect,
                AnnotationKind::Suggestion,
                AnnotationKind::Question,
            ],
        );
    }

    pub fn cycle_author_kind(&mut self) {
        self.author_kind = cycle(
            self.author_kind,
            &[AuthorKind::Human, AuthorKind::Agent, AuthorKind::Tool],
        );
    }

    pub fn cycle_file(&mut self, file_count: usize) {
        self.file = match (self.file, file_count) {
            (_, 0) => None,
            (None, _) => Some(0),
            (Some(value), count) if value + 1 < count => Some(value + 1),
            (Some(_), _) => None,
        };
    }

    pub fn cycle_severity(&mut self) {
        self.severity = cycle(
            self.severity,
            &[
                NoteSeverity::Note,
                NoteSeverity::Low,
                NoteSeverity::Medium,
                NoteSeverity::High,
                NoteSeverity::Critical,
            ],
        );
    }

    pub fn cycle_status(&mut self) {
        self.status = cycle(
            self.status,
            &[
                NoteStatus::Open,
                NoteStatus::Resolved,
                NoteStatus::Dismissed,
                NoteStatus::AcceptedRisk,
            ],
        );
    }

    pub fn summary(self) -> String {
        format!(
            "author={} status={} severity={} kind={} file={}",
            label(self.author_kind),
            label(self.status),
            label(self.severity),
            label(self.annotation_kind),
            self.file
                .map_or_else(|| "all".to_owned(), |file| (file + 1).to_string())
        )
    }
}

fn cycle<T: Copy + PartialEq>(current: Option<T>, values: &[T]) -> Option<T> {
    match current {
        None => values.first().copied(),
        Some(current) => values
            .iter()
            .position(|value| *value == current)
            .and_then(|position| values.get(position + 1).copied()),
    }
}

fn label<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "all".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_cycle_returns_to_all() {
        let mut filter = NoteFilter::default();
        for _ in 0..4 {
            filter.cycle_author_kind();
        }
        for _ in 0..5 {
            filter.cycle_status();
        }
        for _ in 0..6 {
            filter.cycle_severity();
        }
        for _ in 0..5 {
            filter.cycle_annotation_kind();
        }
        for _ in 0..4 {
            filter.cycle_file(3);
        }
        assert_eq!(filter, NoteFilter::default());
    }
}
