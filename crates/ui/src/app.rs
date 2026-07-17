use std::cell::RefCell;

use crossterm::event::{KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use mire_core::Changeset;
use ratatui::layout::{Position, Rect};

use crate::layout::UiAreas;
use crate::navigation::{Action, Focus, action_for};
use crate::stream::{LayoutMode, ReviewStream, RowAnchor};
use crate::syntax::SyntaxCache;

const DEFAULT_TERMINAL_HEIGHT: u16 = 24;
const DEFAULT_TERMINAL_WIDTH: u16 = 80;
const DEFAULT_CONTEXT_LINES: usize = 3;
const MAX_CONTEXT_LINES: usize = 100;
const MAX_SEARCH_MATCHES: usize = 100_000;

/// Presentation preferences supplied when an interactive review starts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppOptions {
    /// Inkjet language name or alias applied to every text file.
    pub language_override: Option<String>,
}

/// The deterministic content state shown by the review application.
#[derive(Debug)]
pub enum AppState<'a> {
    /// The changeset is being prepared for display.
    Loading,
    /// The changeset contains no files.
    Empty,
    /// A changeset is available as a continuous review stream.
    Ready(ReviewStream<'a>),
    /// The review could not be prepared or rendered.
    Error(String),
}

/// Terminal-only interaction state kept separate from the changeset model.
pub struct App<'a> {
    state: AppState<'a>,
    focus: Focus,
    layout_mode: LayoutMode,
    scroll: usize,
    scroll_anchor: Option<RowAnchor>,
    selected_file: usize,
    sidebar_offset: usize,
    terminal_width: u16,
    terminal_height: u16,
    help_visible: bool,
    search_input: bool,
    search_query: String,
    search_matches: Vec<RowAnchor>,
    search_match: Option<usize>,
    context_lines: usize,
    wrap_lines: bool,
    syntax_cache: RefCell<SyntaxCache>,
    should_quit: bool,
}

impl<'a> App<'a> {
    /// Creates the initial loading state.
    pub fn loading() -> Self {
        Self::with_state(AppState::Loading, AppOptions::default())
    }

    /// Creates an error state with a user-facing message.
    pub fn error(message: impl Into<String>) -> Self {
        Self::with_state(AppState::Error(message.into()), AppOptions::default())
    }

    /// Creates an empty or ready application from an immutable changeset.
    pub fn ready(changeset: &'a Changeset) -> Self {
        Self::ready_with_options(changeset, AppOptions::default())
    }

    /// Creates an application with explicit presentation preferences.
    pub fn ready_with_options(changeset: &'a Changeset, options: AppOptions) -> Self {
        let areas = UiAreas::new(Rect::new(0, 0, DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT));
        let layout = LayoutMode::Automatic.resolve(areas.review.width);
        let state = if changeset.files().is_empty() {
            AppState::Empty
        } else {
            AppState::Ready(ReviewStream::new(changeset, layout))
        };
        Self::with_state(state, options)
    }

    /// Returns the content currently displayed by the application.
    pub const fn state(&self) -> &AppState<'_> {
        &self.state
    }

    /// Returns the first visible row in the continuous stream.
    pub const fn scroll(&self) -> usize {
        self.scroll
    }

    /// Returns the file selected in the sidebar.
    pub const fn selected_file(&self) -> usize {
        self.selected_file
    }

    /// Returns the first file currently visible in the sidebar.
    pub const fn sidebar_offset(&self) -> usize {
        self.sidebar_offset
    }

    /// Returns the pane currently receiving movement keys.
    pub const fn focus(&self) -> Focus {
        self.focus
    }

    /// Returns the requested unified, split, or automatic layout.
    pub const fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    /// Reports whether the complete binding help is visible.
    pub const fn help_visible(&self) -> bool {
        self.help_visible
    }

    /// Returns the active search text, including input that has not been submitted.
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Reports whether keyboard text is being entered into search.
    pub const fn search_input(&self) -> bool {
        self.search_input
    }

    /// Returns current and total search match positions.
    pub fn search_status(&self) -> Option<(usize, usize)> {
        self.search_match.map(|index| (index + 1, self.search_matches.len()))
    }

    /// Returns the number of context lines retained at each edge of a context run.
    pub const fn context_lines(&self) -> usize {
        self.context_lines
    }

    /// Reports whether long source lines wrap into anchored continuation rows.
    pub const fn wrap_lines(&self) -> bool {
        self.wrap_lines
    }

    /// Returns semantic syntax ranges for one source line.
    pub fn syntax_ranges(&self, file: usize, hunk: usize, line: usize) -> Vec<(usize, usize, usize)> {
        let AppState::Ready(stream) = &self.state else {
            return Vec::new();
        };
        let source = String::from_utf8_lossy(stream.hunk(file, hunk).lines()[line].content());
        let Ok(mut cache) = self.syntax_cache.try_borrow_mut() else {
            return Vec::new();
        };
        cache
            .ranges(file, hunk, line, stream.file(file), &source)
            .iter()
            .map(|range| (range.start, range.end, range.scope))
            .collect()
    }

    /// Reports whether the user requested that the application close.
    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Returns layout rectangles for the current terminal size.
    pub fn areas(&self) -> UiAreas {
        UiAreas::new(Rect::new(0, 0, self.terminal_width, self.terminal_height))
    }

    /// Applies a terminal resize while preserving the top source-row anchor.
    pub fn resize(&mut self, width: u16, height: u16) {
        let anchor = self.current_anchor();
        self.terminal_width = width;
        self.terminal_height = height;
        self.rebuild_stream(anchor);
        self.clamp_scroll();
        self.ensure_sidebar_selection_visible();
    }

    /// Applies one keyboard event.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        if self.search_input {
            self.handle_search_key(key);
        } else if let Some(action) = action_for(key) {
            self.apply(action);
        }
    }

    /// Applies mouse scrolling and selection using the same navigation state as keyboard input.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let areas = self.areas();
        let position = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollDown if areas.sidebar.contains(position) => self.select_file_by(1),
            MouseEventKind::ScrollUp if areas.sidebar.contains(position) => self.select_file_by(-1),
            MouseEventKind::ScrollDown => self.scroll_by(3),
            MouseEventKind::ScrollUp => self.scroll_up(3),
            MouseEventKind::Down(MouseButton::Left) if areas.sidebar.contains(position) => {
                self.select_sidebar_row(mouse.row.saturating_sub(areas.sidebar.y));
            }
            MouseEventKind::Down(MouseButton::Left) if areas.review.contains(position) => {
                self.select_review_row(mouse.row.saturating_sub(areas.review.y));
            }
            _ => {}
        }
    }

    fn with_state(state: AppState<'a>, options: AppOptions) -> Self {
        let syntax_cache = RefCell::new(SyntaxCache::new(options.language_override.as_deref()));
        Self {
            state,
            focus: Focus::Review,
            layout_mode: LayoutMode::Automatic,
            scroll: 0,
            scroll_anchor: None,
            selected_file: 0,
            sidebar_offset: 0,
            terminal_width: DEFAULT_TERMINAL_WIDTH,
            terminal_height: DEFAULT_TERMINAL_HEIGHT,
            help_visible: false,
            search_input: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_match: None,
            context_lines: DEFAULT_CONTEXT_LINES,
            wrap_lines: false,
            syntax_cache,
            should_quit: false,
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleHelp => self.help_visible = !self.help_visible,
            Action::ToggleFocus => self.focus = self.focus.toggle(),
            Action::MoveDown if matches!(self.focus, Focus::Sidebar) => self.select_file_by(1),
            Action::MoveDown => self.scroll_by(1),
            Action::MoveUp if matches!(self.focus, Focus::Sidebar) => self.select_file_by(-1),
            Action::MoveUp => self.scroll_up(1),
            Action::PageDown => self.scroll_by(self.viewport_height().max(1)),
            Action::PageUp => self.scroll_up(self.viewport_height().max(1)),
            Action::FirstRow => self.set_scroll(0),
            Action::LastRow => self.set_scroll(self.max_scroll()),
            Action::NextFile => self.select_file_by(1),
            Action::PreviousFile => self.select_file_by(-1),
            Action::NextHunk => self.jump_hunk(true),
            Action::PreviousHunk => self.jump_hunk(false),
            Action::StartSearch => {
                self.search_input = true;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_match = None;
            }
            Action::NextMatch => self.move_match(true),
            Action::PreviousMatch => self.move_match(false),
            Action::MoreContext => self.set_context(self.context_lines.saturating_add(1)),
            Action::LessContext => self.set_context(self.context_lines.saturating_sub(1)),
            Action::ToggleWrap => {
                self.wrap_lines = !self.wrap_lines;
                self.rebuild_stream(self.current_anchor());
                self.clamp_scroll();
            }
            Action::UnifiedLayout => self.set_layout(LayoutMode::Unified),
            Action::SplitLayout => self.set_layout(LayoutMode::Split),
            Action::AutomaticLayout => self.set_layout(LayoutMode::Automatic),
        }
    }

    fn set_layout(&mut self, mode: LayoutMode) {
        let anchor = self.current_anchor();
        self.layout_mode = mode;
        self.rebuild_stream(anchor);
        self.clamp_scroll();
    }

    fn rebuild_stream(&mut self, anchor: Option<RowAnchor>) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let changeset = stream.changeset();
        let layout = self.layout_mode.resolve(self.areas().review.width);
        let replacement = ReviewStream::with_presentation(
            changeset,
            layout,
            self.context_lines,
            self.wrap_lines.then_some(self.areas().review.width),
        );
        self.scroll = anchor
            .and_then(|value| replacement.index_of_anchor(value))
            .unwrap_or(self.scroll);
        self.scroll_anchor = anchor;
        self.state = AppState::Ready(replacement);
    }

    fn current_anchor(&self) -> Option<RowAnchor> {
        if self.scroll_anchor.is_some() {
            return self.scroll_anchor;
        }
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        stream.anchor_at(self.scroll)
    }

    fn jump_hunk(&mut self, forward: bool) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let origin = self
            .scroll_anchor
            .and_then(|anchor| stream.index_of_anchor(anchor))
            .unwrap_or(self.scroll);
        let target = if forward {
            stream.hunk_positions().find(|(row, _, _)| *row > origin)
        } else {
            stream.hunk_positions().rev().find(|(row, _, _)| *row < origin)
        };
        if let Some((row, file, _)) = target {
            self.selected_file = file;
            self.jump_to_row(row);
            self.ensure_sidebar_selection_visible();
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            crossterm::event::KeyCode::Enter => {
                self.search_input = false;
                self.rebuild_search_matches();
                self.move_match(true);
            }
            crossterm::event::KeyCode::Esc => {
                self.search_input = false;
            }
            crossterm::event::KeyCode::Backspace => {
                self.search_query.pop();
            }
            crossterm::event::KeyCode::Char(character) => self.search_query.push(character),
            _ => {}
        }
    }

    fn rebuild_search_matches(&mut self) {
        self.search_matches.clear();
        self.search_match = None;
        let query = self.search_query.clone();
        if query.is_empty() {
            return;
        }
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        for (file, diff) in stream.changeset().files().iter().enumerate() {
            let mire_core::FileContent::Text { hunks } = diff.content() else {
                continue;
            };
            for (hunk, source) in hunks.iter().enumerate() {
                for (line, source_line) in source.lines().iter().enumerate() {
                    if contains_query(&String::from_utf8_lossy(source_line.content()), &query) {
                        self.search_matches.push(RowAnchor::Line { file, hunk, line });
                        if self.search_matches.len() == MAX_SEARCH_MATCHES {
                            return;
                        }
                    }
                }
            }
        }
    }

    fn move_match(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        let next = match (self.search_match, forward) {
            (Some(index), true) => (index + 1) % self.search_matches.len(),
            (Some(0), false) | (None, false) => self.search_matches.len() - 1,
            (Some(index), false) => index - 1,
            (None, true) => 0,
        };
        self.search_match = Some(next);
        let anchor = self.search_matches[next];
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        if let Some(row) = stream.index_of_anchor(anchor) {
            self.selected_file = anchor_file(anchor);
            self.jump_to_row(row);
            self.ensure_sidebar_selection_visible();
        } else {
            let selected_file = anchor_file(anchor);
            self.set_context(MAX_CONTEXT_LINES);
            let AppState::Ready(stream) = &self.state else {
                return;
            };
            if let Some(row) = stream.index_of_anchor(anchor) {
                self.selected_file = selected_file;
                self.jump_to_row(row);
                self.ensure_sidebar_selection_visible();
            }
        }
    }

    fn set_context(&mut self, context_lines: usize) {
        let context_lines = context_lines.min(MAX_CONTEXT_LINES);
        if context_lines == self.context_lines {
            return;
        }
        let anchor = self.current_anchor();
        self.context_lines = context_lines;
        self.rebuild_stream(anchor);
        self.clamp_scroll();
    }

    fn select_file_by(&mut self, delta: isize) {
        let Some(file_count) = self.file_count() else {
            return;
        };
        let last = file_count.saturating_sub(1);
        self.selected_file = self.selected_file.saturating_add_signed(delta).min(last);
        self.jump_to_selected_file();
    }

    fn select_sidebar_row(&mut self, row: u16) {
        if row == 0 {
            return;
        }
        let Some(file_count) = self.file_count() else {
            return;
        };
        let file = self.sidebar_offset.saturating_add(usize::from(row - 1));
        if file < file_count {
            self.selected_file = file;
            self.focus = Focus::Sidebar;
            self.jump_to_selected_file();
        }
    }

    fn select_review_row(&mut self, row: u16) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let index = self.scroll.saturating_add(usize::from(row));
        if let Some(file) = stream.file_at(index) {
            self.selected_file = file;
            self.focus = Focus::Review;
            self.ensure_sidebar_selection_visible();
        }
    }

    fn jump_to_selected_file(&mut self) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        if let Some(row) = stream.file_position(self.selected_file) {
            self.jump_to_row(row);
            self.ensure_sidebar_selection_visible();
        }
    }

    fn file_count(&self) -> Option<usize> {
        match &self.state {
            AppState::Ready(stream) => Some(stream.changeset().files().len()),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        }
    }

    fn max_scroll(&self) -> usize {
        match &self.state {
            AppState::Ready(stream) => stream.len().saturating_sub(self.viewport_height().max(1)),
            AppState::Loading | AppState::Empty | AppState::Error(_) => 0,
        }
    }

    fn viewport_height(&self) -> usize {
        usize::from(self.areas().review.height)
    }

    fn sidebar_file_height(&self) -> usize {
        usize::from(self.areas().sidebar.height.saturating_sub(1)).max(1)
    }

    fn set_scroll(&mut self, value: usize) {
        let next = value.min(self.max_scroll());
        let moved = next != self.scroll;
        self.scroll = next;
        self.scroll_anchor = match &self.state {
            AppState::Ready(stream) => stream.anchor_at(self.scroll),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        if moved {
            self.sync_selected_file();
        }
    }

    fn jump_to_row(&mut self, value: usize) {
        self.scroll = value.min(self.max_scroll());
        self.scroll_anchor = match &self.state {
            AppState::Ready(stream) => stream.anchor_at(value),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn scroll_by(&mut self, amount: usize) {
        self.set_scroll(self.scroll.saturating_add(amount));
    }

    fn scroll_up(&mut self, amount: usize) {
        self.set_scroll(self.scroll.saturating_sub(amount));
    }

    fn sync_selected_file(&mut self) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        if let Some(file) = stream.file_at(self.scroll) {
            self.selected_file = file;
            self.ensure_sidebar_selection_visible();
        }
    }

    fn ensure_sidebar_selection_visible(&mut self) {
        let height = self.sidebar_file_height();
        if self.selected_file < self.sidebar_offset {
            self.sidebar_offset = self.selected_file;
        } else if self.selected_file >= self.sidebar_offset.saturating_add(height) {
            self.sidebar_offset = self.selected_file.saturating_add(1).saturating_sub(height);
        }
    }
}

const fn anchor_file(anchor: RowAnchor) -> usize {
    match anchor {
        RowAnchor::File { file }
        | RowAnchor::Binary { file }
        | RowAnchor::Hunk { file, .. }
        | RowAnchor::Line { file, .. }
        | RowAnchor::MissingNewline { file, .. } => file,
    }
}

fn contains_query(source: &str, query: &str) -> bool {
    if source.is_ascii() && query.is_ascii() {
        source.to_ascii_lowercase().contains(&query.to_ascii_lowercase())
    } else {
        source.contains(query)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    const PATCH: &[u8] = b"--- a/first.txt\n+++ b/first.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n--- a/second.txt\n+++ b/second.txt\n@@ -1 +1 @@\n-before\n+after\n";

    #[test]
    fn navigation_scrolling_is_bounded_by_the_stream_and_viewport() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(50, 6);
        app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let AppState::Ready(stream) = app.state() else {
            panic!("non-empty patch is ready");
        };
        let stream_len = stream.len();
        assert_eq!(app.scroll(), stream_len - 4);

        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.scroll(), stream_len.saturating_sub(8));
    }

    #[test]
    fn navigation_sidebar_keyboard_and_mouse_select_the_same_file() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut keyboard = App::ready(&changeset);
        keyboard.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));

        let mut mouse = App::ready(&changeset);
        let sidebar = mouse.areas().sidebar;
        mouse.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: sidebar.x,
            row: sidebar.y + 2,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(keyboard.selected_file(), 1);
        assert_eq!(mouse.selected_file(), keyboard.selected_file());
        assert_eq!(mouse.scroll(), keyboard.scroll());
    }

    #[test]
    fn navigation_resize_and_layout_switch_preserve_the_source_anchor() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(64, 6);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let before = app.current_anchor();

        app.resize(120, 6);
        assert_eq!(app.current_anchor(), before);
        assert_eq!(app.selected_file(), 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.current_anchor(), before);
        assert_eq!(app.layout_mode(), LayoutMode::Unified);
    }

    #[test]
    fn navigation_resize_preserves_a_selected_file_when_the_whole_stream_fits() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 1);

        app.resize(120, 40);
        assert_eq!(app.selected_file(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 1);
    }

    #[test]
    fn navigation_hunk_keys_cross_file_boundaries() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(50, 6);
        app.handle_key(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 1);
    }

    #[test]
    fn search_moves_forward_and_backward_across_file_boundaries() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        for key in ['/', 'e'] {
            app.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 0);
        assert_eq!(app.search_status(), Some((1, 4)));

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE));
        assert_eq!(app.selected_file(), 0);
    }

    #[test]
    fn wrapping_and_context_controls_preserve_changed_line_anchors() {
        let patch = b"--- a/file.txt\n+++ b/file.txt\n@@ -1,9 +1,9 @@\n one\n two\n three\n four\n-old value that is deliberately much longer than the review pane\n+new value that is deliberately much longer than the review pane\n six\n seven\n eight\n nine\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        let target = RowAnchor::Line { file: 0, hunk: 0, line: 4 };
        let AppState::Ready(stream) = app.state() else {
            panic!("text patch is ready");
        };
        app.jump_to_row(stream.index_of_anchor(target).expect("changed line is indexed"));

        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(app.current_anchor(), Some(target));
        app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE));
        assert_eq!(app.current_anchor(), Some(target));
    }
}
