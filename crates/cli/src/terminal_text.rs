//! Escaping of stored text before it is written to a terminal or a Markdown
//! export.

use std::borrow::Cow;

/// Replaces every control and invisible formatting character except `\n` and
/// `\t` with its visible `\u{..}` escape.
///
/// Review files hold text written by agents and other tools. Escaping ESC,
/// CSI, OSC, BEL, CR, NUL and the other C0 and C1 controls keeps that text
/// from moving the cursor, clearing the screen or rewriting earlier lines.
/// Escaping bidirectional controls, zero-width characters and the Unicode
/// line and paragraph separators keeps it from showing text in an order or
/// layout other than the one stored.
///
/// A run of backslashes just before `u{` or before an escaped character is
/// doubled, so stored text that spells out an escape stays distinct from an
/// escaped character: an even run before `u{` is literal text, and an odd run
/// ends in the backslash that starts an escape. Other backslashes are kept.
pub fn escape_controls(text: &str) -> Cow<'_, str> {
    escape_where(text, |character| {
        needs_escape(character) && character != '\n' && character != '\t'
    })
}

/// Escapes like [`escape_controls`], and also escapes `\n` and `\t`.
///
/// Use it for text that must stay on one line, such as an author name or a
/// path inside a Markdown list item.
pub fn escape_line_controls(text: &str) -> Cow<'_, str> {
    escape_where(text, needs_escape)
}

/// Reports whether a character is a control or an invisible formatting
/// character that could change how the text around it is shown.
fn needs_escape(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

fn escape_where(text: &str, needs_escape: impl Fn(char) -> bool) -> Cow<'_, str> {
    if !text.chars().any(&needs_escape) && !text.contains("\\u{") {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    let mut backslashes = 0;
    for (index, character) in text.char_indices() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        let escape = needs_escape(character);
        let starts_escape_text = text[index..].starts_with("u{");
        let run_length = if escape || starts_escape_text { backslashes * 2 } else { backslashes };
        escaped.extend(std::iter::repeat_n('\\', run_length));
        backslashes = 0;
        if escape {
            escaped.extend(character.escape_unicode());
        } else {
            escaped.push(character);
        }
    }
    escaped.extend(std::iter::repeat_n('\\', backslashes));
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
    fn bidi_zero_width_and_separator_characters_become_visible() {
        let hidden = [
            ('\u{80}', "\\u{80}"),
            ('\u{85}', "\\u{85}"),
            ('\u{9f}', "\\u{9f}"),
            ('\u{61c}', "\\u{61c}"),
            ('\u{200b}', "\\u{200b}"),
            ('\u{200c}', "\\u{200c}"),
            ('\u{200d}', "\\u{200d}"),
            ('\u{200e}', "\\u{200e}"),
            ('\u{200f}', "\\u{200f}"),
            ('\u{2028}', "\\u{2028}"),
            ('\u{2029}', "\\u{2029}"),
            ('\u{202a}', "\\u{202a}"),
            ('\u{202b}', "\\u{202b}"),
            ('\u{202c}', "\\u{202c}"),
            ('\u{202d}', "\\u{202d}"),
            ('\u{202e}', "\\u{202e}"),
            ('\u{2060}', "\\u{2060}"),
            ('\u{2066}', "\\u{2066}"),
            ('\u{2067}', "\\u{2067}"),
            ('\u{2068}', "\\u{2068}"),
            ('\u{2069}', "\\u{2069}"),
            ('\u{feff}', "\\u{feff}"),
        ];
        for (character, escape) in hidden {
            let text = format!("a{character}b");
            for output in [escape_controls(&text), escape_line_controls(&text)] {
                assert!(!output.contains(character), "{character:?} reached {output:?}");
                assert_eq!(output, format!("a{escape}b"));
            }
        }
    }

    #[test]
    fn a_spelled_out_escape_stays_distinct_from_an_escaped_character() {
        let spelled = escape_controls("\\u{1b}[2J");
        let escaped = escape_controls("\u{1b}[2J");
        assert_eq!(spelled, "\\\\u{1b}[2J");
        assert_eq!(escaped, "\\u{1b}[2J");
        assert_ne!(spelled, escaped);
        assert_eq!(escape_line_controls("a\\u{"), "a\\\\u{");
        assert_ne!(escape_controls("\\\u{1b}"), escape_controls("\\u{1b}"));
        assert_eq!(escape_controls("\\\u{1b}"), "\\\\\\u{1b}");
        assert_eq!(escape_controls("\\\\u{"), "\\\\\\\\u{");
    }

    #[test]
    fn other_backslashes_are_kept() {
        let text = r"\d+\s*\\n A \x1b \";
        assert!(matches!(escape_controls(text), Cow::Borrowed(_)));
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
