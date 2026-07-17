use std::borrow::Cow;

use mire_core::{FileDiff, FileStatus, LineKind, MissingNewline};
use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, AppState};
use crate::navigation::{Focus, help_entries};
use crate::stream::{ReviewStream, RowKey};
use crate::theme::Theme;

const FOOTER: &str = " Tab: focus | j/k: move | [ ]: files | n/N: hunks | 1/2/3: layout | ?: help | q: quit ";

/// Renders the application into a Ratatui frame.
pub fn render(frame: &mut Frame<'_>, app: &App<'_>, theme: &Theme) {
    let areas = app.areas();
    let layout = match app.state() {
        AppState::Ready(stream) => stream.layout().label(),
        AppState::Loading | AppState::Empty | AppState::Error(_) => "unavailable",
    };
    let focus = match app.focus() {
        Focus::Review => "review",
        Focus::Sidebar => "files",
    };
    frame.render_widget(
        Paragraph::new(format!(" Mire review | {layout} | focus: {focus} ")).style(theme.title),
        areas.title,
    );

    match app.state() {
        AppState::Loading => frame.render_widget(Paragraph::new("Loading review..."), areas.review),
        AppState::Empty => frame.render_widget(Paragraph::new("No changes to review."), areas.review),
        AppState::Ready(stream) => {
            render_sidebar(frame, areas.sidebar, stream, app, theme);
            if app.help_visible() {
                render_help(frame, areas.review, theme);
            } else {
                render_stream(frame, areas.review, stream, app.scroll(), theme);
            }
        }
        AppState::Error(message) => {
            frame.render_widget(
                Paragraph::new(format!("Unable to display review:\n{message}")).style(theme.error),
                areas.review,
            );
        }
    }
    frame.render_widget(Paragraph::new(FOOTER).style(theme.footer), areas.footer);
}

fn render_sidebar(
    frame: &mut Frame<'_>, area: ratatui::layout::Rect, stream: &ReviewStream<'_>, app: &App<'_>, theme: &Theme,
) {
    let mut lines = vec![Line::styled(" Files", theme.file)];
    let height = usize::from(area.height.saturating_sub(1));
    let end = app
        .sidebar_offset()
        .saturating_add(height)
        .min(stream.changeset().files().len());
    for file in app.sidebar_offset()..end {
        let marker = if file == app.selected_file() { ">" } else { " " };
        let style = if file == app.selected_file() { theme.selected } else { theme.context };
        lines.push(Line::styled(
            format!("{marker} {}", file_path(stream.file(file))),
            style,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_stream(
    frame: &mut Frame<'_>, area: ratatui::layout::Rect, stream: &ReviewStream<'_>, offset: usize, theme: &Theme,
) {
    let visible = stream
        .visible_keys(offset, usize::from(area.height))
        .map(|key| row_line(stream, key, area.width, theme))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
}

fn render_help(frame: &mut Frame<'_>, area: ratatui::layout::Rect, theme: &Theme) {
    let mut lines = vec![Line::styled(" Navigation and layout", theme.file)];
    lines.extend(help_entries().map(|(keys, description)| Line::from(format!(" {keys:<12} {description}"))));
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line<'a>(stream: &'a ReviewStream<'a>, key: RowKey, width: u16, theme: &Theme) -> Line<'a> {
    match key {
        RowKey::File { file } => file_line(stream.file(file), theme.file),
        RowKey::Binary { .. } => Line::styled("     Binary files differ", theme.marker),
        RowKey::Hunk { file, hunk } => hunk_line(stream, file, hunk, theme),
        RowKey::UnifiedLine { file, hunk, line } => unified_line(stream, file, hunk, line, theme),
        RowKey::SplitLine { file, hunk, old, new } => split_line(stream, file, hunk, old, new, width, theme),
        RowKey::MissingNewline { file, hunk, line } => {
            let marker = stream.hunk(file, hunk).lines()[line].missing_newline();
            Line::styled(
                format!("            \\ No newline at end of {}", missing_side(marker)),
                theme.marker,
            )
        }
    }
}

fn hunk_line<'a>(stream: &'a ReviewStream<'a>, file: usize, hunk: usize, theme: &Theme) -> Line<'a> {
    let hunk = stream.hunk(file, hunk);
    let section = String::from_utf8_lossy(hunk.section());
    Line::styled(
        format!(
            "@@ -{},{} +{},{} @@{}{}",
            hunk.old_start(),
            hunk.old_line_count(),
            hunk.new_start(),
            hunk.new_line_count(),
            if section.is_empty() { "" } else { " " },
            section
        ),
        theme.hunk,
    )
}

fn unified_line<'a>(stream: &'a ReviewStream<'a>, file: usize, hunk: usize, line: usize, theme: &Theme) -> Line<'a> {
    let line = &stream.hunk(file, hunk).lines()[line];
    let old = line
        .old_line()
        .map_or_else(String::new, |number| number.get().to_string());
    let new = line
        .new_line()
        .map_or_else(String::new, |number| number.get().to_string());
    let (prefix, style) = match line.kind() {
        LineKind::Context => (' ', theme.context),
        LineKind::Addition => ('+', theme.addition),
        LineKind::Deletion => ('-', theme.deletion),
    };
    let content = String::from_utf8_lossy(line.content());
    Line::from(vec![
        Span::styled(format!("{old:>5} {new:>5} {prefix}"), style),
        Span::styled(content, style),
    ])
}

fn split_line<'a>(
    stream: &'a ReviewStream<'a>, file: usize, hunk: usize, old: Option<usize>, new: Option<usize>, width: u16,
    theme: &Theme,
) -> Line<'a> {
    let divider_width = 3_usize.min(usize::from(width));
    let available = usize::from(width).saturating_sub(divider_width);
    let old_width = available / 2;
    let new_width = available.saturating_sub(old_width);
    let old = split_cell(stream, file, hunk, old, true, old_width, theme);
    let new = split_cell(stream, file, hunk, new, false, new_width, theme);
    Line::from(vec![old, Span::styled(" │ ", theme.divider), new])
}

fn split_cell<'a>(
    stream: &'a ReviewStream<'a>, file: usize, hunk: usize, index: Option<usize>, old_side: bool, width: usize,
    theme: &Theme,
) -> Span<'a> {
    let Some(index) = index else {
        return Span::raw(" ".repeat(width));
    };
    let line = &stream.hunk(file, hunk).lines()[index];
    let number = if old_side { line.old_line() } else { line.new_line() }
        .map_or_else(String::new, |value| value.get().to_string());
    let (prefix, style) = match line.kind() {
        LineKind::Context => (' ', theme.context),
        LineKind::Addition => ('+', theme.addition),
        LineKind::Deletion => ('-', theme.deletion),
    };
    let content = String::from_utf8_lossy(line.content());
    Span::styled(fit_to_width(&format!("{number:>5} {prefix}{content}"), width), style)
}

fn file_line(file: &FileDiff, style: Style) -> Line<'static> {
    let status = match file.status() {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Modified => "modified",
        FileStatus::Renamed => "renamed",
        FileStatus::Copied => "copied",
    };
    Line::styled(format!("--- {status}: {} ", file_path(file)), style)
}

fn file_path(file: &FileDiff) -> Cow<'_, str> {
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

fn fit_to_width(value: &str, width: usize) -> String {
    let mut result = String::new();
    let mut display_width = 0_usize;
    for character in value.chars() {
        let mut encoded = [0_u8; 4];
        let encoded = character.encode_utf8(&mut encoded);
        let character_width = Span::raw(&*encoded).width();
        if display_width.saturating_add(character_width) > width {
            break;
        }
        result.push(character);
        display_width = display_width.saturating_add(character_width);
    }
    result.extend(std::iter::repeat_n(' ', width.saturating_sub(display_width)));
    result
}

const fn missing_side(marker: MissingNewline) -> &'static str {
    match marker {
        MissingNewline::None => "file",
        MissingNewline::Old => "old file",
        MissingNewline::New => "new file",
        MissingNewline::Both => "both files",
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    const PATCH: &[u8] = b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@ heading\n-old\n+new\n context\n\\ No newline at end of file\ndiff --git a/image.bin b/image.bin\nBinary files a/image.bin and b/image.bin differ\n";

    #[test]
    fn state_snapshots_are_deterministic() {
        insta::assert_snapshot!("state_loading_32x5", snapshot(App::loading(), 32, 5));
        insta::assert_snapshot!("state_error_32x6", snapshot(App::error("broken input"), 32, 6));

        let empty = parse_patch(b"", ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        insta::assert_snapshot!("state_empty_32x5", snapshot(App::ready(&empty), 32, 5));
    }

    #[test]
    fn layout_ready_snapshots_cover_narrow_unified_and_wide_split_terminals() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();

        insta::assert_snapshot!("ready_narrow_24x9", snapshot(App::ready(&changeset), 24, 9));
        insta::assert_snapshot!("ready_ordinary_64x12", snapshot(App::ready(&changeset), 64, 12));
        insta::assert_snapshot!("ready_wide_120x12", snapshot(App::ready(&changeset), 120, 12));
    }

    #[test]
    fn layout_help_snapshot_lists_all_active_bindings() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        insta::assert_snapshot!("help_90x24", snapshot(app, 90, 24));
    }

    #[test]
    fn layout_split_cells_use_terminal_display_width() {
        assert_eq!(fit_to_width("界x", 2), "界");
        assert_eq!(fit_to_width("x", 3), "x  ");
    }

    fn snapshot(mut app: App<'_>, width: u16, height: u16) -> String {
        app.resize(width, height);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app, &Theme::plain())).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
