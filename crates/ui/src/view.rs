use std::borrow::Cow;

use mire_core::{AnchorSide, FileDiff, FileStatus, LineKind, MissingNewline};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::EditorTarget;
use crate::app::{App, AppState, GutterMark};
use crate::chrome::{
    diff_stats, file_path, fit_line, render_footer, render_help, render_sidebar, render_sidebar_divider, render_title,
    truncate_text,
};
use crate::stream::TextRange;
use crate::stream::{ReviewStream, RowKey};
use crate::theme::Theme;

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
    frame.render_widget(Block::new().style(theme.application), frame.area());
    render_title(frame, areas.title, app, theme);

    match app.state() {
        AppState::Loading => {
            frame.render_widget(
                Paragraph::new("  Loading review...").style(theme.application),
                areas.body,
            );
        }
        AppState::Empty => {
            frame.render_widget(
                Paragraph::new("  No changes to review.").style(theme.application),
                areas.body,
            );
        }
        AppState::Ready(stream) => {
            if app.sidebar_visible() {
                render_sidebar(frame, areas.sidebar, stream, app, theme);
                render_sidebar_divider(frame, areas.sidebar_divider, theme);
            }
            if app.help_visible() {
                render_help(frame, areas.review, theme);
            } else {
                render_stream(frame, areas.review, stream, app, theme);
            }
        }
        AppState::Error(message) => {
            frame.render_widget(
                Paragraph::new(format!("  Unable to display review:\n  {message}")).style(theme.error),
                areas.body,
            );
        }
    }
    if app.filter_visible() {
        render_filter_dialog(frame, areas.review, app, theme);
    }
    if app.editor().is_some() {
        render_note_editor(frame, areas.review, app, theme);
    }
    render_footer(frame, areas.footer, app, theme);
}

fn render_stream(
    frame: &mut Frame<'_>, area: ratatui::layout::Rect, stream: &ReviewStream<'_>, app: &App<'_>, theme: &Theme,
) {
    let show_scroll = stream.len() > usize::from(area.height) && area.width > 1;
    let content_area = if show_scroll {
        Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height)
    } else {
        area
    };
    let visible = stream
        .visible_keys(app.scroll(), usize::from(content_area.height))
        .enumerate()
        .map(|(offset, key)| {
            let selected = app.row_selected(app.scroll().saturating_add(offset));
            let line = row_line(stream, key, content_area.width, selected, app, theme);
            if selected { line.patch_style(theme.selected) } else { line }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), content_area);
    if show_scroll {
        render_scroll_indicator(frame, area, stream.len(), app.scroll(), theme);
    }
}

fn render_scroll_indicator(frame: &mut Frame<'_>, area: Rect, total_rows: usize, offset: usize, theme: &Theme) {
    let height = usize::from(area.height);
    let thumb_height = height
        .saturating_mul(height)
        .saturating_add(total_rows.saturating_sub(1))
        .checked_div(total_rows)
        .unwrap_or(1)
        .clamp(1, height);
    let travel = height.saturating_sub(thumb_height);
    let maximum_offset = total_rows.saturating_sub(height);
    let thumb_start = if maximum_offset == 0 {
        0
    } else {
        offset.min(maximum_offset).saturating_mul(travel) / maximum_offset
    };
    let lines = (0..height)
        .map(|row| {
            Line::styled(
                if row >= thumb_start && row < thumb_start + thumb_height { "█" } else { "│" },
                theme.muted,
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(area.right().saturating_sub(1), area.y, 1, area.height),
    );
}

fn row_line<'a>(
    stream: &'a ReviewStream<'a>, key: RowKey, width: u16, selected: bool, app: &App<'a>, theme: &Theme,
) -> Line<'static> {
    let context = RenderContext { stream, app, theme };
    match key {
        RowKey::File { file } => file_line(stream.file(file), width, theme),
        RowKey::Binary { .. } => Line::styled("     Binary files differ", theme.marker),
        RowKey::Hunk { file, hunk } => hunk_line(stream, file, hunk, width, theme),
        RowKey::UnifiedLine { file, hunk, line, range } => {
            context.unified_line(SourcePosition { file, hunk, line }, range, width)
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
        RowKey::Note { note, .. } => note_line(app, note, width, selected, theme),
    }
}

fn note_line(app: &App<'_>, note_index: usize, width: u16, selected: bool, theme: &Theme) -> Line<'static> {
    const ACTIONS: &str = " [e][o][r][d][a]";

    let Some(note) = app.note(note_index) else {
        return Line::styled("  unavailable finding", theme.error);
    };
    let author = note.author().display_name().unwrap_or_else(|| note.author().id());
    let finding = format!(
        " {} {} {} · {author}: {}",
        note.status(),
        note.severity(),
        note.annotation_kind(),
        note.body()
    );
    let action_width = if selected { Span::raw(ACTIONS).width() } else { 0 };
    let finding_width = usize::from(width).saturating_sub(action_width);
    let mut spans = vec![Span::styled(truncate_text(&finding, finding_width), theme.panel)];
    if selected {
        spans.push(Span::styled(ACTIONS, theme.accent));
    }
    fit_line(spans, usize::from(width), theme.panel)
}

fn render_note_editor(frame: &mut Frame<'_>, area: Rect, app: &App<'_>, theme: &Theme) {
    let Some(editor) = app.editor() else {
        return;
    };
    let target = match editor.target() {
        EditorTarget::New(selection) => format!("New note · {}", app.new_note_location(*selection)),
        EditorTarget::Existing(note_id) => format!("Edit {}", note_id.as_str()),
    };
    let body_label = if editor.focused_field() == crate::EditorField::Body { "› Body" } else { "  Body" };
    let severity_label =
        if editor.focused_field() == crate::EditorField::Severity { "› Severity" } else { "  Severity" };
    let kind_label = if editor.focused_field() == crate::EditorField::Kind { "› Kind" } else { "  Kind" };
    let body_lines = editor.body().lines().count().max(1);
    let dialog = centered(
        area,
        72,
        (body_lines.saturating_add(8)).min(usize::from(area.height)) as u16,
    );
    frame.render_widget(Clear, dialog);
    let error = app
        .note_error()
        .map_or(String::new(), |error| format!("\nError: {error}"));
    let cursor = if editor.focused_field() == crate::EditorField::Body { "█" } else { "" };
    let text = format!(
        "{target}\n{body_label}:\n{}{cursor}\n{severity_label}: {}\n{kind_label}: {}{error}\n\nEnter newline · Ctrl-Enter save · Tab focus · ↑↓ change field · Esc cancel",
        editor.body(),
        editor.severity(),
        editor.annotation_kind(),
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(theme.panel)
            .block(Block::new().borders(Borders::ALL).border_style(theme.accent)),
        dialog,
    );
}

fn render_filter_dialog(frame: &mut Frame<'_>, area: Rect, app: &App<'_>, theme: &Theme) {
    let dialog = centered(area, 72, 5);
    frame.render_widget(Clear, dialog);
    let text = format!(
        "Note filters\n{}\n\na author · s status · v severity · k kind · i file · c clear · Enter close",
        app.filter_summary()
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(theme.panel)
            .block(Block::new().borders(Borders::ALL).border_style(theme.accent)),
        dialog,
    );
}

fn centered(area: Rect, maximum_width: u16, height: u16) -> Rect {
    let width = area.width.min(maximum_width).max(1);
    let height = area.height.min(height).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn hunk_line(stream: &ReviewStream<'_>, file: usize, hunk: usize, width: u16, theme: &Theme) -> Line<'static> {
    let hunk = stream.hunk(file, hunk);
    let section = String::from_utf8_lossy(hunk.section());
    let prefix = if width >= 32 { " ▾ " } else { "" };
    fit_line(
        vec![Span::styled(
            format!(
                "{prefix}@@ -{},{} +{},{} @@{}{}",
                hunk.old_start(),
                hunk.old_line_count(),
                hunk.new_start(),
                hunk.new_line_count(),
                if section.is_empty() { "" } else { " " },
                section
            ),
            theme.hunk,
        )],
        usize::from(width),
        theme.hunk,
    )
}

impl RenderContext<'_, '_, '_, '_> {
    fn unified_line(&self, position: SourcePosition, range: TextRange, width: u16) -> Line<'static> {
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
        let gutter = if range.start == 0 {
            format!(
                "{old:>5} {new:>5} {}",
                gutter_symbol(
                    &[
                        self.app
                            .gutter_mark(position.file, position.hunk, position.line, AnchorSide::Old),
                        self.app
                            .gutter_mark(position.file, position.hunk, position.line, AnchorSide::New),
                    ],
                    prefix,
                )
            )
        } else {
            "            ↪".to_owned()
        };
        let counterpart = paired_line(self.stream, position.file, position.hunk, position.line);
        let mut spans = vec![Span::styled(gutter, style)];
        spans.extend(self.styled_source(position, range, counterpart, style));
        fit_line(spans, usize::from(width), style)
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
        let gutter = if range.start == 0 {
            let side = if old_side { AnchorSide::Old } else { AnchorSide::New };
            format!(
                "{number:>5} {}",
                gutter_symbol(&[self.app.gutter_mark(file, hunk, index, side)], prefix)
            )
        } else {
            "      ↪".to_owned()
        };
        let gutter_width = Span::raw(&gutter).width();
        let mut spans = vec![Span::styled(gutter, style)];
        let source_spans = self.styled_source(SourcePosition { file, hunk, line: index }, range, counterpart, style);
        spans.extend(clip_spans(source_spans, width.saturating_sub(gutter_width)));
        let used = gutter_width + spans.iter().skip(1).map(Span::width).sum::<usize>();
        spans.push(Span::styled(" ".repeat(width.saturating_sub(used)), style));
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

fn gutter_symbol(marks: &[GutterMark], fallback: char) -> char {
    if marks.contains(&GutterMark::Cursor) {
        '>'
    } else if marks.contains(&GutterMark::Selection) {
        '▌'
    } else if marks.contains(&GutterMark::Finding) {
        '◆'
    } else {
        fallback
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

fn file_line(file: &FileDiff, width: u16, theme: &Theme) -> Line<'static> {
    let available = usize::from(width);
    let mut metadata = vec![Span::styled(file_status_label(file.status()), theme.muted)];
    if let Some((additions, deletions)) = diff_stats(file) {
        if additions > 0 {
            metadata.push(Span::raw(" "));
            metadata.push(Span::styled(format!("+{additions}"), theme.addition_meta));
        }
        if deletions > 0 {
            metadata.push(Span::raw(" "));
            metadata.push(Span::styled(format!("-{deletions}"), theme.deletion_meta));
        }
    }
    if available < 32 {
        metadata.clear();
    }
    let metadata_width = metadata.iter().map(Span::width).sum::<usize>();
    let path_width = available.saturating_sub(metadata_width.saturating_add(3));
    let path = truncate_text(&file_path(file), path_width);
    let path_width = Span::raw(&path).width();
    let gap = available.saturating_sub(path_width + metadata_width + 2);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(path, theme.accent),
        Span::raw(" ".repeat(gap.max(1))),
    ];
    spans.extend(metadata);
    fit_line(spans, available, theme.file)
}

fn file_status_label(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Modified => "modified",
        FileStatus::Renamed => "renamed",
        FileStatus::Copied => "copied",
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
    use ratatui::style::Modifier;

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
    fn new_note_editor_shows_the_side_qualified_source_location() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let review = mire_core::Review::new(
            mire_core::ReviewRevision::new(1).unwrap(),
            changeset,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let mut app = App::review_with_options(&review, crate::AppOptions::default());
        app.resize(100, 12);
        for _ in 0..3 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));

        assert!(snapshot(app, 100, 12).contains("New note · b/file.txt:2-2"));
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

    #[test]
    fn changed_backgrounds_fill_unified_rows_and_split_cells() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let theme = Theme::dark();

        let mut unified = App::ready(&changeset);
        unified.resize(64, 12);
        let unified_area = unified.areas().review;
        let mut terminal = Terminal::new(TestBackend::new(64, 12)).unwrap();
        terminal.draw(|frame| render(frame, &unified, &theme)).unwrap();
        let buffer = terminal.backend().buffer();
        for x in unified_area.x..unified_area.right() {
            assert!([theme.deletion.bg, theme.intraline.bg].contains(&buffer[(x, unified_area.y + 2)].style().bg));
            assert!([theme.addition.bg, theme.intraline.bg].contains(&buffer[(x, unified_area.y + 3)].style().bg));
        }
        assert_eq!(
            buffer[(unified_area.right() - 1, unified_area.y + 2)].style().bg,
            theme.deletion.bg
        );
        assert_eq!(
            buffer[(unified_area.right() - 1, unified_area.y + 3)].style().bg,
            theme.addition.bg
        );

        let mut split = App::ready(&changeset);
        split.resize(120, 12);
        let split_area = split.areas().review;
        let cell_width = split_area.width.saturating_sub(3) / 2;
        let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
        terminal.draw(|frame| render(frame, &split, &theme)).unwrap();
        let buffer = terminal.backend().buffer();
        let changed_row = split_area.y + 2;
        for x in split_area.x..split_area.x + cell_width {
            assert!([theme.deletion.bg, theme.intraline.bg].contains(&buffer[(x, changed_row)].style().bg));
        }
        for x in split_area.x + cell_width + 3..split_area.right() {
            assert!([theme.addition.bg, theme.intraline.bg].contains(&buffer[(x, changed_row)].style().bg));
        }
        assert_eq!(
            buffer[(split_area.x + cell_width - 1, changed_row)].style().bg,
            theme.deletion.bg
        );
        assert_eq!(
            buffer[(split_area.right() - 1, changed_row)].style().bg,
            theme.addition.bg
        );
    }

    #[test]
    fn named_theme_styles_cover_states_help_and_both_review_layouts() {
        let theme = Theme::resolve(crate::ThemeFamily::Catppuccin, Some(crate::ColorMode::Dark));

        let loading = rendered_styles(App::loading(), 32, 5, &theme);
        assert!(has_style(&loading, theme.application));
        assert!(has_style(&loading, theme.title));
        assert!(has_style(&loading, theme.footer));

        let error = rendered_styles(App::error("broken input"), 32, 6, &theme);
        assert!(has_style(&error, theme.error));

        let empty = parse_patch(b"", ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let empty_styles = rendered_styles(App::ready(&empty), 32, 5, &theme);
        assert!(has_style(&empty_styles, theme.application));

        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let narrow = rendered_styles(App::ready(&changeset), 24, 9, &theme);
        assert!(has_style(&narrow, theme.addition));
        assert!(has_style(&narrow, theme.deletion));

        let wide = rendered_styles(App::ready(&changeset), 120, 12, &theme);
        for style in [
            theme.panel,
            theme.selected,
            theme.file,
            theme.hunk,
            theme.addition,
            theme.deletion,
            theme.marker,
        ] {
            assert!(has_style(&wide, style), "wide review omitted {style:?}");
        }

        let mut help = App::ready(&changeset);
        help.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let help_styles = rendered_styles(help, 90, 24, &theme);
        assert!(has_style(&help_styles, theme.panel));
        assert!(has_style(&help_styles, theme.file));
    }

    #[test]
    fn named_theme_styles_cover_search_syntax_and_intraline_emphasis() {
        let theme = Theme::resolve(crate::ThemeFamily::Eldritch, Some(crate::ColorMode::Light));
        let patch = b"diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        for character in "new".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let styles = rendered_styles(app, 120, 12, &theme);
        assert!(has_style(&styles, theme.search_match));
        assert!(styles.iter().any(|style| {
            style.bg == theme.intraline.bg
                && style.add_modifier.contains(Modifier::BOLD)
                && !style.add_modifier.contains(Modifier::UNDERLINED)
        }));
        let addition_styles = styles
            .iter()
            .filter(|style| style.bg == theme.addition.bg)
            .collect::<Vec<_>>();
        assert!(!addition_styles.is_empty());
        assert!(addition_styles.iter().all(|style| style.fg == theme.addition.fg));
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

    fn rendered_styles(mut app: App<'_>, width: u16, height: u16, theme: &Theme) -> Vec<Style> {
        app.resize(width, height);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app, theme)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .flat_map(|y| (0..width).map(move |x| buffer[(x, y)].style()))
            .collect()
    }

    fn has_style(styles: &[Style], expected: Style) -> bool {
        styles.iter().any(|style| {
            style.fg == expected.fg && style.bg == expected.bg && style.add_modifier.contains(expected.add_modifier)
        })
    }
}
