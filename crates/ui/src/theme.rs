use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

/// Semantic styles used by review rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub title: Style,
    pub file: Style,
    pub hunk: Style,
    pub addition: Style,
    pub deletion: Style,
    pub context: Style,
    pub marker: Style,
    pub error: Style,
    pub footer: Style,
}

impl Theme {
    /// Detects a light or dark terminal and falls back to a dark theme.
    ///
    /// `NO_COLOR` disables colored diff rows and avoids querying the terminal.
    pub fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::plain();
        }
        match theme_mode(QueryOptions::default()).unwrap_or(ThemeMode::Dark) {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }

    /// Returns deterministic colors intended for dark terminal backgrounds.
    pub const fn dark() -> Self {
        Self {
            title: Style::new()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
            file: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::LightBlue),
            addition: Style::new().fg(Color::LightGreen),
            deletion: Style::new().fg(Color::LightRed),
            context: Style::new().fg(Color::Gray),
            marker: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            footer: Style::new().fg(Color::DarkGray),
        }
    }

    /// Returns deterministic colors intended for light terminal backgrounds.
    pub const fn light() -> Self {
        Self {
            title: Style::new()
                .fg(Color::White)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
            file: Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::Blue),
            addition: Style::new().fg(Color::Green),
            deletion: Style::new().fg(Color::Red),
            context: Style::new().fg(Color::DarkGray),
            marker: Style::new().fg(Color::Magenta),
            error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            footer: Style::new().fg(Color::DarkGray),
        }
    }

    /// Returns a legible theme with no foreground or background colors.
    pub const fn plain() -> Self {
        Self {
            title: Style::new().add_modifier(Modifier::BOLD),
            file: Style::new().add_modifier(Modifier::BOLD),
            hunk: Style::new(),
            addition: Style::new(),
            deletion: Style::new(),
            context: Style::new(),
            marker: Style::new(),
            error: Style::new().add_modifier(Modifier::BOLD),
            footer: Style::new(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
