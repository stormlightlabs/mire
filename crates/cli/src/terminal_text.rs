//! Escaping of stored text before it is written to a terminal.

use std::borrow::Cow;

/// Replaces every control character except `\n` and `\t` with its visible
/// `\u{..}` escape.
///
/// Review files hold text written by agents and other tools. Escaping ESC,
/// CSI, OSC, BEL, CR, NUL and the other C0 and C1 controls keeps that text
/// from moving the cursor, clearing the screen or rewriting earlier lines.
pub fn escape_controls(text: &str) -> Cow<'_, str> {
    if !text.chars().any(needs_escape) {
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

fn needs_escape(character: char) -> bool {
    character.is_control() && character != '\n' && character != '\t'
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
    fn plain_text_is_borrowed() {
        assert!(matches!(escape_controls("plain\ttext\n"), Cow::Borrowed(_)));
    }
}
