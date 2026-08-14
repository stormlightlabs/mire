use std::borrow::Cow;

use mire_core::{FileContent, FileDiff, FileStatus, LineKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, AppState, InteractionMode};
use crate::navigation::{Focus, help_entries};
use crate::stream::ReviewStream;
use crate::theme::Theme;

const EDITOR_FOOTER: &[(&str, &str)] = &[
    ("Enter", "save"),
    ("Tab", "severity"),
    ("Shift-Tab", "kind"),
    ("Esc", "cancel"),
];
const FILTER_FOOTER: &[(&str, &str)] = &[("a/s/v/k/i", "filter"), ("c", "clear"), ("Esc", "close")];
const SEARCH_FOOTER: &[(&str, &str)] = &[("Enter", "find"), ("Esc", "cancel")];
const RANGE_FOOTER: &[(&str, &str)] = &[("j/k", "extend"), ("c", "note"), ("v / Esc", "cancel")];
const HELP_FOOTER: &[(&str, &str)] = &[("? / Esc", "close"), ("q", "quit")];
const REVIEW_FOOTER: &[(&str, &str)] = &[("?", "help"), ("q", "quit")];
const FINDING_FOOTER: &[(&str, &str)] = &[
    ("e", "edit"),
    ("r/d/o/a", "status"),
    ("p/P", "findings"),
    ("Esc", "clear focus"),
    ("q", "quit"),
];
const SIDEBAR_FOOTER: &[(&str, &str)] = &[("j/k", "files"), ("Tab", "review"), ("Esc", "review"), ("q", "quit")];
const SOURCE_FOOTER: &[(&str, &str)] = &[
    ("j/k", "move"),
    ("v", "range"),
    ("c", "note"),
    ("/", "search"),
    ("?", "help"),
    ("q", "quit"),
];

/// Renders the responsive application status bar.
pub fn render_title(frame: &mut Frame<'_>, area: Rect, app: &App<'_>, theme: &Theme) {
    let layout = match app.state() {
        AppState::Ready(stream) => stream.layout().label(),
        AppState::Loading | AppState::Empty | AppState::Error(_) => "unavailable",
    };
    let focus = match app.focus() {
        Focus::Review => "review",
        Focus::Sidebar => "files",
    };
    let mut spans = vec![Span::styled(" Mire review", theme.accent)];
    push_title_segment(&mut spans, layout.to_owned(), theme.muted, theme);
    if let Some(error) = app.note_error() {
        push_title_segment(&mut spans, format!("unsaved: {error}"), theme.error, theme);
    } else if app.filter_visible() {
        push_title_segment(&mut spans, app.filter_summary(), theme.accent, theme);
    } else if let Some(selection) = app.selection_label() {
        push_title_segment(&mut spans, selection, theme.accent, theme);
    } else if app.is_dirty() {
        push_title_segment(&mut spans, "unsaved".to_owned(), theme.error, theme);
    } else if app.search_input() {
        push_title_segment(&mut spans, format!("/{}█", app.search_query()), theme.accent, theme);
    } else if let Some((current, total)) = app.search_status() {
        push_title_segment(
            &mut spans,
            format!("/{} {current}/{total}", app.search_query()),
            theme.accent,
            theme,
        );
    } else {
        if area.width >= 40 {
            push_title_segment(&mut spans, format!("focus {focus}"), theme.muted, theme);
        }
        if area.width >= 56 {
            push_title_segment(
                &mut spans,
                format!("context {}", app.context_lines()),
                theme.muted,
                theme,
            );
        }
        if area.width >= 72 {
            push_title_segment(
                &mut spans,
                format!("wrap {}", if app.wrap_lines() { "on" } else { "off" }),
                theme.muted,
                theme,
            );
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.title), area);
}

/// Renders keyboard hints for the active interaction state without hiding its mode on narrow terminals.
pub fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App<'_>, theme: &Theme) {
    let (mode, entries) = footer_context(app.interaction_mode());
    let mut spans = vec![Span::raw(" "), Span::styled(mode, theme.accent)];
    let mut used = 1 + Span::raw(mode).width();
    for (key, description) in entries {
        let entry_width = Span::raw(*key).width() + 1 + Span::raw(*description).width();
        if used.saturating_add(2).saturating_add(entry_width) > usize::from(area.width) {
            break;
        }
        spans.push(Span::styled("  ", theme.divider));
        spans.push(Span::styled(*key, theme.accent));
        spans.push(Span::styled(format!(" {description}"), theme.footer));
        used += 2 + entry_width;
    }
    frame.render_widget(
        Paragraph::new(fit_line(spans, usize::from(area.width), theme.footer)),
        area,
    );
}

fn footer_context(mode: InteractionMode) -> (&'static str, &'static [(&'static str, &'static str)]) {
    match mode {
        InteractionMode::Editor => ("editor", EDITOR_FOOTER),
        InteractionMode::Filter => ("filter", FILTER_FOOTER),
        InteractionMode::Search => ("search", SEARCH_FOOTER),
        InteractionMode::RangeSelection => ("range", RANGE_FOOTER),
        InteractionMode::Help => ("help", HELP_FOOTER),
        InteractionMode::Review => ("", REVIEW_FOOTER),
        InteractionMode::Finding => ("finding", FINDING_FOOTER),
        InteractionMode::Sidebar => ("files", SIDEBAR_FOOTER),
        InteractionMode::Source => ("source", SOURCE_FOOTER),
    }
}

/// Renders the file navigator with selection, status, and compact change counts.
pub fn render_sidebar(frame: &mut Frame<'_>, area: Rect, stream: &ReviewStream<'_>, app: &App<'_>, theme: &Theme) {
    let total = stream.changeset().files().len();
    let position = app.selected_file().saturating_add(1).min(total);
    let mut lines = vec![sidebar_header(area.width, position, total, theme)];
    let height = usize::from(area.height.saturating_sub(1));
    let end = app
        .sidebar_offset()
        .saturating_add(height)
        .min(stream.changeset().files().len());
    for file in app.sidebar_offset()..end {
        lines.push(sidebar_file_line(
            stream.file(file),
            area.width,
            file == app.selected_file(),
            theme,
        ));
    }
    frame.render_widget(Paragraph::new(lines).style(theme.panel), area);
}

/// Renders the structural boundary between file navigation and review content.
pub fn render_sidebar_divider(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    frame.render_widget(Block::new().borders(Borders::LEFT).border_style(theme.divider), area);
}

/// Renders the complete keyboard and mouse help panel.
pub fn render_help(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let mut lines = vec![fit_line(
        vec![Span::styled(" Navigation and layout", theme.accent)],
        usize::from(area.width),
        theme.file,
    )];
    lines.extend(help_entries().map(|(keys, description)| Line::from(format!(" {keys:<12} {description}"))));
    frame.render_widget(Paragraph::new(lines).style(theme.panel), area);
}

/// Returns addition and deletion counts for a text file, or `None` for a binary file.
pub fn diff_stats(file: &FileDiff) -> Option<(usize, usize)> {
    let FileContent::Text { hunks } = file.content() else {
        return None;
    };
    let mut additions = 0;
    let mut deletions = 0;
    for line in hunks.iter().flat_map(|hunk| hunk.lines()) {
        match line.kind() {
            LineKind::Addition => additions += 1,
            LineKind::Deletion => deletions += 1,
            LineKind::Context => {}
        }
    }
    Some((additions, deletions))
}

/// Returns the display path for a normalized file diff.
pub fn file_path(file: &FileDiff) -> Cow<'_, str> {
    let old = file
        .old_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    let new = file
        .new_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    match (old, new) {
        (Some(old), Some(new)) if old != new => Cow::Owned(format!("{old} -> {new}")),
        (_, Some(new)) => new,
        (Some(old), None) => old,
        (None, None) => Cow::Borrowed("<unknown>"),
    }
}

/// Pads a styled line to the requested width so its background spans the pane.
pub fn fit_line(mut spans: Vec<Span<'static>>, width: usize, style: Style) -> Line<'static> {
    let used = spans_width(&spans);
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    Line::from(spans).style(style)
}

/// Truncates text to terminal display cells and retains an ellipsis when space permits.
pub fn truncate_text(value: &str, width: usize) -> String {
    if Span::raw(value).width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let target = width.saturating_sub(1);
    let mut result = String::new();
    let mut used = 0_usize;
    for character in value.chars() {
        let character_width = Span::raw(character.to_string()).width().max(1);
        if used.saturating_add(character_width) > target {
            break;
        }
        result.push(character);
        used += character_width;
    }
    result.push('…');
    result
}

fn push_title_segment(spans: &mut Vec<Span<'static>>, value: String, style: Style, theme: &Theme) {
    spans.push(Span::styled(" │ ", theme.divider));
    spans.push(Span::styled(value, style));
}

fn truncate_path(value: &str, width: usize) -> String {
    if Span::raw(value).width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let target = width.saturating_sub(1);
    let mut suffix = Vec::new();
    let mut used = 0_usize;
    for character in value.chars().rev() {
        let character_width = Span::raw(character.to_string()).width().max(1);
        if used.saturating_add(character_width) > target {
            break;
        }
        suffix.push(character);
        used += character_width;
    }
    let mut result = String::from("…");
    result.extend(suffix.into_iter().rev());
    result
}

fn sidebar_header(width: u16, position: usize, total: usize, theme: &Theme) -> Line<'static> {
    let left = " Files";
    let right = format!("{position}/{total} ");
    let available = usize::from(width);
    let left_width = Span::raw(left).width();
    let right_width = Span::raw(&right).width();
    let mut spans = vec![Span::styled(left, theme.accent)];
    if left_width.saturating_add(right_width).saturating_add(1) <= available {
        spans.push(Span::raw(" ".repeat(available - left_width - right_width)));
        spans.push(Span::styled(right, theme.muted));
    }
    fit_line(spans, available, theme.panel)
}

fn sidebar_file_line(file: &FileDiff, width: u16, selected: bool, theme: &Theme) -> Line<'static> {
    let available = usize::from(width);
    let mut metadata = Vec::new();
    if let Some((additions, deletions)) = diff_stats(file) {
        if additions > 0 {
            metadata.push(Span::styled(format!("+{additions}"), theme.addition_meta));
        }
        if deletions > 0 {
            if !metadata.is_empty() {
                metadata.push(Span::raw(" "));
            }
            metadata.push(Span::styled(format!("-{deletions}"), theme.deletion_meta));
        }
    }
    let metadata_width = spans_width(&metadata);
    let show_metadata = metadata_width > 0 && available >= metadata_width.saturating_add(13);
    if !show_metadata {
        metadata.clear();
    }
    let metadata_width = spans_width(&metadata);
    let gap_width = usize::from(metadata_width > 0);
    let prefix_width = 4;
    let path_width = available.saturating_sub(prefix_width + gap_width + metadata_width);
    let path = truncate_path(&file_path(file), path_width);
    let used = prefix_width + Span::raw(&path).width() + metadata_width;
    let gap = available.saturating_sub(used);
    let marker_style = if selected { theme.accent } else { theme.muted };
    let mut spans = vec![
        Span::styled(if selected { "▌" } else { " " }, marker_style),
        Span::raw(" "),
        Span::styled(file_status_code(file.status()), theme.muted),
        Span::raw(" "),
        Span::raw(path),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(metadata);
    fit_line(spans, available, if selected { theme.selected } else { theme.panel })
}

fn file_status_code(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "A",
        FileStatus::Deleted => "D",
        FileStatus::Modified => "M",
        FileStatus::Renamed => "R",
        FileStatus::Copied => "C",
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

#[cfg(test)]
mod tests {
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn file_metadata_counts_changes_and_truncates_by_display_width() {
        let patch = b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();

        assert_eq!(diff_stats(&changeset.files()[0]), Some((1, 1)));
        assert_eq!(truncate_text("界abcdef", 5), "界ab…");
        assert_eq!(Span::raw(truncate_text("界abcdef", 5)).width(), 5);
        assert_eq!(truncate_path("crates/ui/src/app.rs", 12), "…/src/app.rs");
        assert_eq!(Span::raw(truncate_path("目录/界abcdef.rs", 8)).width(), 8);
    }

    #[test]
    fn selected_sidebar_row_fills_the_pane_and_preserves_the_path() {
        let patch = b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(64, 12);
        let areas = app.areas();
        let theme = Theme::dark();
        let backend = TestBackend::new(64, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| crate::view::render(frame, &app, &theme)).unwrap();

        let row = areas.sidebar.y + 1;
        let buffer = terminal.backend().buffer();
        let text = (areas.sidebar.x..areas.sidebar.right())
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>();
        assert!(text.contains("file.txt"));
        for x in areas.sidebar.x..areas.sidebar.right() {
            assert_eq!(buffer[(x, row)].style().bg, theme.selected.bg);
        }
    }
}
