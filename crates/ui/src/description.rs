//! Layout of the changeset title and description dialog.

use ratatui::layout::Rect;
use ratatui::text::Span;

/// Text shown in the dialog when the changeset has no description.
pub const EMPTY_DESCRIPTION: &str = "No description.";

const TAB_WIDTH: usize = 4;

/// The geometry and wrapped text of the description dialog for one frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptionDialog {
    /// Screen area of the dialog, including its border.
    pub area: Rect,
    /// Description text wrapped to the dialog's inner width.
    pub lines: Vec<String>,
    /// First wrapped line shown, clamped so the last page stays full.
    pub scroll: usize,
}

impl DescriptionDialog {
    /// Returns the number of text rows inside the dialog border.
    pub const fn text_height(&self) -> usize {
        self.area.height.saturating_sub(2) as usize
    }

    /// Returns the largest scroll offset that still fills the dialog.
    pub fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(self.text_height())
    }
}

/// Wraps text into lines no wider than `width` terminal cells.
///
/// Hard line breaks are kept, words wrap at spaces, and a word wider than
/// the line is split. Tabs become spaces and other control characters are
/// dropped so the text cannot move the terminal cursor.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        wrap_paragraph(&sanitize(paragraph), width, &mut lines);
    }
    lines
}

/// Replaces control characters in a single-line value with spaces.
pub fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .collect()
}

fn sanitize(paragraph: &str) -> String {
    let mut result = String::with_capacity(paragraph.len());
    for character in paragraph.chars() {
        match character {
            '\t' => result.push_str(&" ".repeat(TAB_WIDTH)),
            character if character.is_control() => {}
            character => result.push(character),
        }
    }
    result
}

fn wrap_paragraph(paragraph: &str, width: usize, lines: &mut Vec<String>) {
    let mut line = String::new();
    let mut used = 0_usize;
    let mut started = false;
    for word in paragraph.split(' ') {
        let word_width = text_width(word);
        let separator = usize::from(started);
        if used + separator + word_width <= width {
            if started {
                line.push(' ');
            }
            line.push_str(word);
            used += separator + word_width;
            started = true;
            continue;
        }
        if started {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        started = true;
        if word_width <= width {
            line.push_str(word);
            used = word_width;
            continue;
        }
        for character in word.chars() {
            let character_width = char_width(character);
            if used + character_width > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(character);
            used += character_width;
        }
    }
    lines.push(line);
}

fn text_width(value: &str) -> usize {
    value.chars().map(char_width).sum()
}

fn char_width(character: char) -> usize {
    let mut buffer = [0_u8; 4];
    Span::raw(&*character.encode_utf8(&mut buffer)).width().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_breaks_and_wraps_words_at_spaces() {
        assert_eq!(
            wrap_text("one two three\n\nfour", 7),
            vec!["one two", "three", "", "four"]
        );
    }

    #[test]
    fn wrap_splits_long_words_and_keeps_indentation() {
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("  - item", 20), vec!["  - item"]);
        assert_eq!(wrap_text("界界界", 4), vec!["界界", "界"]);
    }

    #[test]
    fn wrap_expands_tabs_and_drops_control_characters() {
        assert_eq!(wrap_text("a\tb\r\n\u{1b}[2Jc", 20), vec!["a    b", "[2Jc"]);
        assert_eq!(single_line("a\u{1b}b"), "a b");
    }
}
