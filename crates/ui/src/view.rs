use std::borrow::Cow;

use mire_core::{FileDiff, FileStatus, LineKind, MissingNewline};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, AppState};
use crate::stream::{ReviewStream, RowKey};
use crate::theme::Theme;

const TITLE: &str = " Mire review ";
const FOOTER: &str = " j/k or arrows: scroll | PgUp/PgDn | Home/End | q: quit ";

/// Renders the application into a Ratatui frame.
pub fn render(frame: &mut Frame<'_>, app: &App<'_>, theme: &Theme) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    frame.render_widget(Paragraph::new(TITLE).style(theme.title), areas[0]);
    match app.state() {
        AppState::Loading => frame.render_widget(Paragraph::new("Loading review..."), areas[1]),
        AppState::Empty => frame.render_widget(Paragraph::new("No changes to review."), areas[1]),
        AppState::Ready(stream) => render_stream(frame, areas[1], stream, app.scroll(), theme),
        AppState::Error(message) => {
            frame.render_widget(
                Paragraph::new(format!("Unable to display review:\n{message}")).style(theme.error),
                areas[1],
            );
        }
    }
    frame.render_widget(Paragraph::new(FOOTER).style(theme.footer), areas[2]);
}

fn render_stream(
    frame: &mut Frame<'_>, area: ratatui::layout::Rect, stream: &ReviewStream<'_>, offset: usize, theme: &Theme,
) {
    let visible = stream
        .visible_keys(offset, usize::from(area.height))
        .map(|key| row_line(stream, key, theme))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
}

fn row_line<'a>(stream: &'a ReviewStream<'a>, key: RowKey, theme: &Theme) -> Line<'a> {
    match key {
        RowKey::File { file } => file_line(stream.file(file), theme.file),
        RowKey::Binary { .. } => Line::styled("     Binary files differ", theme.marker),
        RowKey::Hunk { file, hunk } => {
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
        RowKey::Line { file, hunk, line } => {
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
        RowKey::MissingNewline { file, hunk, line } => {
            let marker = stream.hunk(file, hunk).lines()[line].missing_newline();
            Line::styled(
                format!("            \\ No newline at end of {}", missing_side(marker)),
                theme.marker,
            )
        }
    }
}

fn file_line(file: &FileDiff, style: Style) -> Line<'static> {
    let status = match file.status() {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Modified => "modified",
        FileStatus::Renamed => "renamed",
        FileStatus::Copied => "copied",
    };
    let old = file
        .old_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    let new = file
        .new_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    let path = match (old, new) {
        (Some(old), Some(new)) if old != new => Cow::Owned(format!("{old} -> {new}")),
        (_, Some(new)) => new,
        (Some(old), None) => old,
        (None, None) => Cow::Borrowed("<unknown>"),
    };
    Line::styled(format!("--- {status}: {path} "), style)
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
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    const PATCH: &[u8] = b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@ heading\n-old\n+new\n context\n\\ No newline at end of file\ndiff --git a/image.bin b/image.bin\nBinary files a/image.bin and b/image.bin differ\n";

    #[test]
    fn state_snapshots_are_deterministic() {
        insta::assert_snapshot!("state_loading_32x5", snapshot(&App::loading(), 32, 5));
        insta::assert_snapshot!("state_error_32x6", snapshot(&App::error("broken input"), 32, 6));

        let empty = parse_patch(b"", ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        insta::assert_snapshot!("state_empty_32x5", snapshot(&App::ready(&empty), 32, 5));
    }

    #[test]
    fn ready_snapshots_cover_narrow_ordinary_and_wide_terminals() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let app = App::ready(&changeset);

        insta::assert_snapshot!("ready_narrow_24x9", snapshot(&app, 24, 9));
        insta::assert_snapshot!("ready_ordinary_64x12", snapshot(&app, 64, 12));
        insta::assert_snapshot!("ready_wide_120x12", snapshot(&app, 120, 12));
    }

    fn snapshot(app: &App<'_>, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app, &Theme::plain())).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
