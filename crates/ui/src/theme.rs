use std::fmt;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

const ICEBERG_DARK: Palette = Palette {
    accent: rgb(132, 160, 198),
    panel_bg: rgb(22, 24, 33),
    surface0: rgb(30, 33, 50),
    surface1: rgb(39, 44, 66),
    surface_dim: rgb(15, 17, 23),
    overlay0: rgb(68, 75, 113),
    overlay1: rgb(107, 112, 137),
    text: rgb(198, 200, 209),
    subtext0: rgb(129, 133, 150),
    mauve: rgb(160, 147, 199),
    pink: rgb(242, 101, 181),
    green: rgb(180, 190, 130),
    addition_bg: rgb(46, 49, 48),
    yellow: rgb(226, 164, 120),
    red: rgb(226, 120, 120),
    deletion_bg: rgb(53, 38, 46),
    blue: rgb(132, 160, 198),
    teal: rgb(137, 184, 194),
    peach: rgb(226, 164, 120),
};

const NORD_LIGHT: Palette = Palette {
    accent: rgb(59, 110, 168),
    panel_bg: rgb(229, 233, 240),
    surface0: rgb(194, 208, 231),
    surface1: rgb(184, 197, 219),
    surface_dim: rgb(229, 233, 240),
    overlay0: rgb(174, 186, 207),
    overlay1: rgb(96, 114, 140),
    text: rgb(46, 52, 64),
    subtext0: rgb(96, 114, 140),
    mauve: rgb(151, 54, 91),
    pink: rgb(153, 50, 75),
    green: rgb(79, 137, 76),
    addition_bg: rgb(206, 219, 215),
    yellow: rgb(154, 117, 0),
    red: rgb(153, 50, 75),
    deletion_bg: rgb(218, 206, 215),
    blue: rgb(59, 110, 168),
    teal: rgb(57, 142, 172),
    peach: rgb(172, 68, 38),
};

const ELDRITCH_MINIMAL: Palette = Palette {
    accent: rgb(55, 244, 153),
    panel_bg: rgb(23, 25, 40),
    surface0: rgb(33, 35, 55),
    surface1: rgb(41, 46, 66),
    surface_dim: rgb(23, 25, 40),
    overlay0: rgb(59, 66, 97),
    overlay1: rgb(100, 115, 183),
    text: rgb(235, 250, 250),
    subtext0: rgb(171, 180, 218),
    mauve: rgb(164, 140, 242),
    pink: rgb(242, 101, 181),
    green: rgb(55, 244, 153),
    addition_bg: rgb(28, 58, 57),
    yellow: rgb(241, 252, 121),
    red: rgb(241, 108, 117),
    deletion_bg: rgb(56, 37, 52),
    blue: rgb(4, 209, 249),
    teal: rgb(4, 209, 249),
    peach: rgb(247, 198, 127),
};

const ELDRITCH_DUSK: Palette = Palette {
    accent: rgb(56, 255, 159),
    panel_bg: rgb(240, 243, 244),
    surface0: rgb(226, 230, 232),
    surface1: rgb(213, 217, 219),
    surface_dim: rgb(240, 243, 244),
    overlay0: rgb(201, 203, 205),
    overlay1: rgb(91, 115, 220),
    text: rgb(30, 32, 41),
    subtext0: rgb(91, 115, 220),
    mauve: rgb(138, 105, 247),
    pink: rgb(251, 91, 182),
    green: rgb(56, 255, 159),
    addition_bg: rgb(212, 245, 230),
    yellow: rgb(255, 249, 82),
    red: rgb(251, 91, 102),
    deletion_bg: rgb(242, 220, 222),
    blue: rgb(10, 214, 255),
    teal: rgb(10, 214, 255),
    peach: rgb(255, 175, 77),
};

const CATPPUCCIN_MOCHA: Palette = Palette {
    accent: rgb(203, 166, 247),
    panel_bg: rgb(30, 30, 46),
    surface0: rgb(49, 50, 68),
    surface1: rgb(69, 71, 90),
    surface_dim: rgb(24, 24, 37),
    overlay0: rgb(108, 112, 134),
    overlay1: rgb(127, 132, 156),
    text: rgb(205, 214, 244),
    subtext0: rgb(166, 173, 200),
    mauve: rgb(203, 166, 247),
    pink: rgb(245, 194, 231),
    green: rgb(166, 227, 161),
    addition_bg: rgb(50, 60, 63),
    yellow: rgb(249, 226, 175),
    red: rgb(243, 139, 168),
    deletion_bg: rgb(62, 47, 64),
    blue: rgb(137, 180, 250),
    teal: rgb(148, 226, 213),
    peach: rgb(250, 179, 135),
};

const CATPPUCCIN_LATTE: Palette = Palette {
    accent: rgb(136, 57, 239),
    panel_bg: rgb(239, 241, 245),
    surface0: rgb(204, 208, 218),
    surface1: rgb(188, 192, 204),
    surface_dim: rgb(230, 233, 239),
    overlay0: rgb(156, 160, 176),
    overlay1: rgb(140, 143, 161),
    text: rgb(76, 79, 105),
    subtext0: rgb(108, 111, 133),
    mauve: rgb(136, 57, 239),
    pink: rgb(234, 118, 203),
    green: rgb(64, 160, 43),
    addition_bg: rgb(213, 229, 220),
    yellow: rgb(223, 142, 29),
    red: rgb(210, 15, 57),
    deletion_bg: rgb(235, 207, 219),
    blue: rgb(30, 102, 245),
    teal: rgb(23, 146, 153),
    peach: rgb(254, 100, 11),
};

/// A stable built-in theme identifier selected by users.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemeFamily {
    /// Select the Eldritch variant matching the terminal color mode.
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
    /// Style patched onto changed text within an added or removed row.
    pub intraline: Style,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Palette {
    accent: Color,
    panel_bg: Color,
    surface0: Color,
    surface1: Color,
    surface_dim: Color,
    overlay0: Color,
    overlay1: Color,
    text: Color,
    subtext0: Color,
    mauve: Color,
    pink: Color,
    green: Color,
    addition_bg: Color,
    yellow: Color,
    red: Color,
    deletion_bg: Color,
    blue: Color,
    teal: Color,
    peach: Color,
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
        let variant = match (family, mode) {
            (ThemeFamily::Iceberg, ColorMode::Dark) => ThemeVariant::IcebergDark,
            (ThemeFamily::Iceberg, ColorMode::Light) => ThemeVariant::NordLight,
            (ThemeFamily::Auto | ThemeFamily::Eldritch, ColorMode::Dark) => ThemeVariant::EldritchMinimal,
            (ThemeFamily::Auto | ThemeFamily::Eldritch, ColorMode::Light) => ThemeVariant::EldritchDusk,
            (ThemeFamily::Catppuccin, ColorMode::Dark) => ThemeVariant::CatppuccinMocha,
            (ThemeFamily::Catppuccin, ColorMode::Light) => ThemeVariant::CatppuccinLatte,
        };
        Self::from_palette(variant)
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
            intraline: Style::new().add_modifier(Modifier::BOLD),
        }
    }

    /// Returns a conservative ANSI-color theme for limited terminals.
    pub const fn low_color() -> Self {
        Self {
            variant: None,
            syntax_colors: false,
            syntax: SyntaxStyles::plain(),
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
            intraline: Style::new().add_modifier(Modifier::BOLD),
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

    const fn from_palette(variant: ThemeVariant) -> Self {
        let palette = match variant {
            ThemeVariant::IcebergDark => ICEBERG_DARK,
            ThemeVariant::NordLight => NORD_LIGHT,
            ThemeVariant::EldritchMinimal => ELDRITCH_MINIMAL,
            ThemeVariant::EldritchDusk => ELDRITCH_DUSK,
            ThemeVariant::CatppuccinMocha => CATPPUCCIN_MOCHA,
            ThemeVariant::CatppuccinLatte => CATPPUCCIN_LATTE,
        };
        let emphasis_text = match variant {
            ThemeVariant::NordLight => palette.surface_dim,
            ThemeVariant::EldritchDusk | ThemeVariant::CatppuccinLatte => palette.text,
            ThemeVariant::IcebergDark | ThemeVariant::EldritchMinimal | ThemeVariant::CatppuccinMocha => {
                palette.surface_dim
            }
        };
        let danger_text = match variant {
            ThemeVariant::NordLight | ThemeVariant::CatppuccinLatte => palette.surface_dim,
            ThemeVariant::EldritchDusk => palette.text,
            ThemeVariant::IcebergDark | ThemeVariant::EldritchMinimal | ThemeVariant::CatppuccinMocha => {
                palette.surface_dim
            }
        };
        let light_variant = matches!(
            variant,
            ThemeVariant::NordLight | ThemeVariant::EldritchDusk | ThemeVariant::CatppuccinLatte
        );
        let interface_accent =
            if matches!(variant, ThemeVariant::EldritchDusk) { palette.mauve } else { palette.accent };
        let (addition_meta, deletion_meta) = if light_variant {
            (
                Style::new().fg(palette.text).add_modifier(Modifier::BOLD),
                Style::new().fg(palette.text).add_modifier(Modifier::ITALIC),
            )
        } else {
            (
                Style::new().fg(palette.green).add_modifier(Modifier::BOLD),
                Style::new().fg(palette.red).add_modifier(Modifier::BOLD),
            )
        };
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
            application: Style::new().fg(palette.text).bg(palette.panel_bg),
            panel: Style::new().fg(palette.text).bg(palette.surface_dim),
            accent: Style::new().fg(interface_accent).add_modifier(Modifier::BOLD),
            muted: Style::new().fg(palette.subtext0),
            title: Style::new().fg(palette.text).bg(palette.surface_dim),
            selected: Style::new()
                .fg(palette.text)
                .bg(palette.surface1)
                .add_modifier(Modifier::BOLD),
            divider: Style::new().fg(palette.overlay0).bg(palette.panel_bg),
            file: Style::new()
                .fg(palette.text)
                .bg(palette.surface0)
                .add_modifier(Modifier::BOLD),
            hunk: if light_variant {
                Style::new().fg(palette.text).bg(palette.surface0)
            } else {
                Style::new().fg(palette.blue).bg(palette.surface_dim)
            },
            addition: Style::new().fg(palette.text).bg(palette.addition_bg),
            addition_meta,
            deletion: Style::new().fg(palette.text).bg(palette.deletion_bg),
            deletion_meta,
            context: Style::new().fg(palette.text).bg(palette.panel_bg),
            marker: if light_variant {
                Style::new()
                    .fg(palette.text)
                    .bg(palette.surface0)
                    .add_modifier(Modifier::ITALIC)
            } else {
                Style::new().fg(palette.peach).bg(palette.panel_bg)
            },
            error: Style::new()
                .fg(danger_text)
                .bg(palette.red)
                .add_modifier(Modifier::BOLD),
            footer: Style::new().fg(palette.subtext0).bg(palette.surface_dim),
            search_match: Style::new()
                .fg(emphasis_text)
                .bg(palette.yellow)
                .add_modifier(Modifier::BOLD),
            intraline: Style::new().bg(palette.surface1).add_modifier(Modifier::BOLD),
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
            (ThemeFamily::Auto, ColorMode::Light, ThemeVariant::EldritchDusk),
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
            (NORD_LIGHT, rgb(229, 233, 240), rgb(79, 137, 76), rgb(153, 50, 75)),
            (ELDRITCH_MINIMAL, rgb(23, 25, 40), rgb(55, 244, 153), rgb(241, 108, 117)),
            (ELDRITCH_DUSK, rgb(240, 243, 244), rgb(56, 255, 159), rgb(251, 91, 102)),
            (
                CATPPUCCIN_MOCHA,
                rgb(30, 30, 46),
                rgb(166, 227, 161),
                rgb(243, 139, 168),
            ),
            (CATPPUCCIN_LATTE, rgb(239, 241, 245), rgb(64, 160, 43), rgb(210, 15, 57)),
        ];
        for (palette, background, addition, deletion) in cases {
            assert_eq!(palette.panel_bg, background);
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

    fn semantic_styles(theme: Theme) -> [Style; 19] {
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
            theme.intraline,
        ]
    }

    fn contrast(style: Style) -> f64 {
        let (
            Some(Color::Rgb(foreground_red, foreground_green, foreground_blue)),
            Some(Color::Rgb(background_red, background_green, background_blue)),
        ) = (style.fg, style.bg)
        else {
            panic!("contrast test requires controlled RGB pairs");
        };
        let foreground = luminance(foreground_red, foreground_green, foreground_blue);
        let background = luminance(background_red, background_green, background_blue);
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    fn luminance(red: u8, green: u8, blue: u8) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
    }
}
