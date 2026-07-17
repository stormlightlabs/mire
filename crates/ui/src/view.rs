use std::borrow::Cow;

use mire_core::{FileDiff, FileStatus, LineKind, MissingNewline};
use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, AppState};
use crate::navigation::{Focus, help_entries};
use crate::stream::TextRange;
use crate::stream::{ReviewStream, RowKey};
use crate::theme::Theme;

const FOOTER: &str =
    " Tab focus | j/k move | [ ] files | { } hunks | / search | +/- context | w wrap | ? help | q quit ";

#[derive(Clone, Copy)]
struct SourcePosition {
    file: usize,
    hunk: usize,
    line: usize,
}

#[derive(Clone, Copy)]
struct SplitSource {
    line: Option<usize>,
    range: Option<TextRange>,
}

struct RenderContext<'stream, 'changeset, 'app, 'theme> {
    stream: &'stream ReviewStream<'changeset>,
    app: &'app App<'changeset>,
    theme: &'theme Theme,
}

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
    let search = if app.search_input() {
        format!(" | /{}█", app.search_query())
    } else if let Some((current, total)) = app.search_status() {
        format!(" | /{} ({current}/{total})", app.search_query())
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(format!(
            " Mire review | {layout} | focus: {focus} | context: {} | wrap: {}{search} ",
            app.context_lines(),
            if app.wrap_lines() { "on" } else { "off" }
        ))
        .style(theme.title),
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
                render_stream(frame, areas.review, stream, app, theme);
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
    frame: &mut Frame<'_>, area: ratatui::layout::Rect, stream: &ReviewStream<'_>, app: &App<'_>, theme: &Theme,
) {
    let visible = stream
        .visible_keys(app.scroll(), usize::from(area.height))
        .map(|key| row_line(stream, key, area.width, app, theme))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
}

fn render_help(frame: &mut Frame<'_>, area: ratatui::layout::Rect, theme: &Theme) {
    let mut lines = vec![Line::styled(" Navigation and layout", theme.file)];
    lines.extend(help_entries().map(|(keys, description)| Line::from(format!(" {keys:<12} {description}"))));
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line<'a>(stream: &'a ReviewStream<'a>, key: RowKey, width: u16, app: &App<'a>, theme: &Theme) -> Line<'static> {
    let context = RenderContext { stream, app, theme };
    match key {
        RowKey::File { file } => file_line(stream.file(file), theme.file),
        RowKey::Binary { .. } => Line::styled("     Binary files differ", theme.marker),
        RowKey::Hunk { file, hunk } => hunk_line(stream, file, hunk, theme),
        RowKey::UnifiedLine { file, hunk, line, range } => {
            context.unified_line(SourcePosition { file, hunk, line }, range)
        }
        RowKey::SplitLine { file, hunk, old, new, old_range, new_range } => context.split_line(
            file,
            hunk,
            SplitSource { line: old, range: old_range },
            SplitSource { line: new, range: new_range },
            width,
        ),
        RowKey::MissingNewline { file, hunk, line } => {
            let marker = stream.hunk(file, hunk).lines()[line].missing_newline();
            Line::styled(
                format!("            \\ No newline at end of {}", missing_side(marker)),
                theme.marker,
            )
        }
        RowKey::ContextGap { hidden, .. } => {
            Line::styled(format!("            ⋯ {hidden} context lines hidden"), theme.marker)
        }
    }
}

fn hunk_line(stream: &ReviewStream<'_>, file: usize, hunk: usize, theme: &Theme) -> Line<'static> {
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

impl RenderContext<'_, '_, '_, '_> {
    fn unified_line(&self, position: SourcePosition, range: TextRange) -> Line<'static> {
        let source_line = &self.stream.hunk(position.file, position.hunk).lines()[position.line];
        let old = source_line
            .old_line()
            .map_or_else(String::new, |number| number.get().to_string());
        let new = source_line
            .new_line()
            .map_or_else(String::new, |number| number.get().to_string());
        let (prefix, style) = match source_line.kind() {
            LineKind::Context => (' ', self.theme.context),
            LineKind::Addition => ('+', self.theme.addition),
            LineKind::Deletion => ('-', self.theme.deletion),
        };
        let gutter =
            if range.start == 0 { format!("{old:>5} {new:>5} {prefix}") } else { "            ↪".to_owned() };
        let counterpart = paired_line(self.stream, position.file, position.hunk, position.line);
        let mut spans = vec![Span::styled(gutter, style)];
        spans.extend(self.styled_source(position, range, counterpart, style));
        Line::from(spans)
    }

    fn split_line(&self, file: usize, hunk: usize, old: SplitSource, new: SplitSource, width: u16) -> Line<'static> {
        let divider_width = 3_usize.min(usize::from(width));
        let available = usize::from(width).saturating_sub(divider_width);
        let old_width = available / 2;
        let new_width = available.saturating_sub(old_width);
        let mut spans = self.split_cell(file, hunk, old, new.line, true, old_width);
        spans.push(Span::styled(" │ ", self.theme.divider));
        spans.extend(self.split_cell(file, hunk, new, old.line, false, new_width));
        Line::from(spans)
    }

    fn split_cell(
        &self, file: usize, hunk: usize, source: SplitSource, counterpart: Option<usize>, old_side: bool, width: usize,
    ) -> Vec<Span<'static>> {
        let (Some(index), Some(range)) = (source.line, source.range) else {
            return vec![Span::raw(" ".repeat(width))];
        };
        let line = &self.stream.hunk(file, hunk).lines()[index];
        let number = if old_side { line.old_line() } else { line.new_line() }
            .map_or_else(String::new, |value| value.get().to_string());
        let (prefix, style) = match line.kind() {
            LineKind::Context => (' ', self.theme.context),
            LineKind::Addition => ('+', self.theme.addition),
            LineKind::Deletion => ('-', self.theme.deletion),
        };
        let gutter = if range.start == 0 { format!("{number:>5} {prefix}") } else { "      ↪".to_owned() };
        let gutter_width = Span::raw(&gutter).width();
        let mut spans = vec![Span::styled(gutter, style)];
        let source_spans = self.styled_source(SourcePosition { file, hunk, line: index }, range, counterpart, style);
        spans.extend(clip_spans(source_spans, width.saturating_sub(gutter_width)));
        let used = gutter_width + spans.iter().skip(1).map(Span::width).sum::<usize>();
        spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
        spans
    }

    fn styled_source(
        &self, position: SourcePosition, range: TextRange, counterpart: Option<usize>, base: Style,
    ) -> Vec<Span<'static>> {
        let source =
            String::from_utf8_lossy(self.stream.hunk(position.file, position.hunk).lines()[position.line].content());
        let end = range.end.min(source.len());
        let start = range.start.min(end);
        let syntax = self.app.syntax_ranges(position.file, position.hunk, position.line);
        let intraline = counterpart
            .filter(|_| source.len() <= 32 * 1024)
            .map(|other| changed_ranges(self.stream, position.file, position.hunk, position.line, other))
            .unwrap_or_default();
        let search = match_ranges(&source, self.app.search_query());
        let mut boundaries = vec![start, end];
        for (range_start, range_end) in syntax
            .iter()
            .map(|range| (range.0, range.1))
            .chain(intraline.iter().copied())
            .chain(search.iter().copied())
        {
            if range_start > start && range_start < end {
                boundaries.push(range_start);
            }
            if range_end > start && range_end < end {
                boundaries.push(range_end);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        boundaries
            .windows(2)
            .filter_map(|bounds| {
                let segment = source.get(bounds[0]..bounds[1])?;
                let mut style = syntax
                    .iter()
                    .rev()
                    .find(|range| bounds[0] >= range.0 && bounds[0] < range.1)
                    .map_or(base, |range| self.theme.syntax(range.2, base));
                if intraline
                    .iter()
                    .any(|(start, end)| bounds[0] >= *start && bounds[0] < *end)
                {
                    style = style.patch(self.theme.intraline);
                }
                if search
                    .iter()
                    .any(|(start, end)| bounds[0] >= *start && bounds[0] < *end)
                {
                    style = self.theme.search_match;
                }
                Some(Span::styled(segment.to_owned(), style))
            })
            .collect()
    }
}

fn clip_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let mut clipped = Vec::new();
    let mut remaining = width;
    for span in spans {
        if remaining == 0 {
            break;
        }
        if span.width() <= remaining {
            remaining -= span.width();
            clipped.push(span);
            continue;
        }
        let mut content = String::new();
        for character in span.content.chars() {
            let character_width = Span::raw(character.to_string()).width().max(1);
            if character_width > remaining {
                break;
            }
            content.push(character);
            remaining -= character_width;
        }
        clipped.push(Span::styled(content, span.style));
    }
    clipped
}

fn changed_ranges(
    stream: &ReviewStream<'_>, file: usize, hunk: usize, line: usize, counterpart: usize,
) -> Vec<(usize, usize)> {
    use similar::{Algorithm, ChangeTag};

    let lines = stream.hunk(file, hunk).lines();
    let selected = String::from_utf8_lossy(lines[line].content());
    let other = String::from_utf8_lossy(lines[counterpart].content());
    let (old, new, selected_tag) = if matches!(lines[line].kind(), LineKind::Deletion) {
        (&*selected, &*other, ChangeTag::Delete)
    } else {
        (&*other, &*selected, ChangeTag::Insert)
    };
    let mut offset = 0;
    let mut ranges = Vec::new();
    for (tag, value) in similar::utils::diff_words(Algorithm::Myers, old, new) {
        if tag == selected_tag {
            ranges.push((offset, offset + value.len()));
            offset += value.len();
        } else if matches!(tag, ChangeTag::Equal) {
            offset += value.len();
        }
    }
    ranges
}

fn paired_line(stream: &ReviewStream<'_>, file: usize, hunk: usize, line: usize) -> Option<usize> {
    let lines = stream.hunk(file, hunk).lines();
    if matches!(lines[line].kind(), LineKind::Context) {
        return None;
    }
    let start = (0..line)
        .rev()
        .find(|index| matches!(lines[*index].kind(), LineKind::Context))
        .map_or(0, |index| index + 1);
    let end = (line + 1..lines.len())
        .find(|index| matches!(lines[*index].kind(), LineKind::Context))
        .unwrap_or(lines.len());
    let deletions = (start..end)
        .filter(|index| matches!(lines[*index].kind(), LineKind::Deletion))
        .collect::<Vec<_>>();
    let additions = (start..end)
        .filter(|index| matches!(lines[*index].kind(), LineKind::Addition))
        .collect::<Vec<_>>();
    match lines[line].kind() {
        LineKind::Deletion => deletions
            .iter()
            .position(|index| *index == line)
            .and_then(|position| additions.get(position).copied()),
        LineKind::Addition => additions
            .iter()
            .position(|index| *index == line)
            .and_then(|position| deletions.get(position).copied()),
        LineKind::Context => None,
    }
}

fn match_ranges(source: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let (haystack, needle) = if source.is_ascii() && query.is_ascii() {
        (
            Cow::Owned(source.to_ascii_lowercase()),
            Cow::Owned(query.to_ascii_lowercase()),
        )
    } else {
        (Cow::Borrowed(source), Cow::Borrowed(query))
    };
    haystack
        .match_indices(&*needle)
        .map(|(start, value)| (start, start + value.len()))
        .collect()
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
    fn intraline_ranges_and_split_clipping_keep_changed_text_readable() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let stream = ReviewStream::new(&changeset, crate::stream::ResolvedLayout::Split);
        let ranges = changed_ranges(&stream, 0, 0, 0, 1);
        assert!(!ranges.is_empty());

        let clipped = clip_spans(vec![Span::styled("界abc", Theme::dark().addition)], 3);
        assert_eq!(clipped.iter().map(Span::width).sum::<usize>(), 3);
        assert_eq!(clipped[0].content, "界a");
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
