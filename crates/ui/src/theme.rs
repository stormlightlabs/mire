use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

/// Semantic styles used by review rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    syntax_colors: bool,
    pub title: Style,
    pub selected: Style,
    pub divider: Style,
    pub file: Style,
    pub hunk: Style,
    pub addition: Style,
    pub deletion: Style,
    pub context: Style,
    pub marker: Style,
    pub error: Style,
    pub footer: Style,
    pub search_match: Style,
    pub intraline: Style,
}

impl Theme {
    /// Detects a light or dark terminal and falls back to a dark theme.
    ///
    /// `NO_COLOR` disables colored diff rows and avoids querying the terminal.
    pub fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::plain();
        }
        if std::env::var_os("TERM").is_some_and(|value| value == "dumb") {
            return Self::low_color();
        }
        match theme_mode(QueryOptions::default()).unwrap_or(ThemeMode::Dark) {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }

    /// Returns deterministic colors intended for dark terminal backgrounds.
    pub const fn dark() -> Self {
        Self {
            syntax_colors: true,
            title: Style::new()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
            selected: Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            divider: Style::new().fg(Color::DarkGray),
            file: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::LightBlue),
            addition: Style::new().fg(Color::LightGreen),
            deletion: Style::new().fg(Color::LightRed),
            context: Style::new().fg(Color::Gray),
            marker: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            footer: Style::new().fg(Color::DarkGray),
            search_match: Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            intraline: Style::new()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Returns deterministic colors intended for light terminal backgrounds.
    pub const fn light() -> Self {
        Self {
            syntax_colors: true,
            title: Style::new()
                .fg(Color::White)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
            selected: Style::new()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            divider: Style::new().fg(Color::DarkGray),
            file: Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::Blue),
            addition: Style::new().fg(Color::Green),
            deletion: Style::new().fg(Color::Red),
            context: Style::new().fg(Color::DarkGray),
            marker: Style::new().fg(Color::Magenta),
            error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            footer: Style::new().fg(Color::DarkGray),
            search_match: Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            intraline: Style::new()
                .add_modifier(Modifier::UNDERLINED)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Returns a legible theme with no foreground or background colors.
    pub const fn plain() -> Self {
        Self {
            syntax_colors: false,
            title: Style::new().add_modifier(Modifier::BOLD),
            selected: Style::new().add_modifier(Modifier::REVERSED),
            divider: Style::new(),
            file: Style::new().add_modifier(Modifier::BOLD),
            hunk: Style::new(),
            addition: Style::new(),
            deletion: Style::new(),
            context: Style::new(),
            marker: Style::new(),
            error: Style::new().add_modifier(Modifier::BOLD),
            footer: Style::new(),
            search_match: Style::new().add_modifier(Modifier::REVERSED),
            intraline: Style::new().add_modifier(Modifier::UNDERLINED),
        }
    }

    /// Returns a conservative ANSI-color theme for limited terminals.
    pub const fn low_color() -> Self {
        Self {
            syntax_colors: false,
            title: Style::new()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
            selected: Style::new()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
            divider: Style::new(),
            file: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::Blue),
            addition: Style::new().fg(Color::Green),
            deletion: Style::new().fg(Color::Red),
            context: Style::new(),
            marker: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            footer: Style::new(),
            search_match: Style::new().add_modifier(Modifier::REVERSED),
            intraline: Style::new().add_modifier(Modifier::UNDERLINED),
        }
    }

    /// Adds syntax color to a diff style when the active theme supports it.
    pub fn syntax(self, scope: usize, base: Style) -> Style {
        if !self.syntax_colors {
            return base;
        }
        let name = inkjet::constants::HIGHLIGHT_NAMES.get(scope).copied().unwrap_or("");
        if name.starts_with("comment") {
            base.fg(Color::DarkGray)
        } else if name.starts_with("string") || name.starts_with("markup") {
            base.fg(Color::Cyan)
        } else if name.starts_with("constant") || name.starts_with("number") {
            base.fg(Color::Magenta)
        } else if name.starts_with("keyword") || name.starts_with("type") {
            base.fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if name.starts_with("function") {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_themes_distinguish_diff_roles_and_plain_output_uses_no_colors() {
        for theme in [Theme::dark(), Theme::light(), Theme::low_color()] {
            assert_ne!(theme.addition, theme.deletion);
            assert_ne!(theme.file, theme.error);
        }
        let plain = Theme::plain();
        for style in [
            plain.title,
            plain.selected,
            plain.divider,
            plain.file,
            plain.hunk,
            plain.addition,
            plain.deletion,
            plain.context,
            plain.marker,
            plain.error,
            plain.footer,
            plain.search_match,
            plain.intraline,
        ] {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
        }
    }
}
