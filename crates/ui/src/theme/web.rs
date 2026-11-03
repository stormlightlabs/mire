//! The browser review's stylesheet, built from the terminal palettes.
//!
//! `packages/app/src/themes.css` holds this output. A test fails when the two
//! differ; `just web-themes` rewrites the file.

use std::fmt::Write as _;

use mire_core::NoteSeverity;
use ratatui::style::{Color, Style};

use super::contrast::contrast_ratio;
use super::{ColorDepth, ColorMode, Theme, ThemeFamily, ThemeVariant};

/// The contrast WCAG AA asks of body text, which every syntax color reaches
/// on every diff row.
const AA_TEXT: f64 = 4.5;

/// How far, in percent, a dark palette's browser diff rows step from the plain
/// row toward the terminal's added and removed row colors.
///
/// The terminal draws changed rows without syntax colors, so a dark palette
/// lights them enough for plain text alone. The browser colors syntax on every
/// row, so it takes half that step and leaves room for the dimmer roles, such
/// as comments. Light palettes' row colors already leave that room.
const DARK_ROW_STEP: u8 = 50;

/// How far, in percent, a dark palette's changed-word background steps from
/// its row toward the change color, before [`word_background`] darkens it.
///
/// Darkening scales every channel alike, so the hue of this step survives it.
const DARK_WORD_STEP: u8 = 40;

/// The contrast a dark palette's changed-word background keeps from its row.
///
/// The syntax colors leave no room to lighten it, and a word only tinted
/// toward the change color is hard to tell from a grey row, so it darkens.
const DARK_WORD_SEPARATION: f64 = 1.2;

/// How far, in percent, a light palette's changed-word background steps from
/// its row toward the change color.
///
/// Lightening would wash this step back out to the row, so the syntax colors
/// darken to fit it instead.
const LIGHT_WORD_STEP: u8 = 10;

/// Builds CSS custom properties for every palette, selected by the
/// `data-theme-family` and `data-theme` attributes on the root element.
///
/// `data-theme-family` holds a [`ThemeFamily`] identifier and `data-theme`
/// holds `light` or `dark`. Each variant's properties come from the roles
/// [`Theme::from_palette`] assigns, so the browser and the terminal share them.
/// Browsers show every RGB color, so each variant uses its truecolor palette.
pub fn web_stylesheet() -> String {
    let mut css = String::from("/* Generated from crates/ui/src/theme.rs by `just web-themes`. Do not edit. */\n");
    for variant in ThemeVariant::ALL {
        css.push('\n');
        let _ = writeln!(css, "/* {} */", variant.as_str());
        css.push_str(&selectors(variant).join(",\n"));
        css.push_str(" {\n");
        let scheme = if variant.is_light() { "light" } else { "dark" };
        let _ = writeln!(css, "\tcolor-scheme: {scheme};");
        for (name, color) in properties(variant) {
            let _ = writeln!(css, "\t--{name}: {};", hex(color));
        }
        css.push_str("}\n");
    }
    css
}

/// The CSS custom properties the browser reads, paired with their colors.
///
/// The diff draws plain rows on `--surface`, and added and removed rows on
/// `--diff-added` and `--diff-removed`. Changed words within those rows take
/// `--diff-added-word` and `--diff-removed-word`, from [`word_background`].
/// The `--syntax-*` colors come from the terminal's syntax roles in
/// [`Theme::syntax`]; where a role falls short of 4.5:1 on one of those five
/// backgrounds, [`readable`] darkens or lightens it just enough.
fn properties(variant: ThemeVariant) -> [(&'static str, Color); 30] {
    let theme = Theme::from_palette(variant, ColorDepth::Truecolor);
    let fg = |style: Style| style.fg.expect("palette roles set a foreground");
    let bg = |style: Style| style.bg.expect("palette roles set a background");
    let light = variant.is_light();
    let row = |terminal_row: Style| {
        if light { bg(terminal_row) } else { mix(bg(theme.panel), bg(terminal_row), DARK_ROW_STEP) }
    };
    let rows = [bg(theme.panel), row(theme.addition), row(theme.deletion)];
    let roles = [
        theme.syntax.comment,
        theme.syntax.string,
        theme.syntax.constant,
        theme.syntax.keyword,
        theme.syntax.function,
        theme.syntax.r#type,
    ];
    let mut text = roles.map(|role| readable(role, light, &rows)).to_vec();
    text.push(fg(theme.application));
    let added_word = word_background(rows[1], fg(theme.addition_meta), light, &text);
    let removed_word = word_background(rows[2], bg(theme.error), light, &text);
    let backgrounds = [rows[0], rows[1], rows[2], added_word, removed_word];
    let syntax = |color: Color| readable(color, light, &backgrounds);
    [
        ("paper", bg(theme.application)),
        ("ink", fg(theme.application)),
        ("muted", fg(theme.muted)),
        ("focus", fg(theme.accent)),
        ("surface", bg(theme.panel)),
        ("paper-deep", bg(theme.file)),
        ("button-hover", bg(theme.selected)),
        ("line", fg(theme.divider)),
        ("line-strong", fg(theme.muted)),
        ("faint", fg(theme.divider)),
        ("selected", fg(theme.application)),
        ("selected-ink", bg(theme.application)),
        ("danger", bg(theme.error)),
        ("success", fg(theme.addition_meta)),
        ("annotation", bg(theme.finding)),
        ("severity-note", fg(theme.severity(NoteSeverity::Note))),
        ("severity-low", fg(theme.severity(NoteSeverity::Low))),
        ("severity-medium", fg(theme.severity(NoteSeverity::Medium))),
        ("severity-high", fg(theme.severity(NoteSeverity::High))),
        ("severity-critical", fg(theme.severity(NoteSeverity::Critical))),
        ("diff-added", rows[1]),
        ("diff-removed", rows[2]),
        ("diff-added-word", added_word),
        ("diff-removed-word", removed_word),
        ("syntax-comment", syntax(theme.syntax.comment)),
        ("syntax-string", syntax(theme.syntax.string)),
        ("syntax-constant", syntax(theme.syntax.constant)),
        ("syntax-keyword", syntax(theme.syntax.keyword)),
        ("syntax-function", syntax(theme.syntax.function)),
        ("syntax-type", syntax(theme.syntax.r#type)),
    ]
}

/// Returns `color`, darkened in 1% steps on a light palette or lightened on a
/// dark one, until it reaches 4.5:1 on every background. A color that already
/// does comes back unchanged.
fn readable(color: Color, light: bool, backgrounds: &[Color]) -> Color {
    let target = if light { Color::Rgb(0, 0, 0) } else { Color::Rgb(255, 255, 255) };
    (0..=100)
        .map(|percent| mix(color, target, percent))
        .find(|candidate| {
            backgrounds
                .iter()
                .all(|background| contrast_ratio(*candidate, *background) >= AA_TEXT)
        })
        .expect("black or white reaches AA text contrast on a palette's diff rows")
}

/// Returns the background for changed words on `row`: a step from `row`
/// toward `change`, the palette's added or removed color, so the words stand
/// out from the rest of the row.
///
/// On a dark palette the step then darkens in 1% steps until every color in
/// `text` reaches 4.5:1 on it and it reaches [`DARK_WORD_SEPARATION`] from
/// `row`. On a light palette the step stays as it is, and [`readable`] darkens
/// the syntax colors to fit it.
fn word_background(row: Color, change: Color, light: bool, text: &[Color]) -> Color {
    if light {
        return mix(row, change, LIGHT_WORD_STEP);
    }
    let tinted = mix(row, change, DARK_WORD_STEP);
    (0..=100)
        .map(|percent| mix(tinted, Color::Rgb(0, 0, 0), percent))
        .find(|candidate| {
            contrast_ratio(row, *candidate) >= DARK_WORD_SEPARATION
                && text
                    .iter()
                    .all(|foreground| contrast_ratio(*foreground, *candidate) >= AA_TEXT)
        })
        .expect("black reaches AA text contrast with a dark palette's text and parts from its rows")
}

/// Mixes `percent` of `to` into `from`, channel by channel, rounding to the
/// nearest value.
fn mix(from: Color, to: Color, percent: u8) -> Color {
    let (Color::Rgb(red, green, blue), Color::Rgb(to_red, to_green, to_blue)) = (from, to) else {
        unreachable!("palettes hold RGB colors, not {from:?} and {to:?}");
    };
    let channel = |from: u8, to: u8| {
        let mixed = (u16::from(from) * u16::from(100 - percent) + u16::from(to) * u16::from(percent) + 50) / 100;
        u8::try_from(mixed).expect("a mix of two channels stays a channel")
    };
    Color::Rgb(channel(red, to_red), channel(green, to_green), channel(blue, to_blue))
}

fn selectors(variant: ThemeVariant) -> Vec<String> {
    let mut selectors = Vec::new();
    if ThemeVariant::resolve(ThemeFamily::Auto, ColorMode::Light) == variant {
        selectors.push(":root".to_owned());
    }
    for family in ThemeFamily::ALL {
        for mode in [ColorMode::Light, ColorMode::Dark] {
            if ThemeVariant::resolve(family, mode) != variant {
                continue;
            }
            let mode_selector = match mode {
                ColorMode::Light => "",
                ColorMode::Dark => "[data-theme='dark']",
            };
            selectors.push(format!(":root[data-theme-family='{family}']{mode_selector}"));
        }
    }
    selectors
}

fn hex(color: Color) -> String {
    match color {
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        other => unreachable!("palettes hold RGB colors, not {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../../packages/app/src/themes.css");

    #[test]
    fn committed_stylesheet_matches_the_terminal_palettes() {
        assert!(
            COMMITTED == web_stylesheet(),
            "packages/app/src/themes.css is out of date; run `just web-themes`"
        );
    }

    #[test]
    fn browser_text_reaches_aa_contrast_in_every_palette() {
        for variant in ThemeVariant::ALL {
            let properties = properties(variant);
            let color = |name: &str| property(&properties, name);
            for background in ["paper", "surface", "annotation"] {
                for foreground in [
                    "ink",
                    "muted",
                    "focus",
                    "danger",
                    "severity-note",
                    "severity-low",
                    "severity-medium",
                    "severity-high",
                    "severity-critical",
                ] {
                    let ratio = contrast_ratio(color(foreground), color(background));
                    assert!(
                        ratio >= 4.5,
                        "{}: --{foreground} on --{background} is {ratio:.2}:1",
                        variant.as_str()
                    );
                }
                // The status dot is a graphic, so 3:1 is enough.
                let ratio = contrast_ratio(color("success"), color(background));
                assert!(
                    ratio >= 3.0,
                    "{}: --success on --{background} is {ratio:.2}:1",
                    variant.as_str()
                );
            }
        }
    }

    #[test]
    fn diff_text_and_syntax_reach_aa_contrast_on_every_diff_row() {
        for variant in ThemeVariant::ALL {
            let properties = properties(variant);
            let color = |name: &str| property(&properties, name);
            for background in [
                "surface",
                "diff-added",
                "diff-removed",
                "diff-added-word",
                "diff-removed-word",
            ] {
                for foreground in [
                    "ink",
                    "syntax-comment",
                    "syntax-string",
                    "syntax-constant",
                    "syntax-keyword",
                    "syntax-function",
                    "syntax-type",
                ] {
                    let ratio = contrast_ratio(color(foreground), color(background));
                    assert!(
                        ratio >= AA_TEXT,
                        "{}: --{foreground} on --{background} is {ratio:.2}:1",
                        variant.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn syntax_colors_stay_on_the_terminal_roles() {
        for variant in ThemeVariant::ALL {
            let theme = Theme::from_palette(variant, ColorDepth::Truecolor);
            let properties = properties(variant);
            let color = |name: &str| property(&properties, name);
            let rows = [
                color("surface"),
                color("diff-added"),
                color("diff-removed"),
                color("diff-added-word"),
                color("diff-removed-word"),
            ];
            for (name, role) in [
                ("syntax-comment", theme.syntax.comment),
                ("syntax-string", theme.syntax.string),
                ("syntax-constant", theme.syntax.constant),
                ("syntax-keyword", theme.syntax.keyword),
                ("syntax-function", theme.syntax.function),
                ("syntax-type", theme.syntax.r#type),
            ] {
                let passes = rows.iter().all(|row| contrast_ratio(role, *row) >= AA_TEXT);
                if passes {
                    assert_eq!(
                        color(name),
                        role,
                        "{}: --{name} leaves the terminal role",
                        variant.as_str()
                    );
                } else {
                    // The nudge stays small: a quarter of the way to black or white already passes.
                    // Light palettes' changed words take most of it, as they cannot lighten.
                    let target = if variant.is_light() { Color::Rgb(0, 0, 0) } else { Color::Rgb(255, 255, 255) };
                    let limit = mix(role, target, 25);
                    assert!(
                        rows.iter().all(|row| contrast_ratio(limit, *row) >= AA_TEXT),
                        "{}: --{name} needs more than a 25% step from the terminal role",
                        variant.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn changed_words_stand_out_from_their_row() {
        for variant in ThemeVariant::ALL {
            let properties = properties(variant);
            let color = |name: &str| property(&properties, name);
            for (word, row) in [("diff-added-word", "diff-added"), ("diff-removed-word", "diff-removed")] {
                let distance = delta_e(color(word), color(row));
                assert!(
                    distance >= 5.0,
                    "{}: --{word} is {distance:.1} delta E from --{row}",
                    variant.as_str()
                );
            }
        }
    }

    #[test]
    fn mix_and_readable_round_and_stop_at_the_first_passing_step() {
        let grey = Color::Rgb(100, 100, 100);
        assert_eq!(mix(grey, Color::Rgb(255, 255, 255), 0), grey);
        assert_eq!(mix(grey, Color::Rgb(255, 255, 255), 100), Color::Rgb(255, 255, 255));
        assert_eq!(
            mix(Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255), 50),
            Color::Rgb(128, 128, 128)
        );
        let paper = Color::Rgb(255, 255, 255);
        assert_eq!(readable(Color::Rgb(0, 0, 0), true, &[paper]), Color::Rgb(0, 0, 0));
        assert_eq!(
            readable(grey, true, &[paper]),
            grey,
            "#646464 already reaches 4.5:1 on white"
        );
        let pale = Color::Rgb(153, 153, 153);
        let darkened = readable(pale, true, &[paper]);
        assert_ne!(darkened, pale);
        assert!(contrast_ratio(darkened, paper) >= AA_TEXT);
        let lightened = readable(grey, false, &[Color::Rgb(40, 40, 40)]);
        assert!(contrast_ratio(lightened, Color::Rgb(40, 40, 40)) >= AA_TEXT);
        let one_step_less = (0..100)
            .map(|percent| mix(grey, Color::Rgb(255, 255, 255), percent))
            .take_while(|candidate| *candidate != lightened)
            .last()
            .expect("the grey needs lightening on a dark background");
        assert!(contrast_ratio(one_step_less, Color::Rgb(40, 40, 40)) < AA_TEXT);
    }

    #[test]
    fn auto_uses_nord_light_and_eldritch_minimal() {
        assert_eq!(
            ThemeVariant::resolve(ThemeFamily::Auto, ColorMode::Light),
            ThemeVariant::NordLight
        );
        assert_eq!(
            ThemeVariant::resolve(ThemeFamily::Auto, ColorMode::Dark),
            ThemeVariant::EldritchMinimal
        );
        let css = web_stylesheet();
        assert!(css.contains("/* nord-light */\n:root,\n:root[data-theme-family='auto'],\n"));
        assert!(css.contains(":root[data-theme-family='catppuccin'][data-theme='dark'] {"));
    }

    /// The CIE76 color difference between two colors, where 2.3 is about the
    /// smallest a viewer notices side by side.
    fn delta_e(left: Color, right: Color) -> f64 {
        let lab = |color: Color| {
            let Color::Rgb(red, green, blue) = color else {
                unreachable!("palettes hold RGB colors");
            };
            let linear = |channel: u8| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
            };
            let (r, g, b) = (linear(red), linear(green), linear(blue));
            let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
            let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
            [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
        };
        let (left, right) = (lab(left), lab(right));
        left.iter().zip(right).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt()
    }

    fn property(properties: &[(&'static str, Color)], name: &str) -> Color {
        properties
            .iter()
            .find(|(property, _)| *property == name)
            .map(|(_, color)| *color)
            .expect("property exists")
    }
}
