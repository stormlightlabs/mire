//! The browser review's stylesheet, built from the terminal palettes.
//!
//! `packages/app/src/themes.css` holds this output. A test fails when the two
//! differ; `just web-themes` rewrites the file.

use std::fmt::Write as _;

use mire_core::NoteSeverity;
use ratatui::style::{Color, Style};

use super::{ColorMode, Theme, ThemeFamily, ThemeVariant};

/// Builds CSS custom properties for every palette, selected by the
/// `data-theme-family` and `data-theme` attributes on the root element.
///
/// `data-theme-family` holds a [`ThemeFamily`] identifier and `data-theme`
/// holds `light` or `dark`. Each variant's properties come from the roles
/// [`Theme::from_palette`] assigns, so the browser and the terminal share them.
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
fn properties(variant: ThemeVariant) -> [(&'static str, Color); 20] {
    let theme = Theme::from_palette(variant);
    let fg = |style: Style| style.fg.expect("palette roles set a foreground");
    let bg = |style: Style| style.bg.expect("palette roles set a background");
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
    ]
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
    use crate::theme::contrast::contrast_ratio;

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
            let color = |name: &str| {
                properties
                    .iter()
                    .find(|(property, _)| *property == name)
                    .map(|(_, color)| *color)
                    .expect("property exists")
            };
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
}
