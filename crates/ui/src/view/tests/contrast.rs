//! Contrast of every cell the review draws, in every color palette.
//!
//! The pairs come from rendered frames, not from a list of roles, so a role
//! drawn on a new background is checked as soon as a scene draws it.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

use super::*;
use crate::theme::contrast::contrast_ratio;
use crate::{ColorDepth, ColorMode, ThemeFamily};

/// Contrast WCAG 2 asks of text.
const TEXT_CONTRAST: f64 = 4.5;
/// Contrast WCAG 2 asks of graphics: box-drawing borders, the scroll bar and
/// geometric marks such as the disclosure triangles and the finding diamond.
const GRAPHIC_CONTRAST: f64 = 3.0;

/// One added and one removed line with an intraline change, context lines for
/// syntax colors, and a second file so the sidebar has an unselected row.
const INTRALINE_PATCH: &[u8] = b"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,5 @@
 /// Prints the total.
 fn main() {
-    let total = 1;
+    let total = 2; // changed
     println!(\"{total}\");
 }
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,2 @@
-pub const NAME: &str = \"old\";
+pub const NAME: &str = \"new\";
 pub struct Config;
";

#[test]
fn every_drawn_cell_has_a_theme_background_and_readable_contrast() {
    let intraline = parse_patch(
        INTRALINE_PATCH,
        ChangesetSource::Patch { label: None },
        PatchLimits::default(),
    )
    .unwrap();
    let markers = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
    let mut generated = intraline.clone();
    for index in 0..generated.files().len() {
        generated.set_generated(index, true);
    }
    let review = notes_review();
    let severities = severities_review();

    let mut failures = Vec::new();
    for family in [ThemeFamily::Iceberg, ThemeFamily::Eldritch, ThemeFamily::Catppuccin] {
        for (mode, depth) in [ColorMode::Dark, ColorMode::Light]
            .into_iter()
            .flat_map(|mode| [ColorDepth::Truecolor, ColorDepth::Xterm256].map(|depth| (mode, depth)))
        {
            let theme = Theme::resolve(family, Some(mode), depth);
            let name = format!("{family}/{mode:?}/{depth:?}");
            let scenes: Vec<(&str, App<'_>, u16, u16)> = vec![
                ("split", keys(App::ready(&intraline), &[KeyCode::Char('2')]), 160, 16),
                ("unified", keys(App::ready(&intraline), &[KeyCode::Char('1')]), 100, 16),
                ("markers", App::ready(&markers), 120, 12),
                ("help", keys(App::ready(&intraline), &[KeyCode::Char('?')]), 90, 24),
                ("generated", App::ready(&generated), 120, 16),
                (
                    "generated focus",
                    keys(App::ready(&generated), &[KeyCode::Tab, KeyCode::Down]),
                    120,
                    16,
                ),
                (
                    "sidebar focus",
                    keys(App::ready(&intraline), &[KeyCode::Tab, KeyCode::Down]),
                    120,
                    16,
                ),
                (
                    "range",
                    keys(
                        App::ready(&intraline),
                        &[KeyCode::Down, KeyCode::Down, KeyCode::Char('v'), KeyCode::Down],
                    ),
                    120,
                    16,
                ),
                (
                    "search",
                    keys(
                        App::ready(&intraline),
                        &[
                            KeyCode::Char('/'),
                            KeyCode::Char('t'),
                            KeyCode::Char('o'),
                            KeyCode::Char('t'),
                            KeyCode::Enter,
                        ],
                    ),
                    120,
                    16,
                ),
                ("notes", review_app(&review, &[]), 120, 20),
                ("note focus", review_app(&review, &[KeyCode::Char('p')]), 120, 20),
                (
                    "note editor",
                    review_app(&review, &[KeyCode::Down, KeyCode::Char('c')]),
                    120,
                    20,
                ),
                ("filter", review_app(&review, &[KeyCode::Char('f')]), 120, 20),
                ("severities", review_app(&severities, &[]), 120, 24),
                (
                    "severity focus",
                    review_app(&severities, &[KeyCode::Char('p')]),
                    120,
                    24,
                ),
                (
                    "severity sidebar",
                    review_app(&severities, &[KeyCode::Tab, KeyCode::Down]),
                    120,
                    24,
                ),
                ("error", App::error("broken input"), 40, 6),
            ];
            for (scene, app, width, height) in scenes {
                let buffer = render_buffer(app, width, height, &theme);
                check_buffer(&buffer, &name, scene, &mut failures);
            }
        }
    }
    failures.sort();
    failures.dedup();
    assert!(
        failures.is_empty(),
        "{} failing pairs:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn keys<'a>(mut app: App<'a>, codes: &[KeyCode]) -> App<'a> {
    app.resize(120, 16);
    for code in codes {
        press(&mut app, *code);
    }
    app
}

fn review_app<'a>(review: &'a mire_core::Review, codes: &[KeyCode]) -> App<'a> {
    keys(App::review_with_options(review, crate::AppOptions::default()), codes)
}

fn render_buffer(mut app: App<'_>, width: u16, height: u16, theme: &Theme) -> Buffer {
    app.resize(width, height);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, &app, theme)).unwrap();
    terminal.backend().buffer().clone()
}

/// Records each cell that leaves the terminal's own color showing, and each
/// drawn glyph below its contrast target, once per palette and color pair.
fn check_buffer(buffer: &Buffer, palette: &str, scene: &str, failures: &mut Vec<String>) {
    for cell in &buffer.content {
        let symbol = cell.symbol();
        let style = cell.style();
        let Some(Color::Rgb(..)) = style.bg else {
            failures.push(format!(
                "{palette}: {scene} leaves the terminal background under {symbol:?}"
            ));
            continue;
        };
        if symbol.trim().is_empty() {
            continue;
        }
        let Some(Color::Rgb(..)) = style.fg else {
            failures.push(format!(
                "{palette}: {scene} draws {symbol:?} in the terminal foreground"
            ));
            continue;
        };
        let (foreground, background) = (style.fg.unwrap(), style.bg.unwrap());
        let target = if symbol.chars().all(is_graphic) { GRAPHIC_CONTRAST } else { TEXT_CONTRAST };
        let ratio = contrast_ratio(foreground, background);
        if ratio < target {
            let kind = if target == TEXT_CONTRAST { "text" } else { "graphic" };
            let failure =
                format!("{palette}: {kind} {foreground:?} on {background:?} is {ratio:.2}:1, below {target}:1");
            if !failures.contains(&failure) {
                failures.push(failure);
            }
        }
    }
}

/// Box drawing, block elements and geometric shapes: borders, the scroll bar,
/// the selection bar and the disclosure and finding marks.
fn is_graphic(character: char) -> bool {
    ('\u{2500}'..='\u{25FF}').contains(&character)
}
