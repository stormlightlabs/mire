use std::cell::RefCell;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mire_core::{Author, Changeset, NoteId, NoteStatus, Provenance, Review, ReviewNote};
use ratatui::layout::{Position, Rect};

use crate::layout::UiAreas;
use crate::navigation::{Action, Focus, action_for};
use crate::note_filter::NoteFilter;
use crate::notes::{EditorTarget, LineSelection, NoteEditor};
use crate::stream::{LayoutMode, ReviewStream, RowAnchor, RowKey};
use crate::syntax::SyntaxCache;
use crate::theme::ThemeFamily;

const DEFAULT_TERMINAL_HEIGHT: u16 = 24;
const DEFAULT_TERMINAL_WIDTH: u16 = 80;
const DEFAULT_CONTEXT_LINES: usize = 3;
const MAX_CONTEXT_LINES: usize = 100;
const MAX_SEARCH_MATCHES: usize = 100_000;

#[derive(Clone, Debug)]
/// Presentation state retained while watched content is unavailable or replaced.
pub struct AppPosition {
    context_lines: usize,
    focus: Focus,
    help_visible: bool,
    layout_mode: LayoutMode,
    note_filter_file_path: Option<Vec<u8>>,
    note_filter: NoteFilter,
    row_offset: usize,
    row_path: Option<Vec<u8>>,
    selected_path: Option<Vec<u8>>,
    wrap_lines: bool,
}

/// Presentation preferences supplied when an interactive review starts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppOptions {
    /// Inkjet language name or alias applied to every text file.
    pub language_override: Option<String>,
    /// Built-in theme family selected for interactive rendering.
    pub theme: ThemeFamily,
    /// Human identity attributed to notes and decisions created in this session.
    pub human_author: Option<Author>,
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
    review_cursor: usize,
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
    review: Option<Review>,
    author: Author,
    note_filter: NoteFilter,
    filter_visible: bool,
    line_selection: Option<LineSelection>,
    editor: Option<NoteEditor>,
    interaction_error: Option<String>,
    save_requested: bool,
    save_error: Option<String>,
    close_editor_after_save: bool,
    dirty: bool,
    quit_after_save: bool,
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

    /// Creates an error state without discarding the session's presentation preferences.
    pub fn error_with_options(message: impl Into<String>, options: AppOptions) -> Self {
        Self::with_state(AppState::Error(message.into()), options)
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

    /// Creates an editable application around a durable review.
    pub fn review_with_options(review: &'a Review, options: AppOptions) -> Self {
        let areas = UiAreas::new(Rect::new(0, 0, DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT));
        let layout = LayoutMode::Automatic.resolve(areas.review.width);
        let state = if review.changeset().files().is_empty() {
            AppState::Empty
        } else {
            AppState::Ready(ReviewStream::with_notes_presentation(
                review.changeset(),
                review.notes(),
                &(0..review.notes().len()).collect::<Vec<_>>(),
                layout,
                DEFAULT_CONTEXT_LINES,
                None,
            ))
        };
        let mut app = Self::with_state(state, options);
        app.review = Some(review.clone());
        app
    }

    /// Returns the content currently displayed by the application.
    pub const fn state(&self) -> &AppState<'_> {
        &self.state
    }

    /// Returns the first visible row in the continuous stream.
    pub const fn scroll(&self) -> usize {
        self.scroll
    }

    /// Reports whether a stream row is the active editable-review cursor.
    pub const fn row_selected(&self, row: usize) -> bool {
        self.review.is_some() && self.review_cursor == row
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

    /// Returns the editable review when this application opened a review file.
    pub const fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }

    /// Reports whether the persistence boundary should save the current review.
    pub const fn save_requested(&self) -> bool {
        self.save_requested
    }

    /// Completes a requested save while retaining unsaved state on failure.
    pub fn finish_save(&mut self, result: Result<(), String>) {
        self.save_requested = false;
        match result {
            Ok(()) => {
                self.dirty = false;
                self.save_error = None;
                if self.close_editor_after_save {
                    self.editor = None;
                    self.line_selection = None;
                    self.close_editor_after_save = false;
                }
                if self.quit_after_save {
                    self.should_quit = true;
                }
            }
            Err(error) => {
                self.dirty = true;
                self.save_error = Some(error);
            }
        }
    }

    /// Returns the active range label, if range selection is in progress.
    pub fn selection_label(&self) -> Option<String> {
        self.line_selection.map(LineSelection::label)
    }

    /// Returns the active note editor.
    pub const fn editor(&self) -> Option<&NoteEditor> {
        self.editor.as_ref()
    }

    /// Reports whether the filter controls are visible.
    pub const fn filter_visible(&self) -> bool {
        self.filter_visible
    }

    /// Returns a concise description of active note filters.
    pub fn filter_summary(&self) -> String {
        self.note_filter.summary()
    }

    /// Returns the latest recoverable interaction or save failure.
    pub fn note_error(&self) -> Option<&str> {
        self.save_error.as_deref().or(self.interaction_error.as_deref())
    }

    /// Reports whether the in-memory review has changes not confirmed on disk.
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Reports whether replacing an editable review would preserve all unsaved work.
    pub const fn can_reload(&self) -> bool {
        !self.dirty && !self.save_requested && self.editor.is_none()
    }

    /// Captures navigation, layout, and filter state for a watched reload.
    pub fn position(&self) -> AppPosition {
        let (row_path, row_offset) = match &self.state {
            AppState::Ready(stream) => {
                let row = self.current_row_index(stream);
                let file = stream.file_at(row).unwrap_or(self.selected_file);
                let start = stream.file_position(file).unwrap_or(row);
                (file_path_bytes(stream.file(file)), row.saturating_sub(start))
            }
            AppState::Loading | AppState::Empty | AppState::Error(_) => (None, 0),
        };
        let selected_path = match &self.state {
            AppState::Ready(stream) => stream
                .changeset()
                .files()
                .get(self.selected_file)
                .and_then(file_path_bytes),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let note_filter_file_path = match (&self.state, self.note_filter.file()) {
            (AppState::Ready(stream), Some(file)) => stream.changeset().files().get(file).and_then(file_path_bytes),
            _ => None,
        };
        AppPosition {
            context_lines: self.context_lines,
            focus: self.focus,
            help_visible: self.help_visible,
            layout_mode: self.layout_mode,
            note_filter_file_path,
            note_filter: self.note_filter,
            row_offset,
            row_path,
            selected_path,
            wrap_lines: self.wrap_lines,
        }
    }

    /// Restores captured state by file identity and logical row offset when possible.
    pub fn restore_position(&mut self, position: &AppPosition) {
        self.context_lines = position.context_lines;
        self.focus = position.focus;
        self.help_visible = position.help_visible;
        self.layout_mode = position.layout_mode;
        self.note_filter = position.note_filter;
        self.wrap_lines = position.wrap_lines;

        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let selected_file = position
            .selected_path
            .as_deref()
            .and_then(|path| find_file(stream.changeset(), path))
            .unwrap_or_else(|| {
                self.selected_file
                    .min(stream.changeset().files().len().saturating_sub(1))
            });
        let remapped_filter = position
            .note_filter_file_path
            .as_deref()
            .and_then(|path| find_file(stream.changeset(), path));
        self.note_filter.set_file(remapped_filter);
        self.selected_file = selected_file;
        self.rebuild_stream(None);

        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let row_file = position
            .row_path
            .as_deref()
            .and_then(|path| find_file(stream.changeset(), path))
            .unwrap_or(selected_file);
        let start = stream.file_position(row_file).unwrap_or(0);
        let end = stream
            .file_position(row_file.saturating_add(1))
            .unwrap_or_else(|| stream.len());
        let target = start.saturating_add(position.row_offset).min(end.saturating_sub(1));
        if self.review.is_some() {
            self.set_review_cursor(target);
        } else {
            self.jump_to_row(target);
            self.selected_file = selected_file;
            self.ensure_sidebar_selection_visible();
        }
    }

    /// Returns one note by its stable stream index.
    pub fn note(&self, index: usize) -> Option<&ReviewNote> {
        self.review.as_ref()?.notes().get(index)
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
        if self.editor.is_some() {
            self.handle_editor_key(key);
        } else if self.filter_visible {
            self.handle_filter_key(key);
        } else if self.search_input {
            self.handle_search_key(key);
        } else if self.handle_note_key(key) {
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
                self.apply_note_mouse_action(mouse.column.saturating_sub(areas.review.x));
            }
            MouseEventKind::Down(MouseButton::Right) if areas.review.contains(position) => {
                self.select_review_row(mouse.row.saturating_sub(areas.review.y));
                self.open_note_editor_from_current(mouse.column < areas.review.x + areas.review.width / 2);
            }
            _ => {}
        }
    }

    fn with_state(state: AppState<'a>, options: AppOptions) -> Self {
        let syntax_cache = RefCell::new(SyntaxCache::new(options.language_override.as_deref()));
        let author = options.human_author.unwrap_or_else(|| {
            Author::new("local-human", Some("Local reviewer".to_owned())).expect("static author is valid")
        });
        Self {
            state,
            focus: Focus::Review,
            layout_mode: LayoutMode::Automatic,
            scroll: 0,
            review_cursor: 0,
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
            review: None,
            author,
            note_filter: NoteFilter::default(),
            filter_visible: false,
            line_selection: None,
            editor: None,
            interaction_error: None,
            save_requested: false,
            save_error: None,
            close_editor_after_save: false,
            dirty: false,
            quit_after_save: false,
            syntax_cache,
            should_quit: false,
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Quit if self.dirty => {
                self.quit_after_save = true;
                self.save_requested = true;
            }
            Action::Quit => self.should_quit = true,
            Action::ToggleHelp => self.help_visible = !self.help_visible,
            Action::ToggleFocus => self.focus = self.focus.toggle(),
            Action::MoveDown if matches!(self.focus, Focus::Sidebar) => self.select_file_by(1),
            Action::MoveDown if self.review.is_some() => self.move_review_cursor(1),
            Action::MoveDown => self.scroll_by(1),
            Action::MoveUp if matches!(self.focus, Focus::Sidebar) => self.select_file_by(-1),
            Action::MoveUp if self.review.is_some() => self.move_review_cursor(-1),
            Action::MoveUp => self.scroll_up(1),
            Action::PageDown if self.review.is_some() => self.move_review_cursor(self.viewport_height() as isize),
            Action::PageDown => self.scroll_by(self.viewport_height().max(1)),
            Action::PageUp if self.review.is_some() => self.move_review_cursor(-(self.viewport_height() as isize)),
            Action::PageUp => self.scroll_up(self.viewport_height().max(1)),
            Action::FirstRow if self.review.is_some() => self.set_review_cursor(0),
            Action::FirstRow => self.set_scroll(0),
            Action::LastRow if self.review.is_some() => self.set_review_cursor(usize::MAX),
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
        self.extend_selection_to_current();
    }

    fn handle_note_key(&mut self, key: KeyEvent) -> bool {
        if self.review.is_none() {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('s')) {
            if self.dirty {
                self.save_requested = true;
            }
            return true;
        }
        match key.code {
            KeyCode::Char('v') => self.toggle_line_selection(),
            KeyCode::Char('c') => self.open_note_editor_from_current(false),
            KeyCode::Char('e') => self.edit_selected_note(),
            KeyCode::Char('p') => self.jump_note(true),
            KeyCode::Char('P') => self.jump_note(false),
            KeyCode::Char('f') => self.filter_visible = true,
            KeyCode::Char('r') => self.change_selected_note_status(NoteStatus::Resolved),
            KeyCode::Char('d') => self.change_selected_note_status(NoteStatus::Dismissed),
            KeyCode::Char('o') => self.change_selected_note_status(NoteStatus::Open),
            KeyCode::Char('a') => self.change_selected_note_status(NoteStatus::AcceptedRisk),
            _ => return false,
        }
        true
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter if self.dirty && self.save_error.is_some() && self.close_editor_after_save => {
                self.save_requested = true;
            }
            KeyCode::Enter => self.commit_editor(),
            KeyCode::Esc => {
                self.editor = None;
                self.close_editor_after_save = false;
                self.interaction_error = None;
            }
            KeyCode::Backspace => {
                self.save_error = None;
                self.close_editor_after_save = false;
                if let Some(editor) = &mut self.editor {
                    editor.backspace();
                }
            }
            KeyCode::Tab => {
                self.save_error = None;
                self.close_editor_after_save = false;
                if let Some(editor) = &mut self.editor {
                    editor.cycle_severity();
                }
            }
            KeyCode::BackTab => {
                self.save_error = None;
                self.close_editor_after_save = false;
                if let Some(editor) = &mut self.editor {
                    editor.cycle_annotation_kind();
                }
            }
            KeyCode::Char(character) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.save_error = None;
                self.close_editor_after_save = false;
                if let Some(editor) = &mut self.editor {
                    editor.push(character);
                }
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.filter_visible = false,
            KeyCode::Char('a') => self.note_filter.cycle_author_kind(),
            KeyCode::Char('s') => self.note_filter.cycle_status(),
            KeyCode::Char('v') => self.note_filter.cycle_severity(),
            KeyCode::Char('k') => self.note_filter.cycle_annotation_kind(),
            KeyCode::Char('i') => self.note_filter.cycle_file(self.file_count().unwrap_or_default()),
            KeyCode::Char('c') => self.note_filter.clear(),
            _ => return,
        }
        let anchor = self.current_anchor();
        self.rebuild_stream(anchor);
        self.clamp_scroll();
    }

    fn toggle_line_selection(&mut self) {
        if self.line_selection.is_some() {
            self.line_selection = None;
            return;
        }
        self.line_selection = self.selection_from_current(false);
        if self.line_selection.is_none() {
            self.interaction_error = Some("select a source line before starting a range".to_owned());
        }
    }

    fn extend_selection_to_current(&mut self) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let Some(row) = stream.row(self.current_row_index(stream)) else {
            return;
        };
        if let Some(selection) = &mut self.line_selection {
            selection.extend_to_row(stream, row);
        }
    }

    fn selection_from_current(&self, prefer_old: bool) -> Option<LineSelection> {
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        LineSelection::from_row(stream, stream.row(self.current_row_index(stream))?, prefer_old)
    }

    fn open_note_editor_from_current(&mut self, prefer_old: bool) {
        if let Some(note) = self.selected_note().cloned() {
            self.editor = Some(NoteEditor::edit(&note));
            return;
        }
        let selection = self.line_selection.or_else(|| self.selection_from_current(prefer_old));
        if let Some(selection) = selection {
            self.line_selection = Some(selection);
            self.editor = Some(NoteEditor::create(selection));
            self.interaction_error = None;
        } else {
            self.interaction_error = Some("select a source line before creating a note".to_owned());
        }
    }

    fn edit_selected_note(&mut self) {
        if let Some(note) = self.selected_note().cloned() {
            self.editor = Some(NoteEditor::edit(&note));
            self.interaction_error = None;
        }
    }

    fn commit_editor(&mut self) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        if editor.body().trim().is_empty() {
            self.interaction_error = Some("note text cannot be empty".to_owned());
            return;
        }
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let mut created_note_id = None;
        let updated = match editor.target() {
            EditorTarget::New(selection) => {
                let AppState::Ready(stream) = &self.state else {
                    return;
                };
                let anchor = match selection.anchor(stream) {
                    Ok(anchor) => anchor,
                    Err(error) => {
                        self.interaction_error = Some(error.to_string());
                        return;
                    }
                };
                let note_id = next_human_note_id(review);
                let note = match ReviewNote::new(
                    note_id.clone(),
                    anchor,
                    self.author.clone(),
                    editor.severity(),
                    NoteStatus::Open,
                    editor.body().to_owned(),
                    Provenance::Human,
                ) {
                    Ok(note) => note.with_annotation_kind(editor.annotation_kind()),
                    Err(error) => {
                        self.interaction_error = Some(error.to_string());
                        return;
                    }
                };
                match review.import_notes(vec![note]) {
                    Ok(updated) => {
                        created_note_id = Some(note_id);
                        updated
                    }
                    Err(error) => {
                        self.interaction_error = Some(error.failures().first().map_or_else(
                            || "note import failed".to_owned(),
                            |failure| failure.error().to_string(),
                        ));
                        return;
                    }
                }
            }
            EditorTarget::Existing(note_id) => match review.edit_note(
                note_id,
                editor.body().to_owned(),
                editor.severity(),
                editor.annotation_kind(),
            ) {
                Ok(updated) => updated,
                Err(error) => {
                    self.interaction_error = Some(error.to_string());
                    return;
                }
            },
        };
        self.review = Some(updated);
        if let (Some(editor), Some(note_id)) = (&mut self.editor, created_note_id) {
            editor.set_existing(note_id);
        }
        self.dirty = true;
        self.save_requested = true;
        self.close_editor_after_save = true;
        self.interaction_error = None;
        self.rebuild_stream(self.current_anchor());
    }

    fn change_selected_note_status(&mut self, status: NoteStatus) {
        let Some(note_id) = self.selected_note().map(|note| note.id().clone()) else {
            return;
        };
        let Some(review) = &self.review else {
            return;
        };
        match review.change_note_status(&note_id, status, self.author.clone()) {
            Ok(updated) => {
                self.review = Some(updated);
                self.dirty = true;
                self.save_requested = true;
                self.interaction_error = None;
                self.rebuild_stream(self.current_anchor());
            }
            Err(error) => self.interaction_error = Some(error.to_string()),
        }
    }

    fn jump_note(&mut self, forward: bool) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        let positions = stream
            .visible_keys(0, stream.len())
            .enumerate()
            .filter_map(|(row, key)| matches!(key, RowKey::Note { .. }).then_some(row))
            .collect::<Vec<_>>();
        if positions.is_empty() {
            return;
        }
        let target = if forward {
            positions
                .iter()
                .copied()
                .find(|row| *row > self.scroll)
                .unwrap_or(positions[0])
        } else {
            positions
                .iter()
                .rev()
                .copied()
                .find(|row| *row < self.scroll)
                .unwrap_or(*positions.last().unwrap_or(&positions[0]))
        };
        self.jump_to_row(target);
        self.sync_selected_file();
    }

    fn selected_note(&self) -> Option<&ReviewNote> {
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        let RowKey::Note { note, .. } = stream.row(self.current_row_index(stream))? else {
            return None;
        };
        self.note(note)
    }

    fn apply_note_mouse_action(&mut self, column: u16) {
        if self.selected_note().is_none() {
            return;
        }
        match column {
            0..=5 => self.edit_selected_note(),
            6..=11 => self.change_selected_note_status(NoteStatus::Open),
            12..=20 => self.change_selected_note_status(NoteStatus::Resolved),
            21..=29 => self.change_selected_note_status(NoteStatus::Dismissed),
            30..=35 => self.change_selected_note_status(NoteStatus::AcceptedRisk),
            _ => {}
        }
    }

    fn current_row_index(&self, stream: &ReviewStream<'_>) -> usize {
        if self.review.is_some() {
            return self.review_cursor.min(stream.len().saturating_sub(1));
        }
        self.scroll_anchor
            .and_then(|anchor| stream.index_of_anchor(anchor))
            .unwrap_or(self.scroll)
    }

    fn move_review_cursor(&mut self, delta: isize) {
        self.set_review_cursor(self.review_cursor.saturating_add_signed(delta));
    }

    fn set_review_cursor(&mut self, row: usize) {
        let AppState::Ready(stream) = &self.state else {
            return;
        };
        self.review_cursor = row.min(stream.len().saturating_sub(1));
        let height = self.viewport_height().max(1);
        if self.review_cursor < self.scroll {
            self.scroll = self.review_cursor;
        } else if self.review_cursor >= self.scroll.saturating_add(height) {
            self.scroll = self.review_cursor.saturating_add(1).saturating_sub(height);
        }
        self.scroll_anchor = stream.anchor_at(self.review_cursor);
        self.sync_selected_file();
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
        let wrap_width = self.wrap_lines.then_some(self.areas().review.width);
        let replacement = if let Some(review) = &self.review {
            let visible = visible_note_indices(review, self.note_filter);
            ReviewStream::with_notes_presentation(
                changeset,
                review.notes(),
                &visible,
                layout,
                self.context_lines,
                wrap_width,
            )
        } else {
            ReviewStream::with_presentation(changeset, layout, self.context_lines, wrap_width)
        };
        let target = anchor
            .and_then(|value| replacement.index_of_anchor(value))
            .unwrap_or(self.scroll);
        if self.review.is_some() {
            self.review_cursor = target.min(replacement.len().saturating_sub(1));
            self.scroll = self.scroll.min(replacement.len().saturating_sub(1));
        } else {
            self.scroll = target;
        }
        self.scroll_anchor = anchor;
        self.state = AppState::Ready(replacement);
        if self.review.is_some() {
            self.set_review_cursor(self.review_cursor);
        }
    }

    fn current_anchor(&self) -> Option<RowAnchor> {
        if self.review.is_some() {
            let AppState::Ready(stream) = &self.state else {
                return None;
            };
            return stream.anchor_at(self.review_cursor);
        }
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
            self.jump_to_row(index);
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
        if self.review.is_some() {
            self.review_cursor = next;
        }
        self.scroll_anchor = match &self.state {
            AppState::Ready(stream) => {
                stream.anchor_at(if self.review.is_some() { self.review_cursor } else { self.scroll })
            }
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        if moved {
            self.sync_selected_file();
        }
    }

    fn jump_to_row(&mut self, value: usize) {
        if self.review.is_some() {
            self.set_review_cursor(value);
            return;
        }
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
        let row = if self.review.is_some() { self.review_cursor } else { self.scroll };
        if let Some(file) = stream.file_at(row) {
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

fn file_path_bytes(file: &mire_core::FileDiff) -> Option<Vec<u8>> {
    file.new_side()
        .or_else(|| file.old_side())
        .map(|side| side.path.as_bytes().to_vec())
}

fn find_file(changeset: &Changeset, path: &[u8]) -> Option<usize> {
    changeset.files().iter().position(|file| {
        file.new_side().is_some_and(|side| side.path.as_bytes() == path)
            || file.old_side().is_some_and(|side| side.path.as_bytes() == path)
    })
}

const fn anchor_file(anchor: RowAnchor) -> usize {
    match anchor {
        RowAnchor::File { file }
        | RowAnchor::Binary { file }
        | RowAnchor::Hunk { file, .. }
        | RowAnchor::Line { file, .. }
        | RowAnchor::MissingNewline { file, .. }
        | RowAnchor::Note { file, .. } => file,
    }
}

fn visible_note_indices(review: &Review, filter: NoteFilter) -> Vec<usize> {
    review
        .notes()
        .iter()
        .enumerate()
        .filter_map(|(index, note)| {
            let file = note_file(review.changeset(), note)?;
            filter.includes(note, file).then_some(index)
        })
        .collect()
}

fn note_file(changeset: &Changeset, note: &ReviewNote) -> Option<usize> {
    changeset.files().iter().position(|file| {
        file.old_side().is_some_and(|side| side.path == *note.anchor().path())
            || file.new_side().is_some_and(|side| side.path == *note.anchor().path())
    })
}

fn next_human_note_id(review: &Review) -> NoteId {
    let base = format!("human-{}", review.revision().get().saturating_add(1));
    let mut suffix = 0_u64;
    loop {
        let candidate = if suffix == 0 { base.clone() } else { format!("{base}-{suffix}") };
        if !review.notes().iter().any(|note| note.id().as_str() == candidate) {
            return NoteId::new(candidate).expect("generated note identifiers are bounded and non-empty");
        }
        suffix = suffix.saturating_add(1);
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
    use mire_core::{
        Anchor, AnchorSide, Author, BytePath, ChangesetSource, LineNumber, LineRange, NoteId, NoteSeverity,
        PatchLimits, Provenance, Review, ReviewNote, ReviewRevision, parse_patch,
    };

    use super::*;

    const PATCH: &[u8] = b"--- a/first.txt\n+++ b/first.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n--- a/second.txt\n+++ b/second.txt\n@@ -1 +1 @@\n-before\n+after\n";
    const REORDERED_PATCH: &[u8] = b"--- a/added.txt\n+++ b/added.txt\n@@ -1 +1 @@\n-old\n+added\n--- a/first.txt\n+++ b/first.txt\n@@ -1,2 +1,2 @@\n-old\n+newer\n context\n--- a/second.txt\n+++ b/second.txt\n@@ -1 +1 @@\n-before\n+after again\n";

    #[test]
    fn reload_position_follows_file_identity_and_preserves_presentation() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(100, 12);
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        let position = app.position();

        let changeset = parse_patch(
            REORDERED_PATCH,
            ChangesetSource::Patch { label: None },
            PatchLimits::default(),
        )
        .unwrap();
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        let mut reloaded = App::review_with_options(&review, AppOptions::default());
        reloaded.resize(100, 12);
        reloaded.restore_position(&position);

        assert_eq!(reloaded.selected_file(), 2);
        assert_eq!(reloaded.layout_mode(), LayoutMode::Unified);
        assert!(reloaded.filter_summary().contains("author=human"));
        assert!(reloaded.filter_summary().contains("file=2"));
        let AppState::Ready(stream) = reloaded.state() else {
            panic!("reloaded review is ready");
        };
        assert_eq!(stream.file_at(reloaded.review_cursor), Some(2));
        assert_eq!(
            reloaded.review_cursor.saturating_sub(stream.file_position(2).unwrap()),
            position.row_offset
        );
    }

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

    #[test]
    fn notes_create_edit_disposition_filter_and_recover_after_save_failure() {
        let review = review();
        let mut app = App::review_with_options(
            &review,
            AppOptions {
                human_author: Some(Author::new("human", Some("Reviewer".to_owned())).unwrap()),
                ..AppOptions::default()
            },
        );
        app.resize(100, 6);
        for _ in 0..3 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        for character in "Needs a guard".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.save_requested());
        assert_eq!(app.review().unwrap().notes().len(), 1);
        assert_eq!(
            app.review().unwrap().notes()[0].severity(),
            mire_core::NoteSeverity::Low
        );
        assert_eq!(
            app.review().unwrap().notes()[0].annotation_kind(),
            mire_core::AnnotationKind::Defect
        );
        app.finish_save(Err("disk full".to_owned()));
        assert_eq!(app.editor().unwrap().body(), "Needs a guard");
        assert_eq!(app.note_error(), Some("disk full"));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.finish_save(Ok(()));
        assert!(app.editor().is_none());

        for (key, split) in [('1', false), ('2', true)] {
            app.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
            let AppState::Ready(stream) = app.state() else {
                panic!("review stream is ready");
            };
            let rows = stream.visible_keys(0, stream.len()).collect::<Vec<_>>();
            let note = rows.iter().position(|row| matches!(row, RowKey::Note { .. })).unwrap();
            assert!(if split {
                matches!(rows[note - 1], RowKey::SplitLine { .. })
            } else {
                matches!(rows[note - 1], RowKey::UnifiedLine { .. })
            });
        }

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.review().unwrap().notes()[0].status(), NoteStatus::Resolved);
        app.finish_save(Ok(()));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.finish_save(Ok(()));
        assert_eq!(app.review().unwrap().notes()[0].body(), "Needs a guard!");

        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(app.filter_summary().contains("author=human"));
        let AppState::Ready(stream) = app.state() else {
            panic!("review stream is ready");
        };
        assert!(
            !stream
                .visible_keys(0, stream.len())
                .any(|row| matches!(row, RowKey::Note { .. }))
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        let AppState::Ready(stream) = app.state() else {
            panic!("review stream is ready");
        };
        assert!(
            stream
                .visible_keys(0, stream.len())
                .any(|row| matches!(row, RowKey::Note { .. }))
        );
    }

    #[test]
    fn mouse_can_open_an_editor_and_apply_the_same_status_actions_as_keys() {
        let review = review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(100, 12);
        let review_area = app.areas().review;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: review_area.x + review_area.width - 1,
            row: review_area.y + 3,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.editor().is_some());
        for character in "Mouse note".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.finish_save(Ok(()));
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));

        let AppState::Ready(stream) = app.state() else {
            panic!("review stream is ready");
        };
        let note_row = stream
            .visible_keys(0, stream.len())
            .position(|row| matches!(row, RowKey::Note { .. }))
            .unwrap();
        let visible_row = note_row.saturating_sub(app.scroll()) as u16;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: review_area.x + 12,
            row: review_area.y + visible_row,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.review().unwrap().notes()[0].status(), NoteStatus::Resolved);
    }

    #[test]
    fn notes_expand_their_hunk_when_an_anchored_context_line_would_be_hidden() {
        let patch = b"--- a/file.txt\n+++ b/file.txt\n@@ -1,10 +1,10 @@\n-old\n+new\n two\n three\n four\n five\n six\n seven\n eight\n nine\n ten\n";
        let changeset = parse_patch(patch, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mire_core::FileContent::Text { hunks } = changeset.files()[0].content() else {
            panic!("fixture is textual");
        };
        let range = LineRange::new(LineNumber::new(8).unwrap(), LineNumber::new(8).unwrap()).unwrap();
        let anchor = Anchor::new(
            &changeset,
            BytePath::new(b"file.txt".to_vec()).unwrap(),
            AnchorSide::New,
            range,
            hunks[0].fingerprint(),
        )
        .unwrap();
        let author = Author::new("agent", None).unwrap();
        let note = ReviewNote::new(
            NoteId::new("hidden-context").unwrap(),
            anchor,
            author.clone(),
            NoteSeverity::Medium,
            NoteStatus::Open,
            "This context still matters".to_owned(),
            Provenance::Agent { producer: "fixture".to_owned() },
        )
        .unwrap();
        let review = Review::new(
            ReviewRevision::new(1).unwrap(),
            changeset,
            vec![note],
            vec![
                mire_core::NoteEvent::new(
                    1,
                    NoteId::new("hidden-context").unwrap(),
                    author,
                    mire_core::NoteEventKind::Created { status: NoteStatus::Open },
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let app = App::review_with_options(&review, AppOptions::default());
        let AppState::Ready(stream) = app.state() else {
            panic!("review stream is ready");
        };
        let rows = stream.visible_keys(0, stream.len()).collect::<Vec<_>>();
        let note = rows.iter().position(|row| matches!(row, RowKey::Note { .. })).unwrap();
        assert!(matches!(
            rows[note - 1],
            RowKey::UnifiedLine { line: 8, .. } | RowKey::SplitLine { new: Some(8), .. }
        ));
    }

    fn review() -> Review {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
    }
}
