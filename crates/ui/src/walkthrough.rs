//! Walkthrough mode: the authored steps of a review, how well each step still
//! points at the diff, and the pane that shows them above the review stream.

use mire_core::{Anchor, AnchorSide, Changeset, FileContent, ReanchorOutcome, Walkthrough, WalkthroughStep};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::chrome::fit_line;
use crate::description::{single_line, wrap_text};
use crate::theme::Theme;

/// Text shown when walkthrough mode opens on a review without a walkthrough.
pub const NO_WALKTHROUGH: &str = "This review has no walkthrough.";

const NO_WALKTHROUGH_HINT: &str = "An agent writes one with `mire walkthrough apply`.";
const NO_OVERVIEW: &str = "No overview.";
const PANE_PADDING: usize = 2;
const DIVIDER_HEIGHT: u16 = 1;

/// The part of a walkthrough that walkthrough mode shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalkthroughPosition {
    /// The overview, read before the first step.
    Overview,
    /// One step, by zero-based index into the walkthrough's steps.
    Step(usize),
}

/// How one line of the walkthrough pane is styled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneStyle {
    /// The heading with the step progress.
    Heading,
    /// A stale or ambiguous step warning.
    Warning,
    /// Overview or narrative text.
    Body,
    /// Secondary text, such as the step list.
    Muted,
}

/// Counts a step's anchors and those that no longer point at one place.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StepHealth {
    /// Every anchor of the step.
    pub anchors: usize,
    /// Anchors whose code no longer matches any place in the diff.
    pub stale: usize,
    /// Anchors whose code matches more than one place in the diff.
    pub ambiguous: usize,
}

/// One wrapped line of the walkthrough pane, before a theme styles it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneLine {
    /// How the line is styled.
    pub style: PaneStyle,
    /// The text, already free of control characters.
    pub text: String,
}

impl WalkthroughPosition {
    /// Returns the one-based step number, or 0 for the overview.
    pub const fn number(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Step(index) => index + 1,
        }
    }

    /// Returns the next position, staying on the last step.
    pub fn next(self, steps: usize) -> Self {
        match self {
            _ if steps == 0 => Self::Overview,
            Self::Overview => Self::Step(0),
            Self::Step(index) => Self::Step((index + 1).min(steps - 1)),
        }
    }

    /// Returns the previous position, ending on the overview.
    pub const fn previous(self) -> Self {
        match self {
            Self::Overview | Self::Step(0) => Self::Overview,
            Self::Step(index) => Self::Step(index - 1),
        }
    }

    /// Keeps the position inside a walkthrough with `steps` steps.
    pub fn clamp(self, steps: usize) -> Self {
        match self {
            Self::Step(_) if steps == 0 => Self::Overview,
            Self::Step(index) => Self::Step(index.min(steps - 1)),
            Self::Overview => Self::Overview,
        }
    }
}

impl StepHealth {
    /// Counts the stale and ambiguous anchors of one step.
    pub fn of(step: &WalkthroughStep) -> Self {
        let mut health = Self { anchors: step.anchors().len(), ..Self::default() };
        for anchor in step.anchors() {
            match anchor.reanchor_outcome() {
                Some(ReanchorOutcome::Stale { .. }) => health.stale += 1,
                Some(ReanchorOutcome::Ambiguous { .. }) => health.ambiguous += 1,
                _ => {}
            }
        }
        health
    }

    /// Reports whether no anchor points at one place in the diff.
    pub const fn unplaced(self) -> bool {
        self.stale + self.ambiguous == self.anchors
    }

    /// Returns plain-language warnings, empty when every anchor is placed.
    pub fn warnings(self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.stale > 0 {
            warnings.push(format!(
                "Stale: {} of {} {} no longer {} the diff. The code changed after this step was written.",
                self.stale,
                self.anchors,
                plural(self.anchors, "location", "locations"),
                plural(self.stale, "matches", "match"),
            ));
        }
        if self.ambiguous > 0 {
            warnings.push(format!(
                "Ambiguous: {} of {} {} {} more than one place in the diff.",
                self.ambiguous,
                self.anchors,
                plural(self.anchors, "location", "locations"),
                plural(self.ambiguous, "matches", "match"),
            ));
        }
        if !warnings.is_empty() && self.unplaced() {
            warnings.push("The diff did not move, because this step has no place to show.".to_owned());
        }
        warnings
    }

    /// Returns a short tag for the step list, or `None` for a placed step.
    pub const fn tag(self) -> Option<&'static str> {
        match (self.stale > 0, self.ambiguous > 0) {
            (true, true) => Some("stale, ambiguous"),
            (true, false) => Some("stale"),
            (false, true) => Some("ambiguous"),
            (false, false) => None,
        }
    }
}

/// Builds the pane's lines for a position, wrapped to `width` cells.
///
/// Agent-written text is wrapped by [`wrap_text`], which drops control
/// characters so a narrative cannot move the terminal cursor.
pub fn pane_lines(walkthrough: Option<&Walkthrough>, position: WalkthroughPosition, width: u16) -> Vec<PaneLine> {
    build_pane(walkthrough, position, width, usize::MAX)
}

/// Counts the pane's lines for a position, stopping at `limit`.
///
/// Layout needs only enough lines to fill the pane, so this skips wrapping
/// the rest of a long step list.
pub fn pane_line_count(
    walkthrough: Option<&Walkthrough>, position: WalkthroughPosition, width: u16, limit: usize,
) -> usize {
    build_pane(walkthrough, position, width, limit).len()
}

/// Returns the pane height for its lines: the lines and a divider, at most
/// half of `available` rows, and none when fewer than four rows are free.
pub fn pane_height(line_count: usize, available: u16) -> u16 {
    let limit = available / 2;
    if limit < 2 {
        return 0;
    }
    u16::try_from(line_count)
        .unwrap_or(u16::MAX)
        .saturating_add(DIVIDER_HEIGHT)
        .min(limit)
}

/// Finds the file, hunk, and first line index an anchor points at in a capture.
pub fn locate_anchor(changeset: &Changeset, anchor: &Anchor) -> Option<(usize, usize, Option<usize>)> {
    changeset.files().iter().enumerate().find_map(|(file, diff)| {
        let side = match anchor.side() {
            AnchorSide::Old => diff.old_side(),
            AnchorSide::New => diff.new_side(),
        }?;
        if &side.path != anchor.path() {
            return None;
        }
        let FileContent::Text { hunks } = diff.content() else {
            return None;
        };
        let (hunk, found) = hunks
            .iter()
            .enumerate()
            .find(|(_, hunk)| hunk.fingerprint() == anchor.hunk_fingerprint())?;
        let line = found.lines().iter().position(|line| {
            let number = match anchor.side() {
                AnchorSide::Old => line.old_line(),
                AnchorSide::New => line.new_line(),
            };
            number == Some(anchor.range().start())
        });
        Some((file, hunk, line))
    })
}

/// Renders the walkthrough pane above the review stream.
pub fn render_walkthrough(frame: &mut Frame<'_>, area: Rect, app: &App<'_>, theme: &Theme) {
    let Some(position) = app.walkthrough_view() else {
        return;
    };
    if area.height == 0 {
        return;
    }
    let text_rows = usize::from(area.height.saturating_sub(DIVIDER_HEIGHT));
    let mut lines = pane_lines(app.walkthrough(), position, area.width);
    // The first line names the overview or step, so it stays when the pane
    // has a single text row. The cut marker needs a row of its own below it.
    if lines.len() > text_rows && text_rows > 1 {
        let hidden = lines.len() - text_rows + 1;
        lines.truncate(text_rows - 1);
        lines.push(PaneLine { style: PaneStyle::Muted, text: format!("… {hidden} more lines") });
    }
    let rendered = lines
        .into_iter()
        .take(text_rows)
        .map(|line| {
            let style = match line.style {
                PaneStyle::Heading => theme.accent,
                PaneStyle::Warning => theme.error,
                PaneStyle::Body => theme.panel,
                PaneStyle::Muted => theme.muted,
            };
            fit_line(
                vec![Span::raw(" "), Span::styled(line.text, style)],
                usize::from(area.width),
                theme.panel,
            )
        })
        .collect::<Vec<Line<'static>>>();
    frame.render_widget(
        Paragraph::new(rendered)
            .style(theme.panel)
            .block(Block::new().borders(Borders::BOTTOM).border_style(theme.divider)),
        area,
    );
}

fn build_pane(
    walkthrough: Option<&Walkthrough>, position: WalkthroughPosition, width: u16, limit: usize,
) -> Vec<PaneLine> {
    let width = usize::from(width).saturating_sub(PANE_PADDING).max(1);
    let mut lines = Vec::new();
    let Some(walkthrough) = walkthrough else {
        push_wrapped(&mut lines, PaneStyle::Heading, "Walkthrough", width);
        push_wrapped(&mut lines, PaneStyle::Body, NO_WALKTHROUGH, width);
        push_wrapped(&mut lines, PaneStyle::Muted, NO_WALKTHROUGH_HINT, width);
        return lines;
    };
    let steps = walkthrough.steps();
    match position.clamp(steps.len()) {
        WalkthroughPosition::Overview => {
            let count = format!("{} {}", steps.len(), plural(steps.len(), "step", "steps"));
            push_wrapped(&mut lines, PaneStyle::Heading, &format!("Overview · {count}"), width);
            match walkthrough.overview() {
                Some(overview) => push_wrapped(&mut lines, PaneStyle::Body, overview, width),
                None => push_wrapped(&mut lines, PaneStyle::Muted, NO_OVERVIEW, width),
            }
            for (index, step) in steps.iter().enumerate() {
                if lines.len() >= limit {
                    break;
                }
                let tag = StepHealth::of(step)
                    .tag()
                    .map_or(String::new(), |tag| format!(" ({tag})"));
                let entry = format!("{}. {}{tag}", index + 1, single_line(step.title()));
                push_wrapped(&mut lines, PaneStyle::Muted, &entry, width);
            }
        }
        WalkthroughPosition::Step(index) => {
            let step = &steps[index];
            let heading = format!("Step {} of {} · {}", index + 1, steps.len(), single_line(step.title()));
            push_wrapped(&mut lines, PaneStyle::Heading, &heading, width);
            for warning in StepHealth::of(step).warnings() {
                push_wrapped(&mut lines, PaneStyle::Warning, &warning, width);
            }
            push_wrapped(&mut lines, PaneStyle::Body, step.narrative(), width);
        }
    }
    lines.truncate(limit);
    lines
}

fn push_wrapped(lines: &mut Vec<PaneLine>, style: PaneStyle, text: &str, width: usize) {
    lines.extend(wrap_text(text, width).into_iter().map(|text| PaneLine { style, text }));
}

const fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
pub mod fixtures {
    //! Reviews with walkthroughs for the walkthrough, app, and view tests.

    use mire_core::{
        AnchorSide, Author, BytePath, Changeset, ChangesetSource, LineNumber, LineRange, PatchLimits, Provenance,
        Review, ReviewRevision, StepInput, StepLocation, WalkthroughInput, parse_patch,
    };

    /// A three-file change: a port constant, a parser fix, and a new guide.
    pub const PATCH: &[u8] = b"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub mod parse;
-pub const PORT: u16 = 80;
+pub const PORT: u16 = 8080;
 pub const HOST: &str = \"localhost\";
diff --git a/src/parse/mod.rs b/src/parse/mod.rs
--- a/src/parse/mod.rs
+++ b/src/parse/mod.rs
@@ -1,3 +1,4 @@
 pub fn split(line: &str) -> Option<(&str, &str)> {
-    line.split_once('=')
+    let (key, value) = line.split_once('=')?;
+    Some((key.trim(), value.trim()))
 }
diff --git a/docs/guide.md b/docs/guide.md
new file mode 100644
--- /dev/null
+++ b/docs/guide.md
@@ -0,0 +1,2 @@
+# Guide
+Read key = value lines.
";

    /// A later capture: the port changed again and the guide repeats itself.
    pub const LATER_PATCH: &[u8] = b"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub mod parse;
-pub const PORT: u16 = 80;
+pub const PORT: u16 = 9090;
 pub const HOST: &str = \"localhost\";
diff --git a/src/parse/mod.rs b/src/parse/mod.rs
--- a/src/parse/mod.rs
+++ b/src/parse/mod.rs
@@ -1,3 +1,4 @@
 pub fn split(line: &str) -> Option<(&str, &str)> {
-    line.split_once('=')
+    let (key, value) = line.split_once('=')?;
+    Some((key.trim(), value.trim()))
 }
diff --git a/docs/guide.md b/docs/guide.md
new file mode 100644
--- /dev/null
+++ b/docs/guide.md
@@ -0,0 +1,4 @@
+# Guide
+Read key = value lines.
+# Guide
+Read key = value lines.
";

    /// Parses a fixture patch.
    pub fn changeset(patch: &[u8]) -> Changeset {
        parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap()
    }

    /// A review of [`PATCH`] without a walkthrough.
    pub fn plain_review() -> Review {
        Review::new(
            ReviewRevision::new(1).unwrap(),
            changeset(PATCH),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    /// A review of [`PATCH`] with a three-step walkthrough.
    pub fn walkthrough_review() -> Review {
        walkthrough_review_with_steps(3)
    }

    /// A review of [`PATCH`] with the first `count` steps of the walkthrough
    /// [`walkthrough_review`] has.
    pub fn walkthrough_review_with_steps(count: usize) -> Review {
        let review = plain_review();
        let location = |path: &str, start: u64, end: u64| {
            StepLocation::new(
                BytePath::new(path.as_bytes().to_vec()).unwrap(),
                AnchorSide::New,
                LineRange::new(LineNumber::new(start).unwrap(), LineNumber::new(end).unwrap()).unwrap(),
            )
        };
        let step = |title: &str, narrative: &str, path: &str, start: u64, end: u64| {
            StepInput::new(
                None,
                title.to_owned(),
                narrative.to_owned(),
                vec![location(path, start, end)],
                Vec::new(),
                None,
            )
        };
        let mut steps = vec![
            step(
                "Trim keys and values",
                "`split` now trims both sides and returns `None` when there is no `=`. Check that callers expect trimmed text.",
                "src/parse/mod.rs",
                2,
                3,
            ),
            step(
                "Move the dev server port",
                "The default port moves from 80 to 8080, so the server starts without root.",
                "src/lib.rs",
                2,
                2,
            ),
            step(
                "Document the format",
                "The guide names the `key = value` format the parser reads.",
                "docs/guide.md",
                1,
                2,
            ),
        ];
        steps.truncate(count);
        let input = WalkthroughInput::new(
            Some("Parsed values keep their spaces, so `port = 8080` fails to match. This change trims them and moves the dev server port.".to_owned()),
            steps,
            Author::new("review-agent", None).unwrap(),
            Provenance::Agent { producer: "fixture".to_owned() },
        );
        review.write_walkthrough(review.revision(), input).unwrap()
    }

    /// The walkthrough review refreshed to [`LATER_PATCH`]: step 2 is stale
    /// and step 3 is ambiguous.
    pub fn refreshed_review() -> Review {
        walkthrough_review().reanchor(changeset(LATER_PATCH)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::fixtures::{plain_review, refreshed_review, walkthrough_review, walkthrough_review_with_steps};
    use super::*;
    use crate::AppOptions;
    use crate::app::InteractionMode;

    #[test]
    fn keys_open_the_overview_step_through_and_close_walkthrough_mode() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        press(&mut app, 'W');
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Overview));
        assert_eq!(app.walkthrough_progress(), Some((0, 3)));
        assert_eq!(app.interaction_mode(), InteractionMode::Walkthrough);
        assert!(app.areas().walkthrough.height > 0);

        press(&mut app, 'J');
        press(&mut app, 'J');
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Step(1)));
        assert_eq!(app.walkthrough_progress(), Some((2, 3)));
        assert_eq!(selected_path(&app), "src/lib.rs");

        press(&mut app, 'K');
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Step(0)));
        assert_eq!(selected_path(&app), "src/parse/mod.rs");

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.walkthrough_view(), None);
        assert_eq!(app.areas().walkthrough.height, 0);
    }

    #[test]
    fn a_step_scrolls_its_hunk_to_the_top_and_expands_a_collapsed_file() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        press(&mut app, 'J');
        press(&mut app, 'J');
        press(&mut app, 'J');
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Step(2)));
        assert_eq!(selected_path(&app), "docs/guide.md");

        // Collapse the guide, then come back to its step.
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.file_collapsed(0));
        press(&mut app, 'K');
        press(&mut app, 'J');
        assert!(!app.file_collapsed(0));
        assert_eq!(selected_path(&app), "docs/guide.md");
    }

    #[test]
    fn stale_and_ambiguous_steps_leave_the_diff_in_place_and_say_why() {
        let review = refreshed_review();
        let steps = review.walkthrough().unwrap().steps();
        assert_eq!(
            StepHealth::of(&steps[1]),
            StepHealth { anchors: 1, stale: 1, ambiguous: 0 }
        );
        assert_eq!(
            StepHealth::of(&steps[2]),
            StepHealth { anchors: 1, stale: 0, ambiguous: 1 }
        );

        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        press(&mut app, 'J');
        let scroll = app.scroll();
        press(&mut app, 'J');
        assert_eq!(app.scroll(), scroll);
        let text = pane_lines(app.walkthrough(), WalkthroughPosition::Step(1), 120)
            .into_iter()
            .map(|line| line.text)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            text.contains("Stale: 1 of 1 location no longer matches the diff."),
            "{text}"
        );
    }

    #[test]
    fn live_walkthrough_commands_step_through_authored_steps() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        app.start_walkthrough().unwrap();
        assert!(app.live_state().walkthrough_active);
        assert_eq!(app.live_state().walkthrough_progress, Some((0, 3)));

        app.move_walkthrough(true).unwrap();
        app.move_walkthrough(true).unwrap();
        assert_eq!(app.live_state().walkthrough_progress, Some((2, 3)));
        assert_eq!(
            app.live_state().selected_path.as_deref(),
            Some(b"src/lib.rs".as_slice())
        );
        app.move_walkthrough(false).unwrap();
        assert_eq!(app.live_state().walkthrough_progress, Some((1, 3)));

        app.stop_walkthrough().unwrap();
        assert!(!app.live_state().walkthrough_active);
        assert_eq!(app.walkthrough_view(), None);
        assert_eq!(app.live_state().walkthrough_progress, None);
    }

    #[test]
    fn closing_walkthrough_mode_locally_ends_the_live_walkthrough() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        app.start_walkthrough().unwrap();
        app.move_walkthrough(true).unwrap();
        assert_eq!(app.live_state().walkthrough_progress, Some((1, 3)));

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let state = app.live_state();
        assert!(!state.walkthrough_active);
        assert_eq!(state.walkthrough_progress, None);

        let scroll = app.scroll();
        assert_eq!(app.move_walkthrough(true), Err("walkthrough_inactive"));
        assert_eq!(app.walkthrough_view(), None);
        assert_eq!(app.scroll(), scroll);

        app.start_walkthrough().unwrap();
        app.move_walkthrough(true).unwrap();
        press(&mut app, 'W');
        assert!(!app.live_state().walkthrough_active);
        assert_eq!(app.walkthrough_view(), None);
    }

    #[test]
    fn live_steps_without_start_are_refused_and_leave_the_terminal_in_place() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        let (scroll, path) = (app.scroll(), selected_path(&app));
        assert_eq!(app.move_walkthrough(true), Err("walkthrough_inactive"));
        assert_eq!(app.move_walkthrough(false), Err("walkthrough_inactive"));
        assert_eq!(app.walkthrough_view(), None);
        assert_eq!(app.scroll(), scroll);
        assert_eq!(selected_path(&app), path);

        // A walkthrough the terminal opened with W is not a session walkthrough.
        press(&mut app, 'W');
        assert_eq!(app.move_walkthrough(true), Err("walkthrough_inactive"));
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Overview));

        // Without an authored walkthrough, the steps stay finding navigation.
        let plain = plain_review();
        let mut app = App::review_with_options(&plain, AppOptions::default());
        app.resize(120, 24);
        assert_ne!(app.move_walkthrough(true), Err("walkthrough_inactive"));
    }

    #[test]
    fn live_walkthrough_commands_wait_while_the_terminal_takes_input() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        app.start_walkthrough().unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(app.live_control_busy());
        let (scroll, path) = (app.scroll(), selected_path(&app));
        assert_eq!(app.move_walkthrough(true), Err("interaction_busy"));
        assert_eq!(app.move_walkthrough(false), Err("interaction_busy"));
        assert_eq!(app.stop_walkthrough(), Err("interaction_busy"));
        assert_eq!(app.walkthrough_view(), Some(WalkthroughPosition::Overview));
        assert!(app.live_state().walkthrough_active);
        assert_eq!(app.scroll(), scroll);
        assert_eq!(selected_path(&app), path);
    }

    #[test]
    fn reload_without_the_walkthrough_ends_the_session_walkthrough() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        app.start_walkthrough().unwrap();
        app.move_walkthrough(true).unwrap();
        let position = app.position();

        let plain = plain_review();
        let mut reloaded = App::review_with_options(&plain, AppOptions::default());
        reloaded.resize(120, 24);
        reloaded.restore_position(&position);
        let state = reloaded.live_state();
        assert!(!state.walkthrough_active);
        assert_eq!(state.walkthrough_progress, None);
        assert_eq!(reloaded.walkthrough_view(), None);
    }

    #[test]
    fn reload_with_fewer_steps_clamps_the_session_walkthrough() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        app.start_walkthrough().unwrap();
        for _ in 0..3 {
            app.move_walkthrough(true).unwrap();
        }
        assert_eq!(app.live_state().walkthrough_progress, Some((3, 3)));
        let position = app.position();

        let shorter = walkthrough_review_with_steps(1);
        let mut reloaded = App::review_with_options(&shorter, AppOptions::default());
        reloaded.resize(120, 24);
        reloaded.restore_position(&position);
        let state = reloaded.live_state();
        assert!(state.walkthrough_active);
        assert_eq!(state.walkthrough_progress, Some((1, 1)));
        assert_eq!(reloaded.walkthrough_view(), Some(WalkthroughPosition::Step(0)));
        reloaded.stop_walkthrough().unwrap();
        assert!(!reloaded.live_state().walkthrough_active);
    }

    #[test]
    fn pane_line_count_stops_at_its_limit() {
        let review = walkthrough_review();
        let full = pane_lines(review.walkthrough(), WalkthroughPosition::Overview, 120).len();
        assert_eq!(
            pane_line_count(review.walkthrough(), WalkthroughPosition::Overview, 120, usize::MAX),
            full
        );
        assert_eq!(
            pane_line_count(review.walkthrough(), WalkthroughPosition::Overview, 120, 2),
            2
        );
    }

    #[test]
    fn local_walkthrough_mode_reports_no_live_progress() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        press(&mut app, 'W');
        assert_eq!(app.walkthrough_progress(), Some((0, 3)));
        assert!(!app.live_state().walkthrough_active);
        assert_eq!(app.live_state().walkthrough_progress, None);
    }

    #[test]
    fn walkthrough_mode_keeps_file_keys_when_the_sidebar_has_focus() {
        let review = walkthrough_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(120, 24);
        press(&mut app, 'W');
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.interaction_mode(), InteractionMode::WalkthroughSidebar);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.interaction_mode(), InteractionMode::Walkthrough);
    }

    #[test]
    fn walkthrough_mode_without_a_walkthrough_says_so() {
        let review = plain_review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        press(&mut app, 'W');
        assert_eq!(app.walkthrough_progress(), None);
        let lines = pane_lines(app.walkthrough(), WalkthroughPosition::Overview, 80);
        assert!(lines.iter().any(|line| line.text == NO_WALKTHROUGH));
    }

    #[test]
    fn pane_text_drops_control_characters() {
        let lines = pane_lines(None, WalkthroughPosition::Overview, 80);
        assert!(lines.iter().all(|line| !line.text.chars().any(char::is_control)));
        let mut lines = Vec::new();
        push_wrapped(&mut lines, PaneStyle::Body, "a\u{1b}[2Jb", 80);
        assert_eq!(lines[0].text, "a[2Jb");
    }

    #[test]
    fn positions_move_between_the_overview_and_the_last_step() {
        let overview = WalkthroughPosition::Overview;
        assert_eq!(overview.next(3), WalkthroughPosition::Step(0));
        assert_eq!(WalkthroughPosition::Step(2).next(3), WalkthroughPosition::Step(2));
        assert_eq!(WalkthroughPosition::Step(0).previous(), overview);
        assert_eq!(overview.previous(), overview);
        assert_eq!(overview.next(0), overview);
        assert_eq!(WalkthroughPosition::Step(5).clamp(2), WalkthroughPosition::Step(1));
        assert_eq!(WalkthroughPosition::Step(1).number(), 2);
    }

    #[test]
    fn warnings_name_stale_and_ambiguous_locations_in_plain_words() {
        let placed = StepHealth { anchors: 2, stale: 0, ambiguous: 0 };
        assert!(placed.warnings().is_empty());
        assert_eq!(placed.tag(), None);

        let stale = StepHealth { anchors: 2, stale: 1, ambiguous: 0 };
        assert_eq!(
            stale.warnings(),
            ["Stale: 1 of 2 locations no longer matches the diff. The code changed after this step was written."]
        );

        let unplaced = StepHealth { anchors: 1, stale: 0, ambiguous: 1 };
        assert_eq!(
            unplaced.warnings(),
            [
                "Ambiguous: 1 of 1 location matches more than one place in the diff.",
                "The diff did not move, because this step has no place to show.",
            ]
        );
        assert_eq!(unplaced.tag(), Some("ambiguous"));
    }

    #[test]
    fn pane_height_leaves_half_the_rows_for_the_diff() {
        assert_eq!(pane_height(2, 20), 3);
        assert_eq!(pane_height(30, 20), 10);
        assert_eq!(pane_height(5, 3), 0);
    }

    fn press(app: &mut App<'_>, character: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::SHIFT));
    }

    fn selected_path(app: &App<'_>) -> String {
        String::from_utf8(app.live_state().selected_path.unwrap()).unwrap()
    }
}
