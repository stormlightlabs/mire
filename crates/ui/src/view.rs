use std::borrow::Cow;

use mire_core::{AnchorSide, FileDiff, FileStatus, LineKind, MissingNewline};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::EditorTarget;
use crate::app::{App, AppState, GutterMark};
use crate::chrome::{
    diff_stats, file_path, fit_line, render_footer, render_help, render_sidebar, render_sidebar_divider, render_title,
    truncate_text,
};
use crate::stream::TextRange;
use crate::stream::{ReviewStream, RowKey};
use crate::theme::Theme;
use crate::walkthrough::render_walkthrough;

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
                render_walkthrough(frame, areas.walkthrough, app, theme);
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
    if app.file_picker_visible() {
        render_file_picker(frame, app, theme);
    }
    if app.description_visible() {
        render_description_dialog(frame, app, theme);
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
    let thumb_start = offset
        .min(maximum_offset)
        .saturating_mul(travel)
        .checked_div(maximum_offset)
        .unwrap_or(0);
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
        RowKey::File { file } => file_line(
            stream.file(file),
            width,
            app.file_collapsed(file),
            app.collapsed_file_line_count(file),
            theme,
        ),
        RowKey::Binary { .. } => Line::styled("     Binary files differ", theme.marker),
        RowKey::Hunk { file, hunk } => hunk_line(
            stream,
            file,
            hunk,
            width,
            app.hunk_collapsed(file, hunk),
            app.collapsed_hunk_line_count(file, hunk),
            theme,
        ),
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
    const RULE: &str = "▌ ";
    const MARK: &str = "● ";

    let Some(note) = app.note(note_index) else {
        return Line::styled("  unavailable finding", theme.error);
    };
    let severity = theme.severity(note.severity());
    let author = note.author().display_name().unwrap_or_else(|| note.author().id());
    let label = format!("{RULE}{MARK}{}", note.severity());
    let kind = note.annotation_kind().to_string();
    let status = note.status().to_string();
    let action_width = if selected { Span::raw(ACTIONS).width() } else { 0 };
    let room = usize::from(width).saturating_sub(action_width);
    let label = truncate_text(&label, room);
    let room = room.saturating_sub(Span::raw(&label).width());
    let details = note_details(&[&kind, &status, author], note.body(), room);
    let room = room.saturating_sub(Span::raw(&details).width());
    let body = truncate_text(note.body(), room);
    let mut spans = vec![
        Span::styled(label, severity),
        Span::styled(details, theme.muted),
        Span::styled(body, Style::new().add_modifier(Modifier::ITALIC)),
    ];
    if selected {
        spans.push(Span::styled(ACTIONS, theme.accent));
    }
    fit_line(spans, usize::from(width), theme.finding)
}

/// Picks the finding metadata that fits beside the body in `room` cells.
///
/// The body keeps at least 20 cells, or its whole width when
/// shorter. Metadata gives way first: the last part (the author) goes, then the
/// rest (kind and status), and only then does the body shrink.
fn note_details(parts: &[&str], body: &str, room: usize) -> String {
    const NOTE_BODY_MIN: usize = 20;
    const GAP: &str = "  ";

    let body_need = Span::raw(body).width().min(NOTE_BODY_MIN);
    let candidates = [parts, &parts[..parts.len().saturating_sub(1)]];
    candidates
        .into_iter()
        .filter(|kept| !kept.is_empty())
        .map(|kept| format!(" · {}{GAP}", kept.join(" · ")))
        .find(|details| Span::raw(details).width() + body_need <= room)
        .unwrap_or_else(|| truncate_text(GAP, room))
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

fn render_description_dialog(frame: &mut Frame<'_>, app: &App<'_>, theme: &Theme) {
    let dialog = app.description_dialog();
    let block_width = usize::from(dialog.area.width.saturating_sub(4));
    let title = truncate_text(app.title().unwrap_or("Description"), block_width);
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme.accent)
        .padding(Padding::horizontal(1))
        .title(Span::styled(format!(" {title} "), theme.accent));
    if dialog.max_scroll() > 0 {
        let position = format!(
            " {}-{}/{} ",
            dialog.scroll + 1,
            (dialog.scroll + dialog.text_height()).min(dialog.lines.len()),
            dialog.lines.len()
        );
        block = block.title_bottom(Line::styled(position, theme.muted).right_aligned());
    }
    let lines = dialog
        .lines
        .iter()
        .skip(dialog.scroll)
        .take(dialog.text_height())
        .map(|line| Line::raw(line.clone()))
        .collect::<Vec<_>>();
    frame.render_widget(Clear, dialog.area);
    frame.render_widget(Paragraph::new(lines).style(theme.panel).block(block), dialog.area);
}

fn render_file_picker(frame: &mut Frame<'_>, app: &App<'_>, theme: &Theme) {
    let dialog = app.file_picker_area();
    let entries = app.file_picker_entries();
    let selected = app.file_picker_selected().unwrap_or_default();
    let offset = app.file_picker_offset().unwrap_or_default();
    let query = app.file_picker_query().unwrap_or_default();
    let height = usize::from(dialog.height.saturating_sub(5));
    let mut lines = vec![
        Line::styled(" File jump", theme.accent),
        Line::styled(format!(" > {query}█"), theme.panel),
        Line::styled(format!(" {} changed files", entries.len()), theme.muted),
    ];
    for (index, entry) in entries.iter().enumerate().skip(offset).take(height) {
        let changes = entry
            .changes
            .map(|(additions, deletions)| format!(" +{additions} -{deletions}"))
            .unwrap_or_default();
        let findings = if entry.findings.open == 0 && entry.findings.completed == 0 {
            String::new()
        } else {
            let severity = entry
                .findings
                .highest_open_severity
                .map(|severity| format!(" {severity}"))
                .unwrap_or_default();
            format!(" !{} ✓{}{}", entry.findings.open, entry.findings.completed, severity)
        };
        let line = format!(
            " {} {} {}{}{}",
            if index == selected { "▌" } else { " " },
            file_status_code(entry.status),
            entry.path,
            changes,
            findings
        );
        lines.push(fit_line(
            vec![Span::styled(
                truncate_text(&line, usize::from(dialog.width.saturating_sub(2))),
                if index == selected { theme.selected } else { theme.panel },
            )],
            usize::from(dialog.width.saturating_sub(2)),
            if index == selected { theme.selected } else { theme.panel },
        ));
    }
    frame.render_widget(Clear, dialog);
    frame.render_widget(
        Paragraph::new(lines)
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

fn hunk_line(
    stream: &ReviewStream<'_>, file: usize, hunk: usize, width: u16, collapsed: bool, hidden_lines: usize,
    theme: &Theme,
) -> Line<'static> {
    let hunk = stream.hunk(file, hunk);
    let section = String::from_utf8_lossy(hunk.section());
    let disclosure = if collapsed { "▸" } else { "▾" };
    let hidden = if collapsed { format!(" · {hidden_lines} lines hidden") } else { String::new() };
    fit_line(
        vec![Span::styled(
            format!(
                " {disclosure} @@ -{},{} +{},{} @@{}{}{}",
                hunk.old_start(),
                hunk.old_line_count(),
                hunk.new_start(),
                hunk.new_line_count(),
                if section.is_empty() { "" } else { " " },
                section,
                hidden,
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
                    style = style.patch(self.theme.emphasis(base));
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

fn file_line(file: &FileDiff, width: u16, collapsed: bool, hidden_lines: usize, theme: &Theme) -> Line<'static> {
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
    let disclosure = if collapsed { "▸" } else { "▾" };
    let hidden = if collapsed { format!(" · {hidden_lines} lines hidden") } else { String::new() };
    let path_width = available.saturating_sub(metadata_width.saturating_add(5));
    let path = truncate_text(&format!("{}{}", file_path(file), hidden), path_width);
    let path_width = Span::raw(&path).width();
    let gap = available.saturating_sub(path_width + metadata_width + 3);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(disclosure, theme.accent),
        Span::raw(" "),
        Span::styled(path, theme.accent),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(metadata);
    fit_line(spans, available, theme.file)
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
    mod contrast;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    use super::*;

    const PATCH: &[u8] = b"diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@ heading\n-old\n+new\n context\n\\ No newline at end of file\ndiff --git a/image.bin b/image.bin\nBinary files a/image.bin and b/image.bin differ\n";
    const DESCRIPTION: &str = "Parsed keys and values keep surrounding spaces, so `port = 8080` fails to match. Trim both sides before returning them.\n\nThe dev server also moves to port 8080:\n- update the default\n- document the change in the guide";
    const NESTED_PATCH: &[u8] = b"diff --git a/src/lib.rs b/src/lib.rs
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

        let mut tall = App::ready(&changeset);
        tall.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        insta::assert_snapshot!("help_tall_90x48", snapshot(tall, 90, 48));
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
            assert!(
                [theme.deletion.bg, theme.deletion_emphasis.bg].contains(&buffer[(x, unified_area.y + 2)].style().bg)
            );
            assert!(
                [theme.addition.bg, theme.addition_emphasis.bg].contains(&buffer[(x, unified_area.y + 3)].style().bg)
            );
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
            assert!([theme.deletion.bg, theme.deletion_emphasis.bg].contains(&buffer[(x, changed_row)].style().bg));
        }
        for x in split_area.x + cell_width + 3..split_area.right() {
            assert!([theme.addition.bg, theme.addition_emphasis.bg].contains(&buffer[(x, changed_row)].style().bg));
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
        for emphasis in [theme.addition_emphasis, theme.deletion_emphasis] {
            assert!(styles.iter().any(|style| {
                style.fg == emphasis.fg
                    && style.bg == emphasis.bg
                    && style.add_modifier.contains(Modifier::BOLD)
                    && !style.add_modifier.contains(Modifier::UNDERLINED)
            }));
        }
        let addition_styles = styles
            .iter()
            .filter(|style| style.bg == theme.addition.bg)
            .collect::<Vec<_>>();
        assert!(!addition_styles.is_empty());
        assert!(addition_styles.iter().all(|style| style.fg == theme.addition.fg));
    }

    #[test]
    fn sidebar_snapshots_cover_the_file_list_focus_and_hidden_states() {
        let changeset = nested_changeset();
        insta::assert_snapshot!("sidebar_files_100x16", snapshot(App::ready(&changeset), 100, 16));

        let mut focused = App::ready(&changeset);
        press(&mut focused, KeyCode::Tab);
        press(&mut focused, KeyCode::Down);
        insta::assert_snapshot!("sidebar_focused_100x16", snapshot(focused, 100, 16));

        let mut hidden = App::ready(&changeset);
        press(&mut hidden, KeyCode::Char('b'));
        insta::assert_snapshot!("sidebar_hidden_100x16", snapshot(hidden, 100, 16));
    }

    #[test]
    fn layout_snapshots_cover_forced_unified_and_split_modes() {
        let changeset = nested_changeset();

        let mut unified = App::ready(&changeset);
        press(&mut unified, KeyCode::Char('1'));
        insta::assert_snapshot!("layout_unified_120x16", snapshot(unified, 120, 16));

        let mut split = App::ready(&changeset);
        press(&mut split, KeyCode::Char('2'));
        insta::assert_snapshot!("layout_split_120x16", snapshot(split, 120, 16));
    }

    #[test]
    fn collapse_snapshots_cover_a_collapsed_file_and_a_collapsed_hunk() {
        let changeset = nested_changeset();

        let mut file = App::ready(&changeset);
        press(&mut file, KeyCode::Enter);
        insta::assert_snapshot!("collapsed_file_100x16", snapshot(file, 100, 16));

        let mut hunk = App::ready(&changeset);
        hunk.resize(100, 16);
        press(&mut hunk, KeyCode::Down);
        press(&mut hunk, KeyCode::Enter);
        insta::assert_snapshot!("collapsed_hunk_100x16", snapshot(hunk, 100, 16));
    }

    #[test]
    fn finding_lines_use_their_own_surface_and_a_color_per_severity() {
        use mire_core::NoteSeverity;

        let theme = Theme::resolve(crate::ThemeFamily::Iceberg, Some(crate::ColorMode::Light));
        let review = severities_review();
        let app = App::review_with_options(&review, crate::AppOptions::default());
        let styles = rendered_styles(app, 120, 24, &theme);
        assert!(has_style(&styles, theme.finding));
        for severity in [
            NoteSeverity::Note,
            NoteSeverity::Low,
            NoteSeverity::Medium,
            NoteSeverity::High,
            NoteSeverity::Critical,
        ] {
            let mark = theme.severity(severity).bg(theme.finding.bg.unwrap());
            assert!(has_style(&styles, mark), "{severity} mark on the finding line");
        }
        assert!(!has_style(&styles, theme.error), "no filled severity badge");
    }

    #[test]
    fn note_snapshots_cover_inline_findings_focus_editor_and_filters() {
        let review = notes_review();
        insta::assert_snapshot!(
            "notes_inline_120x18",
            snapshot(App::review_with_options(&review, crate::AppOptions::default()), 120, 18)
        );

        let mut focused = App::review_with_options(&review, crate::AppOptions::default());
        press(&mut focused, KeyCode::Char('p'));
        insta::assert_snapshot!("notes_focused_120x18", snapshot(focused, 120, 18));

        let mut editor = App::review_with_options(&review, crate::AppOptions::default());
        editor.resize(120, 18);
        for _ in 0..2 {
            press(&mut editor, KeyCode::Down);
        }
        press(&mut editor, KeyCode::Char('c'));
        editor.handle_paste("Check the port range.");
        insta::assert_snapshot!("notes_editor_120x18", snapshot(editor, 120, 18));

        let mut filters = App::review_with_options(&review, crate::AppOptions::default());
        press(&mut filters, KeyCode::Char('f'));
        insta::assert_snapshot!("notes_filters_120x18", snapshot(filters, 120, 18));
    }

    #[test]
    fn narrow_finding_lines_drop_metadata_before_the_body() {
        let review = notes_review();
        insta::assert_snapshot!(
            "notes_inline_60x18",
            snapshot(App::review_with_options(&review, crate::AppOptions::default()), 60, 18)
        );
        insta::assert_snapshot!(
            "notes_inline_40x18",
            snapshot(App::review_with_options(&review, crate::AppOptions::default()), 40, 18)
        );
    }

    #[test]
    fn finding_details_drop_author_then_kind_and_status() {
        let parts = ["defect", "open", "reviewer"];
        let body = "Port 8080 collides with the dev server.";
        assert_eq!(note_details(&parts, body, 49), " · defect · open · reviewer  ");
        assert_eq!(note_details(&parts, body, 48), " · defect · open  ");
        assert_eq!(note_details(&parts, body, 38), " · defect · open  ");
        assert_eq!(note_details(&parts, body, 37), "  ");
        assert_eq!(note_details(&parts, "Short", 23), " · defect · open  ");
    }

    #[test]
    fn viewed_snapshot_marks_the_selected_file_in_the_file_list() {
        let review = notes_review();
        let mut app = App::review_with_options(&review, crate::AppOptions::default());
        app.resize(120, 18);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('m'));
        assert!(app.viewed_change().is_some_and(|change| change.mark.is_some()));
        app.finish_viewed(Ok(()));
        assert_eq!(app.file_viewed(1), Some(crate::FileViewed::Viewed));
        assert_eq!(app.file_viewed(0), Some(crate::FileViewed::NotViewed));
        insta::assert_snapshot!("sidebar_viewed_120x18", snapshot(app, 120, 18));
    }

    #[test]
    fn generated_snapshot_marks_generated_files_in_the_file_list() {
        let mut changeset = nested_changeset();
        let guide = changeset
            .files()
            .iter()
            .position(|file| file.canonical_path().as_bytes() == b"docs/guide.md")
            .unwrap();
        changeset.set_generated(guide, true);
        let mut app = App::ready(&changeset);
        app.resize(120, 16);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        insta::assert_snapshot!("sidebar_generated_120x16", snapshot(app, 120, 16));
    }

    #[test]
    fn walkthrough_snapshot_shows_progress_in_the_header() {
        let review = notes_review();
        let mut app = App::review_with_options(&review, crate::AppOptions::default());
        app.start_walkthrough().unwrap();
        insta::assert_snapshot!("walkthrough_started_120x18", snapshot(app, 120, 18));
    }

    #[test]
    fn walkthrough_snapshots_cover_each_mode_state() {
        use crate::walkthrough::fixtures::{plain_review, refreshed_review, walkthrough_review};

        let review = walkthrough_review();
        let open = |review, keys: &str, width, height| {
            let mut app = App::review_with_options(review, crate::AppOptions::default());
            app.resize(width, height);
            for key in keys.chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT));
            }
            app
        };
        insta::assert_snapshot!(
            "walkthrough_overview_120x24",
            snapshot(open(&review, "W", 120, 24), 120, 24)
        );
        insta::assert_snapshot!(
            "walkthrough_step_120x24",
            snapshot(open(&review, "JJ", 120, 24), 120, 24)
        );
        insta::assert_snapshot!(
            "walkthrough_step_narrow_40x16",
            snapshot(open(&review, "JJ", 40, 16), 40, 16)
        );
        insta::assert_snapshot!(
            "walkthrough_overview_narrow_40x16",
            snapshot(open(&review, "W", 40, 16), 40, 16)
        );
        insta::assert_snapshot!(
            "walkthrough_step_tiny_20x6",
            snapshot(open(&review, "JJ", 20, 6), 20, 6)
        );
        insta::assert_snapshot!(
            "walkthrough_overview_tiny_20x6",
            snapshot(open(&review, "W", 20, 6), 20, 6)
        );
        let mut files = open(&review, "W", 120, 24);
        files.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        insta::assert_snapshot!("walkthrough_sidebar_120x24", snapshot(files, 120, 24));

        let refreshed = refreshed_review();
        insta::assert_snapshot!(
            "walkthrough_stale_120x24",
            snapshot(open(&refreshed, "JJ", 120, 24), 120, 24)
        );
        insta::assert_snapshot!(
            "walkthrough_ambiguous_120x24",
            snapshot(open(&refreshed, "JJJ", 120, 24), 120, 24)
        );

        let plain = plain_review();
        insta::assert_snapshot!("walkthrough_none_100x16", snapshot(open(&plain, "W", 100, 16), 100, 16));

        let mut session = App::review_with_options(&review, crate::AppOptions::default());
        session.resize(120, 24);
        session.start_walkthrough().unwrap();
        session.move_walkthrough(true).unwrap();
        insta::assert_snapshot!("walkthrough_session_120x24", snapshot(session, 120, 24));
    }

    #[test]
    fn overlay_snapshots_cover_the_file_picker_and_search() {
        let changeset = nested_changeset();

        let mut picker = App::ready(&changeset);
        picker.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        insta::assert_snapshot!("file_picker_100x16", snapshot(picker, 100, 16));

        let mut search = App::ready(&changeset);
        press(&mut search, KeyCode::Char('/'));
        for character in "trim".chars() {
            press(&mut search, KeyCode::Char(character));
        }
        insta::assert_snapshot!("search_prompt_100x16", snapshot(search, 100, 16));
    }

    #[test]
    fn title_snapshots_cover_the_header_and_narrow_truncation() {
        let changeset = nested_changeset();
        let options = || crate::AppOptions {
            title: Some("Trim parsed values and move the dev server port".to_owned()),
            description: Some(DESCRIPTION.to_owned()),
            ..crate::AppOptions::default()
        };

        insta::assert_snapshot!(
            "header_title_120x16",
            snapshot(App::ready_with_options(&changeset, options()), 120, 16)
        );
        insta::assert_snapshot!(
            "header_title_narrow_60x10",
            snapshot(App::ready_with_options(&changeset, options()), 60, 10)
        );
    }

    #[test]
    fn description_snapshots_cover_the_dialog_scrolling_and_no_description() {
        let changeset = nested_changeset();
        let options = |description: &str| crate::AppOptions {
            title: Some("Trim parsed values and move the dev server port".to_owned()),
            description: Some(description.to_owned()),
            ..crate::AppOptions::default()
        };

        let mut dialog = App::ready_with_options(&changeset, options(DESCRIPTION));
        press(&mut dialog, KeyCode::Char('i'));
        insta::assert_snapshot!("description_dialog_100x20", snapshot(dialog, 100, 20));

        let long = (1..=30)
            .map(|line| format!("{line}. Step {line} of the rollout checks the parser against fixture {line}."))
            .collect::<Vec<_>>()
            .join("\n");
        let mut scrolled = App::ready_with_options(&changeset, options(&long));
        scrolled.resize(100, 20);
        press(&mut scrolled, KeyCode::Char('i'));
        press(&mut scrolled, KeyCode::PageDown);
        press(&mut scrolled, KeyCode::Char('j'));
        insta::assert_snapshot!("description_scrolled_100x20", snapshot(scrolled, 100, 20));

        let mut empty = App::ready(&changeset);
        press(&mut empty, KeyCode::Char('i'));
        insta::assert_snapshot!("description_empty_100x16", snapshot(empty, 100, 16));
    }

    fn nested_changeset() -> mire_core::Changeset {
        parse_patch(
            NESTED_PATCH,
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap()
    }

    /// A review of the nested patch with an open defect and an open question.
    fn notes_review() -> mire_core::Review {
        use mire_core::{AnnotationKind, NoteSeverity};

        review_with_notes(&[
            (
                "src/lib.rs",
                2,
                "port",
                NoteSeverity::Medium,
                AnnotationKind::Defect,
                "Port 8080 collides with the dev server.",
            ),
            (
                "src/parse/mod.rs",
                3,
                "trim",
                NoteSeverity::Note,
                AnnotationKind::Question,
                "Should an empty value be accepted?",
            ),
        ])
    }

    /// A review of the nested patch with one open finding at each severity.
    fn severities_review() -> mire_core::Review {
        use mire_core::{AnnotationKind, NoteSeverity};

        review_with_notes(&[
            (
                "src/lib.rs",
                1,
                "low",
                NoteSeverity::Low,
                AnnotationKind::Comment,
                "Name the module.",
            ),
            (
                "src/lib.rs",
                2,
                "high",
                NoteSeverity::High,
                AnnotationKind::Defect,
                "Port 8080 is taken.",
            ),
            (
                "src/lib.rs",
                3,
                "medium",
                NoteSeverity::Medium,
                AnnotationKind::Suggestion,
                "Read the host.",
            ),
            (
                "src/parse/mod.rs",
                1,
                "note",
                NoteSeverity::Note,
                AnnotationKind::Question,
                "Why split here?",
            ),
            (
                "src/parse/mod.rs",
                3,
                "critical",
                NoteSeverity::Critical,
                AnnotationKind::Defect,
                "Trim drops data.",
            ),
        ])
    }

    /// A review of the nested patch with one open agent finding per entry:
    /// path, new-side line, identifier, severity, kind and body.
    fn review_with_notes(
        notes: &[(
            &str,
            u64,
            &str,
            mire_core::NoteSeverity,
            mire_core::AnnotationKind,
            &str,
        )],
    ) -> mire_core::Review {
        use mire_core::{
            Anchor, AnchorSide, Author, BytePath, FileContent, LineNumber, LineRange, NoteEvent, NoteEventKind, NoteId,
            NoteStatus, Provenance, Review, ReviewNote, ReviewRevision,
        };

        let changeset = nested_changeset();
        let author = Author::new("reviewer", None).unwrap();
        let (notes, events) = notes
            .iter()
            .zip(1..)
            .map(|(&(path, line, id, severity, kind, body), sequence)| {
                let file = changeset
                    .files()
                    .iter()
                    .find(|file| {
                        file.new_side()
                            .is_some_and(|side| side.path.as_bytes() == path.as_bytes())
                    })
                    .expect("fixture has the file");
                let FileContent::Text { hunks } = file.content() else {
                    panic!("fixture is textual");
                };
                let line = LineNumber::new(line).unwrap();
                let anchor = Anchor::new(
                    &changeset,
                    BytePath::new(path.as_bytes().to_vec()).unwrap(),
                    AnchorSide::New,
                    LineRange::new(line, line).unwrap(),
                    hunks[0].fingerprint(),
                )
                .unwrap();
                let note = ReviewNote::new(
                    NoteId::new(id).unwrap(),
                    anchor,
                    author.clone(),
                    severity,
                    NoteStatus::Open,
                    body.to_owned(),
                    Provenance::Agent { producer: "fixture".to_owned() },
                )
                .unwrap()
                .with_annotation_kind(kind);
                let created = NoteEventKind::Created { status: NoteStatus::Open };
                let event = NoteEvent::new(sequence, NoteId::new(id).unwrap(), author.clone(), created).unwrap();
                (note, event)
            })
            .unzip();
        Review::new(ReviewRevision::new(1).unwrap(), changeset.clone(), notes, events).unwrap()
    }

    fn press(app: &mut App<'_>, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
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
