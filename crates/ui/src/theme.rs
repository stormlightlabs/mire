use std::fmt;
use std::str::FromStr;

use mire_core::NoteSeverity;
use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

pub use web::web_stylesheet;

#[cfg(test)]
pub mod contrast;
mod web;
#[cfg(test)]
pub mod xterm256;

const ICEBERG_DARK: Palette = Palette {
    accent: rgb(132, 160, 198),
    background: rgb(22, 24, 33),
    raised: rgb(15, 17, 23),
    header: rgb(17, 20, 66),
    selection: rgb(42, 47, 70),
    annotation: rgb(8, 9, 13),
    border: rgb(95, 103, 143),
    overlay1: rgb(132, 137, 160),
    text: rgb(198, 200, 209),
    subtext0: rgb(152, 156, 172),
    mauve: rgb(160, 147, 199),
    pink: rgb(242, 101, 181),
    green: rgb(180, 190, 130),
    added: rgb(180, 190, 130),
    addition_bg: rgb(46, 49, 48),
    addition_emphasis: rgb(73, 77, 64),
    yellow: rgb(226, 164, 120),
    red: rgb(226, 120, 120),
    deletion_bg: rgb(50, 35, 43),
    deletion_emphasis: rgb(88, 54, 61),
    blue: rgb(132, 160, 198),
    teal: rgb(137, 184, 194),
    peach: rgb(226, 164, 120),
    warning: rgb(226, 164, 120),
};

const NORD_LIGHT: Palette = Palette {
    accent: rgb(46, 90, 145),
    background: rgb(236, 239, 244),
    raised: rgb(229, 233, 240),
    header: rgb(216, 222, 233),
    selection: rgb(209, 218, 230),
    annotation: rgb(248, 249, 251),
    border: rgb(108, 120, 142),
    overlay1: rgb(96, 114, 140),
    text: rgb(46, 52, 64),
    subtext0: rgb(76, 86, 106),
    mauve: rgb(151, 54, 91),
    pink: rgb(153, 50, 75),
    green: rgb(79, 137, 76),
    added: rgb(56, 100, 53),
    addition_bg: rgb(218, 240, 221),
    addition_emphasis: rgb(188, 210, 182),
    yellow: rgb(235, 203, 139),
    red: rgb(153, 50, 75),
    deletion_bg: rgb(239, 217, 220),
    deletion_emphasis: rgb(218, 182, 192),
    blue: rgb(59, 110, 168),
    teal: rgb(57, 142, 172),
    peach: rgb(172, 68, 38),
    warning: rgb(120, 82, 14),
};

const ELDRITCH_MINIMAL: Palette = Palette {
    accent: rgb(55, 244, 153),
    background: rgb(23, 25, 40),
    raised: rgb(23, 25, 40),
    header: rgb(20, 23, 70),
    selection: rgb(44, 46, 69),
    annotation: rgb(14, 15, 24),
    border: rgb(96, 106, 150),
    overlay1: rgb(122, 137, 204),
    text: rgb(235, 250, 250),
    subtext0: rgb(171, 180, 218),
    mauve: rgb(164, 140, 242),
    pink: rgb(242, 101, 181),
    green: rgb(55, 244, 153),
    added: rgb(55, 244, 153),
    addition_bg: rgb(23, 53, 52),
    addition_emphasis: rgb(35, 104, 81),
    yellow: rgb(241, 252, 121),
    red: rgb(241, 108, 117),
    deletion_bg: rgb(56, 37, 52),
    deletion_emphasis: rgb(102, 55, 68),
    blue: rgb(4, 209, 249),
    teal: rgb(4, 209, 249),
    peach: rgb(247, 198, 127),
    warning: rgb(247, 198, 127),
};

const ELDRITCH_DUSK: Palette = Palette {
    accent: rgb(98, 70, 190),
    background: rgb(240, 243, 244),
    raised: rgb(226, 230, 232),
    header: rgb(213, 217, 219),
    selection: rgb(219, 218, 239),
    annotation: rgb(250, 251, 251),
    border: rgb(118, 122, 134),
    overlay1: rgb(91, 115, 220),
    text: rgb(30, 32, 41),
    subtext0: rgb(75, 86, 140),
    mauve: rgb(98, 70, 190),
    pink: rgb(251, 91, 182),
    green: rgb(56, 255, 159),
    added: rgb(16, 108, 64),
    addition_bg: rgb(212, 245, 230),
    addition_emphasis: rgb(163, 248, 208),
    yellow: rgb(255, 249, 82),
    red: rgb(160, 45, 55),
    deletion_bg: rgb(242, 220, 222),
    deletion_emphasis: rgb(223, 187, 190),
    blue: rgb(10, 214, 255),
    teal: rgb(10, 214, 255),
    peach: rgb(255, 175, 77),
    warning: rgb(138, 76, 0),
};

const CATPPUCCIN_MOCHA: Palette = Palette {
    accent: rgb(203, 166, 247),
    background: rgb(30, 30, 46),
    raised: rgb(24, 24, 37),
    header: rgb(46, 47, 65),
    selection: rgb(58, 59, 80),
    annotation: rgb(17, 17, 27),
    border: rgb(108, 112, 134),
    overlay1: rgb(137, 142, 166),
    text: rgb(205, 214, 244),
    subtext0: rgb(166, 173, 200),
    mauve: rgb(203, 166, 247),
    pink: rgb(245, 194, 231),
    green: rgb(166, 227, 161),
    added: rgb(166, 227, 161),
    addition_bg: rgb(50, 60, 60),
    addition_emphasis: rgb(73, 93, 83),
    yellow: rgb(249, 226, 175),
    red: rgb(243, 139, 168),
    deletion_bg: rgb(77, 31, 80),
    deletion_emphasis: rgb(98, 65, 85),
    blue: rgb(137, 180, 250),
    teal: rgb(148, 226, 213),
    peach: rgb(250, 179, 135),
    warning: rgb(250, 179, 135),
};

const CATPPUCCIN_LATTE: Palette = Palette {
    accent: rgb(114, 40, 210),
    background: rgb(239, 241, 245),
    raised: rgb(230, 233, 239),
    header: rgb(220, 224, 232),
    selection: rgb(222, 213, 240),
    annotation: rgb(249, 250, 252),
    border: rgb(110, 113, 135),
    overlay1: rgb(140, 143, 161),
    text: rgb(76, 79, 105),
    subtext0: rgb(86, 89, 112),
    mauve: rgb(136, 57, 239),
    pink: rgb(234, 118, 203),
    green: rgb(64, 160, 43),
    added: rgb(40, 105, 26),
    addition_bg: rgb(206, 235, 208),
    addition_emphasis: rgb(166, 207, 160),
    yellow: rgb(240, 196, 120),
    red: rgb(186, 12, 50),
    deletion_bg: rgb(237, 209, 219),
    deletion_emphasis: rgb(232, 186, 199),
    blue: rgb(24, 88, 215),
    teal: rgb(23, 146, 153),
    peach: rgb(254, 100, 11),
    warning: rgb(156, 68, 0),
};

/// A stable built-in theme identifier selected by users.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemeFamily {
    /// Select Nord Light on light backgrounds and Eldritch Minimal on dark ones.
    #[default]
    Auto,
    /// Select Iceberg Dark or Nord Light.
    Iceberg,
    /// Select Eldritch Minimal or Eldritch Dusk.
    Eldritch,
    /// Select Catppuccin Mocha or Catppuccin Latte.
    Catppuccin,
}

/// The terminal background mode used to choose a palette variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    /// A terminal with a dark background.
    Dark,
    /// A terminal with a light background.
    Light,
}

/// The concrete built-in palette selected after resolving a family and color mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeVariant {
    /// Iceberg's dark palette.
    IcebergDark,
    /// The Nord Light Base16 palette.
    NordLight,
    /// Eldritch's minimal dark palette.
    EldritchMinimal,
    /// Eldritch's Dusk light palette.
    EldritchDusk,
    /// Catppuccin's Mocha palette.
    CatppuccinMocha,
    /// Catppuccin's Latte palette.
    CatppuccinLatte,
}

/// Error returned when a built-in theme identifier is not recognized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseThemeFamilyError {
    value: String,
}

/// Semantic styles used by review rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    variant: Option<ThemeVariant>,
    syntax_colors: bool,
    syntax: SyntaxStyles,
    severity: SeverityStyles,
    /// Base style for the complete terminal surface and review pane.
    pub application: Style,
    /// Style for navigation and help panels.
    pub panel: Style,
    /// Style for active labels, paths, and selection indicators.
    pub accent: Style,
    /// Style for secondary labels and compact metadata.
    pub muted: Style,
    /// Style for the application title bar.
    pub title: Style,
    /// Style for the selected navigation item.
    pub selected: Style,
    /// Style for dividers between review panes.
    pub divider: Style,
    /// Style for file headers and navigation headings.
    pub file: Style,
    /// Style for hunk headers.
    pub hunk: Style,
    /// Style for added source rows.
    pub addition: Style,
    /// Foreground-only style for compact addition counts.
    pub addition_meta: Style,
    /// Style for removed source rows.
    pub deletion: Style,
    /// Foreground-only style for compact deletion counts.
    pub deletion_meta: Style,
    /// Style for unchanged source rows.
    pub context: Style,
    /// Style for binary, context-gap, and missing-newline markers.
    pub marker: Style,
    /// Style for user-facing errors.
    pub error: Style,
    /// Style for the keyboard-help footer.
    pub footer: Style,
    /// Style for matching search text.
    pub search_match: Style,
    /// Style for changed text within an added row.
    pub addition_emphasis: Style,
    /// Style for changed text within a removed row.
    pub deletion_emphasis: Style,
    /// Style for a finding drawn between source rows, on a surface no source row uses.
    pub finding: Style,
}

/// One built-in palette's colors, named by the role each plays.
///
/// The four surfaces step from `background` (the diff) through `raised` (the
/// sidebar, title bar, footer and dialogs) and `header` (file and help
/// headings) to `selection`, an accent-tinted step for the selected row.
/// `annotation` sets a finding apart from the source rows around it, and
/// `warning` marks medium severity in a color that reads as text on every
/// surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Palette {
    accent: Color,
    background: Color,
    raised: Color,
    header: Color,
    selection: Color,
    annotation: Color,
    border: Color,
    overlay1: Color,
    text: Color,
    subtext0: Color,
    mauve: Color,
    pink: Color,
    green: Color,
    added: Color,
    addition_bg: Color,
    addition_emphasis: Color,
    yellow: Color,
    red: Color,
    deletion_bg: Color,
    deletion_emphasis: Color,
    blue: Color,
    teal: Color,
    peach: Color,
    warning: Color,
}

/// Foreground styles for each finding severity. The severity's word always
/// accompanies the color.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SeverityStyles {
    note: Style,
    low: Style,
    medium: Style,
    high: Style,
    critical: Style,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SyntaxStyles {
    comment: Color,
    string: Color,
    constant: Color,
    keyword: Color,
    function: Color,
    r#type: Color,
}

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::Rgb(red, green, blue)
}

impl ThemeFamily {
    /// Every stable identifier accepted by the command line and future configuration layers.
    pub const ALL: [Self; 4] = [Self::Auto, Self::Iceberg, Self::Eldritch, Self::Catppuccin];

    /// Returns the stable lowercase identifier for this family.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Iceberg => "iceberg",
            Self::Eldritch => "eldritch",
            Self::Catppuccin => "catppuccin",
        }
    }
}

impl fmt::Display for ThemeFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ThemeFamily {
    type Err = ParseThemeFamilyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|family| family.as_str() == value)
            .ok_or_else(|| ParseThemeFamilyError { value: value.to_owned() })
    }
}

impl fmt::Display for ParseThemeFamilyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown theme family {:?}", self.value)
    }
}

impl std::error::Error for ParseThemeFamilyError {}

impl Theme {
    /// Detects a terminal's safety and background mode for the requested family.
    ///
    /// `NO_COLOR` disables all colors, while `TERM=dumb` selects a conservative
    /// ANSI theme. A failed color-mode query uses the family's dark variant.
    pub fn detect(family: ThemeFamily) -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::plain();
        }
        if std::env::var_os("TERM").is_some_and(|value| value == "dumb") {
            return Self::low_color();
        }
        let mode = theme_mode(QueryOptions::default()).ok().map(|mode| match mode {
            ThemeMode::Dark => ColorMode::Dark,
            ThemeMode::Light => ColorMode::Light,
        });
        Self::resolve(family, mode)
    }

    /// Resolves a family with an explicit mode, using its dark variant when mode detection failed.
    pub const fn resolve(family: ThemeFamily, mode: Option<ColorMode>) -> Self {
        let mode = match mode {
            Some(mode) => mode,
            None => ColorMode::Dark,
        };
        Self::from_palette(ThemeVariant::resolve(family, mode))
    }

    /// Returns the concrete RGB palette variant, or `None` for terminal-safety themes.
    pub const fn variant(self) -> Option<ThemeVariant> {
        self.variant
    }

    /// Returns deterministic Eldritch Minimal colors for dark terminal backgrounds.
    pub const fn dark() -> Self {
        Self::from_palette(ThemeVariant::EldritchMinimal)
    }

    /// Returns deterministic Eldritch Dusk colors for light terminal backgrounds.
    pub const fn light() -> Self {
        Self::from_palette(ThemeVariant::EldritchDusk)
    }

    /// Returns a legible theme with no foreground or background colors.
    pub const fn plain() -> Self {
        Self {
            variant: None,
            syntax_colors: false,
            syntax: SyntaxStyles::plain(),
            severity: SeverityStyles {
                note: Style::new(),
                low: Style::new(),
                medium: Style::new(),
                high: Style::new().add_modifier(Modifier::BOLD),
                critical: Style::new().add_modifier(Modifier::BOLD),
            },
            application: Style::new(),
            panel: Style::new(),
            accent: Style::new().add_modifier(Modifier::BOLD),
            muted: Style::new().add_modifier(Modifier::DIM),
            title: Style::new(),
            selected: Style::new().add_modifier(Modifier::REVERSED),
            divider: Style::new(),
            file: Style::new().add_modifier(Modifier::BOLD),
            hunk: Style::new(),
            addition: Style::new(),
            addition_meta: Style::new().add_modifier(Modifier::BOLD),
            deletion: Style::new(),
            deletion_meta: Style::new().add_modifier(Modifier::ITALIC),
            context: Style::new(),
            marker: Style::new(),
            error: Style::new().add_modifier(Modifier::BOLD),
            footer: Style::new(),
            search_match: Style::new().add_modifier(Modifier::REVERSED),
            addition_emphasis: Style::new().add_modifier(Modifier::BOLD),
            deletion_emphasis: Style::new().add_modifier(Modifier::BOLD),
            finding: Style::new().add_modifier(Modifier::ITALIC),
        }
    }

    /// Returns a conservative ANSI-color theme for limited terminals.
    pub const fn low_color() -> Self {
        Self {
            variant: None,
            syntax_colors: false,
            syntax: SyntaxStyles::plain(),
            severity: SeverityStyles {
                note: Style::new(),
                low: Style::new().fg(Color::Cyan),
                medium: Style::new().fg(Color::Yellow),
                high: Style::new().fg(Color::Red),
                critical: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            },
            application: Style::new(),
            panel: Style::new(),
            accent: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            muted: Style::new().add_modifier(Modifier::DIM),
            title: Style::new().add_modifier(Modifier::REVERSED),
            selected: Style::new()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
            divider: Style::new(),
            file: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            hunk: Style::new().fg(Color::Blue),
            addition: Style::new().fg(Color::Green),
            addition_meta: Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            deletion: Style::new().fg(Color::Red),
            deletion_meta: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            context: Style::new(),
            marker: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            footer: Style::new(),
            search_match: Style::new().add_modifier(Modifier::REVERSED),
            addition_emphasis: Style::new().add_modifier(Modifier::BOLD),
            deletion_emphasis: Style::new().add_modifier(Modifier::BOLD),
            finding: Style::new().add_modifier(Modifier::ITALIC),
        }
    }

    /// Returns the foreground style that marks a finding's severity.
    ///
    /// The style colors a mark and the severity's word; callers always draw
    /// the word, so severity never rests on color alone.
    pub const fn severity(self, severity: NoteSeverity) -> Style {
        match severity {
            NoteSeverity::Note => self.severity.note,
            NoteSeverity::Low => self.severity.low,
            NoteSeverity::Medium => self.severity.medium,
            NoteSeverity::High => self.severity.high,
            NoteSeverity::Critical => self.severity.critical,
        }
    }

    /// Adds syntax color to a diff style when the active theme supports it.
    pub fn syntax(self, scope: usize, base: Style) -> Style {
        if !self.syntax_colors || base == self.addition || base == self.deletion {
            return base;
        }
        let name = inkjet::constants::HIGHLIGHT_NAMES.get(scope).copied().unwrap_or("");
        if name.starts_with("comment") {
            base.fg(self.syntax.comment)
        } else if name.starts_with("string") || name.starts_with("markup") {
            base.fg(self.syntax.string)
        } else if name.starts_with("constant") || name.starts_with("number") {
            base.fg(self.syntax.constant)
        } else if name.starts_with("keyword") {
            base.fg(self.syntax.keyword).add_modifier(Modifier::BOLD)
        } else if name.starts_with("type") {
            base.fg(self.syntax.r#type).add_modifier(Modifier::BOLD)
        } else if name.starts_with("function") {
            base.fg(self.syntax.function).add_modifier(Modifier::BOLD)
        } else {
            base
        }
    }

    /// Returns the style for changed text within a row drawn in `base`.
    ///
    /// Added and removed rows get their own emphasis; any other row gets none.
    pub fn emphasis(self, base: Style) -> Style {
        if base == self.addition {
            self.addition_emphasis
        } else if base == self.deletion {
            self.deletion_emphasis
        } else {
            Style::new()
        }
    }

    /// Maps a variant's palette to the semantic roles both the terminal and the browser use.
    const fn from_palette(variant: ThemeVariant) -> Self {
        let palette = variant.palette();
        let light_variant = variant.is_light();
        let syntax = if light_variant {
            SyntaxStyles {
                comment: palette.text,
                string: if matches!(variant, ThemeVariant::EldritchDusk) { palette.mauve } else { palette.blue },
                constant: palette.mauve,
                keyword: palette.mauve,
                function: if matches!(variant, ThemeVariant::EldritchDusk) { palette.mauve } else { palette.red },
                r#type: if matches!(variant, ThemeVariant::EldritchDusk) { palette.mauve } else { palette.blue },
            }
        } else {
            SyntaxStyles {
                comment: palette.overlay1,
                string: palette.teal,
                constant: palette.mauve,
                keyword: palette.yellow,
                function: palette.pink,
                r#type: palette.blue,
            }
        };
        Self {
            variant: Some(variant),
            syntax_colors: true,
            syntax,
            severity: SeverityStyles {
                note: Style::new().fg(palette.subtext0),
                low: Style::new().fg(if light_variant { palette.accent } else { palette.blue }),
                medium: Style::new().fg(palette.warning),
                high: Style::new().fg(palette.red),
                critical: Style::new().fg(palette.red).add_modifier(Modifier::BOLD),
            },
            application: Style::new().fg(palette.text).bg(palette.background),
            panel: Style::new().fg(palette.text).bg(palette.raised),
            accent: Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
            muted: Style::new().fg(palette.subtext0),
            title: Style::new().fg(palette.text).bg(palette.raised),
            selected: Style::new()
                .fg(palette.text)
                .bg(palette.selection)
                .add_modifier(Modifier::BOLD),
            divider: Style::new().fg(palette.border),
            file: Style::new()
                .fg(palette.text)
                .bg(palette.header)
                .add_modifier(Modifier::BOLD),
            hunk: Style::new()
                .fg(if light_variant { palette.accent } else { palette.blue })
                .bg(palette.raised),
            addition: Style::new().fg(palette.text).bg(palette.addition_bg),
            addition_meta: Style::new().fg(palette.added).add_modifier(Modifier::BOLD),
            deletion: Style::new().fg(palette.text).bg(palette.deletion_bg),
            deletion_meta: Style::new().fg(palette.red).add_modifier(Modifier::BOLD),
            context: Style::new().fg(palette.text).bg(palette.background),
            marker: if light_variant {
                Style::new()
                    .fg(palette.subtext0)
                    .bg(palette.background)
                    .add_modifier(Modifier::ITALIC)
            } else {
                Style::new().fg(palette.peach).bg(palette.background)
            },
            error: Style::new()
                .fg(palette.raised)
                .bg(palette.red)
                .add_modifier(Modifier::BOLD),
            footer: Style::new().fg(palette.subtext0).bg(palette.raised),
            search_match: Style::new()
                .fg(if light_variant { palette.text } else { palette.raised })
                .bg(palette.yellow)
                .add_modifier(Modifier::BOLD),
            addition_emphasis: Style::new()
                .fg(palette.text)
                .bg(palette.addition_emphasis)
                .add_modifier(Modifier::BOLD),
            deletion_emphasis: Style::new()
                .fg(palette.text)
                .bg(palette.deletion_emphasis)
                .add_modifier(Modifier::BOLD),
            finding: Style::new().fg(palette.text).bg(palette.annotation),
        }
    }
}

impl ThemeVariant {
    /// Every built-in palette, dark and light variants of each family in turn.
    const ALL: [Self; 6] = [
        Self::IcebergDark,
        Self::NordLight,
        Self::EldritchMinimal,
        Self::EldritchDusk,
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
    ];

    /// Picks the palette for a family in a color mode, in the terminal and the browser alike.
    ///
    /// `auto` uses Nord Light on light backgrounds and Eldritch Minimal on dark ones.
    const fn resolve(family: ThemeFamily, mode: ColorMode) -> Self {
        match (family, mode) {
            (ThemeFamily::Iceberg, ColorMode::Dark) => Self::IcebergDark,
            (ThemeFamily::Auto | ThemeFamily::Iceberg, ColorMode::Light) => Self::NordLight,
            (ThemeFamily::Auto | ThemeFamily::Eldritch, ColorMode::Dark) => Self::EldritchMinimal,
            (ThemeFamily::Eldritch, ColorMode::Light) => Self::EldritchDusk,
            (ThemeFamily::Catppuccin, ColorMode::Dark) => Self::CatppuccinMocha,
            (ThemeFamily::Catppuccin, ColorMode::Light) => Self::CatppuccinLatte,
        }
    }

    /// Returns the stable kebab-case name of this palette.
    const fn as_str(self) -> &'static str {
        match self {
            Self::IcebergDark => "iceberg-dark",
            Self::NordLight => "nord-light",
            Self::EldritchMinimal => "eldritch-minimal",
            Self::EldritchDusk => "eldritch-dusk",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::CatppuccinLatte => "catppuccin-latte",
        }
    }

    /// Reports whether this palette is meant for a light background.
    const fn is_light(self) -> bool {
        matches!(self, Self::NordLight | Self::EldritchDusk | Self::CatppuccinLatte)
    }

    const fn palette(self) -> Palette {
        match self {
            Self::IcebergDark => ICEBERG_DARK,
            Self::NordLight => NORD_LIGHT,
            Self::EldritchMinimal => ELDRITCH_MINIMAL,
            Self::EldritchDusk => ELDRITCH_DUSK,
            Self::CatppuccinMocha => CATPPUCCIN_MOCHA,
            Self::CatppuccinLatte => CATPPUCCIN_LATTE,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl SyntaxStyles {
    const fn plain() -> Self {
        Self {
            comment: Color::Reset,
            string: Color::Reset,
            constant: Color::Reset,
            keyword: Color::Reset,
            function: Color::Reset,
            r#type: Color::Reset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEVERITIES: [NoteSeverity; 5] = [
        NoteSeverity::Note,
        NoteSeverity::Low,
        NoteSeverity::Medium,
        NoteSeverity::High,
        NoteSeverity::Critical,
    ];

    #[test]
    fn stable_identifiers_round_trip_and_unknown_identifiers_fail() {
        assert_eq!(
            ThemeFamily::ALL.map(ThemeFamily::as_str),
            ["auto", "iceberg", "eldritch", "catppuccin"]
        );
        for family in ThemeFamily::ALL {
            assert_eq!(family.as_str().parse::<ThemeFamily>().unwrap(), family);
        }
        assert!("nord".parse::<ThemeFamily>().is_err());
    }

    #[test]
    fn every_family_and_mode_resolves_to_the_expected_variant() {
        let cases = [
            (ThemeFamily::Iceberg, ColorMode::Dark, ThemeVariant::IcebergDark),
            (ThemeFamily::Iceberg, ColorMode::Light, ThemeVariant::NordLight),
            (ThemeFamily::Eldritch, ColorMode::Dark, ThemeVariant::EldritchMinimal),
            (ThemeFamily::Eldritch, ColorMode::Light, ThemeVariant::EldritchDusk),
            (ThemeFamily::Catppuccin, ColorMode::Dark, ThemeVariant::CatppuccinMocha),
            (ThemeFamily::Catppuccin, ColorMode::Light, ThemeVariant::CatppuccinLatte),
            (ThemeFamily::Auto, ColorMode::Dark, ThemeVariant::EldritchMinimal),
            (ThemeFamily::Auto, ColorMode::Light, ThemeVariant::NordLight),
        ];
        for (family, mode, expected) in cases {
            assert_eq!(Theme::resolve(family, Some(mode)).variant(), Some(expected));
        }
        for family in ThemeFamily::ALL {
            assert_eq!(
                Theme::resolve(family, None).variant(),
                Theme::resolve(family, Some(ColorMode::Dark)).variant()
            );
        }
    }

    #[test]
    fn palette_sources_map_to_exact_semantic_colors() {
        let cases = [
            (ICEBERG_DARK, rgb(22, 24, 33), rgb(180, 190, 130), rgb(226, 120, 120)),
            (NORD_LIGHT, rgb(236, 239, 244), rgb(79, 137, 76), rgb(153, 50, 75)),
            (ELDRITCH_MINIMAL, rgb(23, 25, 40), rgb(55, 244, 153), rgb(241, 108, 117)),
            (ELDRITCH_DUSK, rgb(240, 243, 244), rgb(56, 255, 159), rgb(160, 45, 55)),
            (
                CATPPUCCIN_MOCHA,
                rgb(30, 30, 46),
                rgb(166, 227, 161),
                rgb(243, 139, 168),
            ),
            (CATPPUCCIN_LATTE, rgb(239, 241, 245), rgb(64, 160, 43), rgb(186, 12, 50)),
        ];
        for (palette, background, addition, deletion) in cases {
            assert_eq!(palette.background, background);
            assert_eq!(palette.green, addition);
            assert_eq!(palette.red, deletion);
        }
    }

    #[test]
    fn color_themes_cover_roles_and_plain_output_uses_no_colors() {
        for family in [ThemeFamily::Iceberg, ThemeFamily::Eldritch, ThemeFamily::Catppuccin] {
            for mode in [ColorMode::Dark, ColorMode::Light] {
                let theme = Theme::resolve(family, Some(mode));
                for style in semantic_styles(theme) {
                    assert!(style.fg.is_some() || style.bg.is_some());
                }
                assert_ne!(theme.addition, theme.deletion);
                assert_ne!(theme.file, theme.error);
            }
        }

        for style in semantic_styles(Theme::plain()) {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
        }
    }

    #[test]
    fn controlled_rgb_pairs_have_readable_contrast() {
        for family in [ThemeFamily::Iceberg, ThemeFamily::Eldritch, ThemeFamily::Catppuccin] {
            for mode in [ColorMode::Dark, ColorMode::Light] {
                let theme = Theme::resolve(family, Some(mode));
                for style in [
                    theme.application,
                    theme.panel,
                    theme.title,
                    theme.selected,
                    theme.file,
                    theme.hunk,
                    theme.addition,
                    theme.deletion,
                    theme.context,
                    theme.marker,
                    theme.error,
                    theme.footer,
                    theme.search_match,
                    theme.finding,
                ] {
                    assert!(contrast(style) >= 3.0, "{family}/{mode:?} has low contrast: {style:?}");
                }
                for style in [theme.accent, theme.muted, theme.addition_meta, theme.deletion_meta] {
                    let style = style.bg(theme.panel.bg.expect("color panel has a background"));
                    assert!(
                        contrast(style) >= 3.0,
                        "{family}/{mode:?} has low metadata contrast: {style:?}"
                    );
                }
                for foreground in [
                    theme.syntax.comment,
                    theme.syntax.string,
                    theme.syntax.constant,
                    theme.syntax.keyword,
                    theme.syntax.function,
                    theme.syntax.r#type,
                ] {
                    let style = Style::new().fg(foreground).bg(theme.context.bg.unwrap());
                    assert!(
                        contrast(style) >= 3.0,
                        "{family}/{mode:?} has low syntax contrast: {style:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn text_muted_and_accent_reach_aa_contrast_on_their_backgrounds() {
        for variant in ThemeVariant::ALL {
            let theme = Theme::from_palette(variant);
            for (background_role, background) in [("application", theme.application), ("panel", theme.panel)] {
                for (foreground_role, foreground) in [
                    ("text", theme.application),
                    ("muted", theme.muted),
                    ("accent", theme.accent),
                ] {
                    let style = Style::new()
                        .fg(foreground.fg.expect("palette roles set a foreground"))
                        .bg(background.bg.expect("palette roles set a background"));
                    let ratio = contrast(style);
                    assert!(
                        ratio >= 4.5,
                        "{}: {foreground_role} on {background_role} is {ratio:.2}:1",
                        variant.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn surfaces_stay_distinct_after_xterm_256_rounding() {
        for variant in ThemeVariant::ALL {
            let palette = variant.palette();
            let index = |color| xterm256::xterm256_index(color);
            let name = variant.as_str();
            let (background, raised, header, selection) = (
                index(palette.background),
                index(palette.raised),
                index(palette.header),
                index(palette.selection),
            );
            let (addition, deletion) = (index(palette.addition_bg), index(palette.deletion_bg));
            assert_ne!(header, background, "{name}: file heading on the diff");
            assert_ne!(selection, background, "{name}: selected row on the diff");
            assert_ne!(selection, raised, "{name}: selected row in the sidebar");
            assert_ne!(selection, header, "{name}: selected file heading");
            assert_ne!(addition, deletion, "{name}: added and removed rows");
            let annotation = index(palette.annotation);
            for (surface, neighbour) in [
                ("diff", background),
                ("hunk header", raised),
                ("file heading", header),
                ("selection", selection),
                ("added row", addition),
                ("removed row", deletion),
            ] {
                assert_ne!(annotation, neighbour, "{name}: finding line matches the {surface}");
            }
            for (role, row) in [("added", addition), ("removed", deletion)] {
                for (surface, neighbour) in [
                    ("diff", background),
                    ("hunk header", raised),
                    ("file heading", header),
                    ("selection", selection),
                ] {
                    assert_ne!(row, neighbour, "{name}: {role} row matches the {surface}");
                }
            }
            assert_ne!(
                index(palette.addition_emphasis),
                addition,
                "{name}: added emphasis on its row"
            );
            assert_ne!(
                index(palette.deletion_emphasis),
                deletion,
                "{name}: removed emphasis on its row"
            );
        }
    }

    #[test]
    fn severity_text_reaches_aa_contrast_wherever_it_is_drawn() {
        for variant in ThemeVariant::ALL {
            let theme = Theme::from_palette(variant);
            for severity in SEVERITIES {
                let foreground = theme
                    .severity(severity)
                    .fg
                    .expect("palette severities set a foreground");
                for (background_role, background) in [
                    ("application", theme.application),
                    ("panel", theme.panel),
                    ("selected", theme.selected),
                    ("finding", theme.finding),
                ] {
                    let style = Style::new()
                        .fg(foreground)
                        .bg(background.bg.expect("palette roles set a background"));
                    let ratio = contrast(style);
                    assert!(
                        ratio >= 4.5,
                        "{}: {severity} on {background_role} is {ratio:.2}:1",
                        variant.as_str()
                    );
                }
            }
            let muted = Style::new()
                .fg(theme.muted.fg.expect("palette roles set a foreground"))
                .bg(theme.finding.bg.expect("palette roles set a background"));
            assert!(contrast(muted) >= 4.5, "{}: muted on finding", variant.as_str());
        }
    }

    #[test]
    fn severities_keep_distinct_styles_in_color_and_plain_themes() {
        for theme in ThemeVariant::ALL
            .map(Theme::from_palette)
            .into_iter()
            .chain([Theme::low_color()])
        {
            for (index, severity) in SEVERITIES.into_iter().enumerate() {
                for other in SEVERITIES.into_iter().skip(index + 1) {
                    assert_ne!(
                        theme.severity(severity),
                        theme.severity(other),
                        "{severity} and {other}"
                    );
                }
            }
        }
        assert_eq!(Theme::plain().severity(NoteSeverity::Medium).fg, None);
    }

    fn semantic_styles(theme: Theme) -> [Style; 21] {
        [
            theme.application,
            theme.panel,
            theme.accent,
            theme.muted,
            theme.title,
            theme.selected,
            theme.divider,
            theme.file,
            theme.hunk,
            theme.addition,
            theme.addition_meta,
            theme.deletion,
            theme.deletion_meta,
            theme.context,
            theme.marker,
            theme.error,
            theme.footer,
            theme.search_match,
            theme.addition_emphasis,
            theme.deletion_emphasis,
            theme.finding,
        ]
    }

    fn contrast(style: Style) -> f64 {
        let (Some(foreground), Some(background)) = (style.fg, style.bg) else {
            panic!("contrast test requires controlled RGB pairs");
        };
        contrast::contrast_ratio(foreground, background)
    }
}
