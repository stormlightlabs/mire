//! Escaping of stored text before it is written to a terminal.

use std::borrow::Cow;

/// Replaces every control character except `\n` and `\t` with its visible
/// `\u{..}` escape.
///
/// Review files hold text written by agents and other tools. Escaping ESC,
/// CSI, OSC, BEL, CR, NUL and the other C0 and C1 controls keeps that text
/// from moving the cursor, clearing the screen or rewriting earlier lines.
pub fn escape_controls(text: &str) -> Cow<'_, str> {
    escape_where(text, |character| {
        character.is_control() && character != '\n' && character != '\t'
    })
}

/// Replaces every control character, `\n` and `\t` included, with its
/// visible `\u{..}` escape.
///
/// Use it for text that must stay on one line, such as an author name or a
/// path inside a Markdown list item.
pub fn escape_line_controls(text: &str) -> Cow<'_, str> {
    escape_where(text, char::is_control)
}

fn escape_where(text: &str, needs_escape: impl Fn(char) -> bool) -> Cow<'_, str> {
    if !text.chars().any(&needs_escape) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if needs_escape(character) {
            escaped.extend(character.escape_unicode());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_become_visible_and_layout_survives() {
        let text = "a\u{1b}[2Jb\u{1b}]0;title\u{7}c\rd\0e\u{9b}f\u{7f}\n\tg";
        assert_eq!(
            escape_controls(text),
            "a\\u{1b}[2Jb\\u{1b}]0;title\\u{7}c\\u{d}d\\u{0}e\\u{9b}f\\u{7f}\n\tg"
        );
    }

    #[test]
    fn line_escape_also_escapes_newlines_and_tabs() {
        assert_eq!(escape_line_controls("a\nb\tc\u{1b}d"), "a\\u{a}b\\u{9}c\\u{1b}d");
    }

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(escape_controls("plain\ttext\n"), Cow::Borrowed(_)));
    }
}
