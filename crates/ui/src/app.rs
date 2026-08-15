use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mire_core::{
    AnchorSide, Author, Changeset, FileContent, FileStatus, Fingerprint, NoteId, NoteSeverity, NoteStatus, Provenance,
    Review, ReviewNote,
};
use ratatui::layout::{Position, Rect};

use crate::layout::UiAreas;
use crate::live::{LiveAction, PresentationKind, PresentationState};
use crate::navigation::{Action, Focus, action_for};
use crate::note_filter::NoteFilter;
use crate::notes::{EditorField, EditorTarget, LineSelection, NoteEditor};
use crate::stream::{CollapseState, LayoutMode, ReviewStream, RowAnchor, RowKey};
use crate::syntax::SyntaxCache;
use crate::theme::ThemeFamily;

const DEFAULT_TERMINAL_HEIGHT: u16 = 24;
const DEFAULT_TERMINAL_WIDTH: u16 = 80;
const DEFAULT_CONTEXT_LINES: usize = 3;
const MAX_CONTEXT_LINES: usize = 100;
const MAX_SEARCH_MATCHES: usize = 100_000;
const NOTE_ACTIONS_WIDTH: u16 = 16;

/// The UI state that determines the currently relevant keyboard actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchState {
    /// The source is being watched for changes.
    Watching,
    /// A watched source was refreshed successfully.
    Refreshed,
    /// The latest watched refresh failed while the displayed review remains available.
    Failed,
}

/// The UI state that determines the currently relevant keyboard actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionMode {
    /// Text is being entered into a note editor.
    Editor,
    /// Note filter controls are open.
    Filter,
    /// A search query is being entered.
    Search,
    /// A changed-file picker is filtering and selecting files.
    FilePicker,
    /// A source range is being selected for a new finding.
    RangeSelection,
    /// The keyboard reference is visible.
    Help,
    /// The review has no source content to navigate.
    Review,
    /// A finding is under the review cursor.
    Finding,
    /// The file navigator has focus.
    Sidebar,
    /// The review cursor is on source or structure.
    Source,
}

/// The highest-priority visual meaning attached to a source gutter cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GutterMark {
    /// The source row has no review-specific gutter mark.
    None,
    /// The source row contains a finding anchor.
    Finding,
    /// The source row belongs to the active range selection.
    Selection,
    /// The source row is under the review cursor.
    Cursor,
}

#[derive(Clone, Debug)]
/// Presentation state retained while watched content is unavailable or replaced.
pub struct AppPosition {
    collapsed_files: BTreeSet<Vec<u8>>,
    collapsed_hunks: BTreeSet<CollapsedHunk>,
    context_lines: usize,
    focus: Focus,
    help_visible: bool,
    layout_mode: LayoutMode,
    note_filter_file_path: Option<Vec<u8>>,
    note_filter: NoteFilter,
    row_offset: usize,
    row_path: Option<Vec<u8>>,
    selected_note_id: Option<String>,
    selected_path: Option<Vec<u8>>,
    sidebar_visible: bool,
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CollapsedHunk {
    path: Vec<u8>,
    fingerprint: Fingerprint,
}

#[derive(Clone, Debug, Default)]
struct FilePicker {
    query: String,
    selected: usize,
    offset: usize,
}

/// Counts the review findings anchored to one changed file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileFindingSummary {
    /// Findings that still need a decision or source change.
    pub open: usize,
    /// Findings with a non-open disposition.
    pub completed: usize,
    /// The most severe open finding, if one is present.
    pub highest_open_severity: Option<NoteSeverity>,
}

/// A changed file shown by the keyboard file picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePickerEntry {
    /// Stable index in the current changeset.
    pub file: usize,
    /// Lossy display text for the repository-relative changed path.
    pub path: String,
    /// The normalized change status.
    pub status: FileStatus,
    /// Added and deleted line counts, when the file is textual.
    pub changes: Option<(usize, usize)>,
    /// Review progress for findings anchored to this file.
    pub findings: FileFindingSummary,
}

/// The deterministic content state shown by the review application.
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
    sidebar_visible: bool,
    terminal_width: u16,
    terminal_height: u16,
    help_visible: bool,
    search_input: bool,
    search_query: String,
    search_matches: Vec<RowAnchor>,
    search_match: Option<usize>,
    file_picker: Option<FilePicker>,
    collapsed_files: BTreeSet<Vec<u8>>,
    collapsed_hunks: BTreeSet<CollapsedHunk>,
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
    watch_state: Option<WatchState>,
    watch_error: Option<String>,
    walkthrough_active: bool,
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
        let areas = UiAreas::new(Rect::new(0, 0, DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT), true);
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
        let areas = UiAreas::new(Rect::new(0, 0, DEFAULT_TERMINAL_WIDTH, DEFAULT_TERMINAL_HEIGHT), true);
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

    /// Reports whether a file's source rows are collapsed.
    pub fn file_collapsed(&self, file: usize) -> bool {
        let AppState::Ready(stream) = &self.state else {
            return false;
        };
        file_path_bytes(stream.file(file)).is_some_and(|path| self.collapsed_files.contains(&path))
    }

    /// Reports whether a hunk's source rows are collapsed.
    pub fn hunk_collapsed(&self, file: usize, hunk: usize) -> bool {
        let AppState::Ready(stream) = &self.state else {
            return false;
        };
        let Some(path) = file_path_bytes(stream.file(file)) else {
            return false;
        };
        self.collapsed_hunks
            .contains(&CollapsedHunk { path, fingerprint: stream.hunk(file, hunk).fingerprint() })
    }

    /// Returns the number of source rows hidden by a collapsed file.
    pub fn collapsed_file_line_count(&self, file: usize) -> usize {
        let AppState::Ready(stream) = &self.state else {
            return 0;
        };
        hidden_file_line_count(stream.file(file))
    }

    /// Returns the number of source rows hidden by a collapsed hunk.
    pub fn collapsed_hunk_line_count(&self, file: usize, hunk: usize) -> usize {
        let AppState::Ready(stream) = &self.state else {
            return 0;
        };
        stream.hunk(file, hunk).lines().len()
    }

    /// Returns aggregate finding state for one changed file.
    pub fn file_finding_summary(&self, file: usize) -> FileFindingSummary {
        self.file_finding_summaries().get(file).copied().unwrap_or_default()
    }

    /// Reports whether the keyboard file picker is open.
    pub const fn file_picker_visible(&self) -> bool {
        self.file_picker.is_some()
    }

    /// Returns the current incremental file-picker filter.
    pub fn file_picker_query(&self) -> Option<&str> {
        self.file_picker.as_ref().map(|picker| picker.query.as_str())
    }

    /// Returns the selected result index in the filtered file picker.
    pub fn file_picker_selected(&self) -> Option<usize> {
        self.file_picker.as_ref().map(|picker| picker.selected)
    }

    /// Returns the first visible result index in the filtered file picker.
    pub fn file_picker_offset(&self) -> Option<usize> {
        self.file_picker.as_ref().map(|picker| picker.offset)
    }

    /// Returns matching changed files and their review progress for the file picker.
    pub fn file_picker_entries(&self) -> Vec<FilePickerEntry> {
        let Some(picker) = &self.file_picker else {
            return Vec::new();
        };
        let AppState::Ready(stream) = &self.state else {
            return Vec::new();
        };
        let summaries = self.file_finding_summaries();
        stream
            .changeset()
            .files()
            .iter()
            .enumerate()
            .filter_map(|(file, diff)| {
                let path = file_path_display(diff);
                contains_query(&path, &picker.query).then_some(FilePickerEntry {
                    file,
                    path,
                    status: diff.status(),
                    changes: file_change_counts(diff),
                    findings: summaries.get(file).copied().unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Returns the screen area occupied by the file-picker dialog.
    pub fn file_picker_area(&self) -> Rect {
        let area = self.areas().body;
        let width = area.width.clamp(1, 80);
        let height = area.height.clamp(1, 16);
        Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        )
    }

    /// Returns the requested unified, split, or automatic layout.
    pub const fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    /// Reports whether the complete binding help is visible.
    pub const fn help_visible(&self) -> bool {
        self.help_visible
    }

    /// Reports whether the file sidebar is visible.
    pub const fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    /// Returns the active search text, including input that has not been submitted.
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Reports whether keyboard text is being entered into search.
    pub const fn search_input(&self) -> bool {
        self.search_input
    }

    /// Returns the active interaction state used by contextual UI chrome.
    pub fn interaction_mode(&self) -> InteractionMode {
        if self.editor.is_some() {
            InteractionMode::Editor
        } else if self.filter_visible {
            InteractionMode::Filter
        } else if self.search_input {
            InteractionMode::Search
        } else if self.file_picker.is_some() {
            InteractionMode::FilePicker
        } else if self.line_selection.is_some() {
            InteractionMode::RangeSelection
        } else if self.help_visible {
            InteractionMode::Help
        } else if !matches!(&self.state, AppState::Ready(stream) if !stream.is_empty()) {
            InteractionMode::Review
        } else if matches!(self.focus, Focus::Sidebar) {
            InteractionMode::Sidebar
        } else if self.selected_note().is_some() {
            InteractionMode::Finding
        } else {
            InteractionMode::Source
        }
    }

    /// Returns the review meaning attached to one source gutter cell.
    pub fn gutter_mark(&self, file: usize, hunk: usize, line: usize, side: AnchorSide) -> GutterMark {
        let AppState::Ready(stream) = &self.state else {
            return GutterMark::None;
        };
        let cursor = matches!(
            stream.row(self.current_row_index(stream)),
            Some(RowKey::UnifiedLine { file: found_file, hunk: found_hunk, line: found_line, .. })
                if found_file == file && found_hunk == hunk && found_line == line
        ) || matches!(
            stream.row(self.current_row_index(stream)),
            Some(RowKey::SplitLine { file: found_file, hunk: found_hunk, old, new, .. })
                if found_file == file && found_hunk == hunk && (old == Some(line) || new == Some(line))
        );
        if cursor {
            return GutterMark::Cursor;
        }

        let source = &stream.hunk(file, hunk).lines()[line];
        let line_number = line_number_on_side(source, side);
        if self
            .line_selection
            .is_some_and(|selection| line_number.is_some_and(|number| selection.contains(file, hunk, side, number)))
        {
            return GutterMark::Selection;
        }

        let Some(review) = &self.review else {
            return GutterMark::None;
        };
        let Some(number) = line_number else {
            return GutterMark::None;
        };
        let Some(path) = (match side {
            AnchorSide::Old => stream.file(file).old_side(),
            AnchorSide::New => stream.file(file).new_side(),
        }) else {
            return GutterMark::None;
        };
        let hunk_fingerprint = stream.hunk(file, hunk).fingerprint();
        if review.notes().iter().any(|note| {
            let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
            anchor.side() == side
                && anchor.path() == &path.path
                && anchor.hunk_fingerprint() == hunk_fingerprint
                && anchor.range().start() <= number
                && number <= anchor.range().end()
        }) {
            GutterMark::Finding
        } else {
            GutterMark::None
        }
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
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        self.line_selection.map(|selection| selection.label(stream))
    }

    /// Returns a new note's side-qualified source location.
    pub fn new_note_location(&self, selection: LineSelection) -> String {
        let AppState::Ready(stream) = &self.state else {
            return "<unknown>".to_owned();
        };
        selection.label(stream)
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

    /// Reports whether local presentation control would interrupt text entry or unsaved review work.
    pub const fn live_control_busy(&self) -> bool {
        self.dirty || self.save_requested || self.editor.is_some() || self.filter_visible || self.search_input
    }

    /// Returns the state of the active filesystem watch, if this session is watched.
    pub const fn watch_state(&self) -> Option<WatchState> {
        self.watch_state
    }

    /// Returns the latest nonfatal watched-refresh failure.
    pub fn watch_error(&self) -> Option<&str> {
        self.watch_error.as_deref()
    }

    /// Updates the visible state of a filesystem watch without replacing the current review.
    pub fn set_watch_state(&mut self, state: WatchState, error: Option<String>) {
        self.watch_state = Some(state);
        self.watch_error = error;
    }

    /// Returns the active walkthrough's finding position and total finding count.
    pub fn walkthrough_progress(&self) -> Option<(usize, usize)> {
        if !self.walkthrough_active {
            return None;
        }
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        let positions = stream
            .visible_keys(0, stream.len())
            .enumerate()
            .filter_map(|(row, key)| matches!(key, RowKey::Note { .. }).then_some(row))
            .collect::<Vec<_>>();
        let current = positions
            .iter()
            .position(|row| *row == self.current_row_index(stream))?;
        Some((current + 1, positions.len()))
    }

    /// Returns compact current-file and overall stream positions.
    pub fn review_progress(&self) -> Option<(usize, usize, usize, usize)> {
        let AppState::Ready(stream) = &self.state else {
            return None;
        };
        let row = self.current_row_index(stream);
        let file = stream.file_at(row)?;
        Some((file + 1, stream.changeset().files().len(), row + 1, stream.len()))
    }

    /// Returns the count of unresolved findings in an editable review.
    pub fn open_finding_count(&self) -> Option<usize> {
        self.review.as_ref().map(|review| {
            review
                .notes()
                .iter()
                .filter(|note| note.status() == NoteStatus::Open)
                .count()
        })
    }

    /// Returns a bounded state snapshot for the live-session protocol.
    pub fn live_state(&self) -> PresentationState {
        let selected_path = match &self.state {
            AppState::Ready(stream) => stream
                .changeset()
                .files()
                .get(self.selected_file)
                .and_then(file_path_bytes),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let selected_note_id = self.selected_note().map(|note| note.id().as_str().to_owned());
        let state = match self.state {
            AppState::Loading => PresentationKind::Loading,
            AppState::Empty => PresentationKind::Empty,
            AppState::Ready(_) => PresentationKind::Ready,
            AppState::Error(_) => PresentationKind::Error,
        };
        let layout = match self.layout_mode {
            LayoutMode::Unified => "unified",
            LayoutMode::Split => "split",
            LayoutMode::Automatic => "automatic",
        };
        PresentationState {
            state,
            selected_path,
            selected_note_id,
            scroll_row: self.scroll,
            layout: layout.to_owned(),
            filter: self.filter_summary(),
            review_revision: self.review.as_ref().map(|review| review.revision().get()),
            walkthrough_active: self.walkthrough_active,
            walkthrough_progress: self.walkthrough_progress(),
        }
    }

    /// Starts a walkthrough and focuses its first visible finding.
    pub fn start_walkthrough(&mut self) -> Result<(), &'static str> {
        if self.live_control_busy() {
            return Err("interaction_busy");
        }
        self.move_walkthrough(true)?;
        self.walkthrough_active = true;
        Ok(())
    }

    /// Ends a walkthrough and returns navigation control to the local user.
    pub fn stop_walkthrough(&mut self) {
        self.walkthrough_active = false;
    }

    /// Applies a live presentation action without changing review data.
    pub fn apply_live_action(&mut self, action: &LiveAction) -> Result<(), &'static str> {
        if self.live_control_busy() {
            return Err("interaction_busy");
        }
        if self.walkthrough_active && matches!(action, LiveAction::Next | LiveAction::Previous) {
            return Err("walkthrough_active");
        }
        match action {
            LiveAction::FocusNote { note_id } => self.focus_live_note(note_id)?,
            LiveAction::FocusLocation { path, side, start_line, end_line } => {
                self.focus_live_location(path, *side, *start_line, *end_line)?
            }
            LiveAction::Next => self.move_walkthrough(true)?,
            LiveAction::Previous => self.move_walkthrough(false)?,
            LiveAction::Inspect | LiveAction::Reload | LiveAction::Walkthrough { .. } => return Err("invalid_request"),
        }
        Ok(())
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
        let selected_note_id = match (&self.state, &self.review) {
            (AppState::Ready(stream), Some(review)) => match stream.row(self.current_row_index(stream)) {
                Some(RowKey::Note { note, .. }) => review.notes().get(note).map(|note| note.id().as_str().to_owned()),
                _ => None,
            },
            _ => None,
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
            collapsed_files: self.collapsed_files.clone(),
            collapsed_hunks: self.collapsed_hunks.clone(),
            context_lines: self.context_lines,
            focus: self.focus,
            help_visible: self.help_visible,
            layout_mode: self.layout_mode,
            note_filter_file_path,
            note_filter: self.note_filter,
            row_offset,
            row_path,
            selected_note_id,
            selected_path,
            sidebar_visible: self.sidebar_visible,
            wrap_lines: self.wrap_lines,
        }
    }

    /// Restores captured state by file identity and logical row offset when possible.
    pub fn restore_position(&mut self, position: &AppPosition) {
        self.collapsed_files = position.collapsed_files.clone();
        self.collapsed_hunks = position.collapsed_hunks.clone();
        self.context_lines = position.context_lines;
        self.focus = position.focus;
        self.help_visible = position.help_visible;
        self.layout_mode = position.layout_mode;
        self.note_filter = position.note_filter;
        self.sidebar_visible = position.sidebar_visible;
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
        self.prune_collapsed_state();

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
        let fallback = start.saturating_add(position.row_offset).min(end.saturating_sub(1));
        let target = position
            .selected_note_id
            .as_deref()
            .and_then(|id| {
                self.review
                    .as_ref()?
                    .notes()
                    .iter()
                    .position(|note| note.id().as_str() == id)
            })
            .and_then(|note| {
                (0..stream.len())
                    .find(|row| matches!(stream.row(*row), Some(RowKey::Note { note: found, .. }) if found == note))
            })
            .unwrap_or(fallback);
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
        UiAreas::new(
            Rect::new(0, 0, self.terminal_width, self.terminal_height),
            self.sidebar_visible,
        )
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
        if matches!(key.code, KeyCode::Esc) && self.dismiss_active_state() {
            return;
        }
        if self.editor.is_some() {
            self.handle_editor_key(key);
        } else if self.filter_visible {
            self.handle_filter_key(key);
        } else if self.search_input {
            self.handle_search_key(key);
        } else if self.file_picker.is_some() {
            self.handle_file_picker_key(key);
        } else if self.handle_note_key(key) {
        } else if let Some(action) = action_for(key) {
            self.apply(action);
        }
    }

    /// Inserts text delivered by a bracketed-paste terminal event into the note body.
    pub fn handle_paste(&mut self, text: &str) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.focused_field() != EditorField::Body {
            return;
        }
        self.save_error = None;
        self.close_editor_after_save = false;
        editor.push_str(text);
    }

    /// Applies mouse scrolling and selection using the same navigation state as keyboard input.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.file_picker.is_some() {
            self.handle_file_picker_mouse(mouse);
            return;
        }
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
                if mouse.column.saturating_sub(areas.review.x) < 3 {
                    self.toggle_current_structure();
                } else {
                    self.apply_note_mouse_action(mouse.column.saturating_sub(areas.review.x));
                }
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
            sidebar_visible: true,
            terminal_width: DEFAULT_TERMINAL_WIDTH,
            terminal_height: DEFAULT_TERMINAL_HEIGHT,
            help_visible: false,
            search_input: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_match: None,
            file_picker: None,
            collapsed_files: BTreeSet::new(),
            collapsed_hunks: BTreeSet::new(),
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
            watch_state: None,
            watch_error: None,
            walkthrough_active: false,
            syntax_cache,
            should_quit: false,
        }
    }

    fn dismiss_active_state(&mut self) -> bool {
        if self.editor.is_some() {
            self.editor = None;
            self.close_editor_after_save = false;
            self.interaction_error = None;
            return true;
        }
        if self.filter_visible {
            self.filter_visible = false;
            return true;
        }
        if self.search_input {
            self.search_input = false;
            self.search_query.clear();
            self.search_matches.clear();
            self.search_match = None;
            return true;
        }
        if self.file_picker.is_some() {
            self.file_picker = None;
            return true;
        }
        if self.line_selection.is_some() {
            self.line_selection = None;
            return true;
        }
        if self.help_visible {
            self.help_visible = false;
            return true;
        }
        if matches!(self.focus, Focus::Sidebar) {
            self.focus = Focus::Review;
            return true;
        }
        false
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
            Action::ToggleSidebar => self.toggle_sidebar(),
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
            Action::ToggleCollapse => self.toggle_current_structure(),
            Action::OpenFilePicker => self.open_file_picker(),
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

    fn toggle_sidebar(&mut self) {
        let anchor = self.current_anchor();
        self.sidebar_visible = !self.sidebar_visible;
        if !self.sidebar_visible {
            self.focus = Focus::Review;
        }
        self.rebuild_stream(anchor);
        self.clamp_scroll();
        self.ensure_sidebar_selection_visible();
    }

    fn toggle_current_structure(&mut self) {
        if !matches!(self.focus, Focus::Review) {
            return;
        }
        let structural = match &self.state {
            AppState::Ready(stream) => stream.row(self.current_row_index(stream)).and_then(|row| match row {
                RowKey::File { file } => {
                    file_path_bytes(stream.file(file)).map(|path| (RowAnchor::File { file }, path, None))
                }
                RowKey::Hunk { file, hunk } => file_path_bytes(stream.file(file)).map(|path| {
                    (
                        RowAnchor::Hunk { file, hunk },
                        path,
                        Some(CollapsedHunk { path: Vec::new(), fingerprint: stream.hunk(file, hunk).fingerprint() }),
                    )
                }),
                _ => None,
            }),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let Some((anchor, path, hunk)) = structural else {
            return;
        };
        match hunk {
            Some(mut hunk) => {
                hunk.path = path;
                if !self.collapsed_hunks.remove(&hunk) {
                    self.collapsed_hunks.insert(hunk);
                }
            }
            None => {
                if !self.collapsed_files.remove(&path) {
                    self.collapsed_files.insert(path);
                }
            }
        }
        self.rebuild_stream(Some(anchor));
        self.clamp_scroll();
        self.ensure_sidebar_selection_visible();
    }

    fn open_file_picker(&mut self) {
        if matches!(&self.state, AppState::Ready(_)) {
            self.file_picker = Some(FilePicker::default());
        }
    }

    fn handle_file_picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.select_file_picker_entry(),
            KeyCode::Down | KeyCode::Char('j') => self.move_file_picker_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_file_picker_selection(-1),
            KeyCode::PageDown => self.move_file_picker_selection(self.file_picker_result_height() as isize),
            KeyCode::PageUp => self.move_file_picker_selection(-(self.file_picker_result_height() as isize)),
            KeyCode::Backspace => {
                if let Some(picker) = &mut self.file_picker {
                    picker.query.pop();
                    picker.selected = 0;
                    picker.offset = 0;
                }
            }
            KeyCode::Char(character) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                if let Some(picker) = &mut self.file_picker {
                    picker.query.push(character);
                    picker.selected = 0;
                    picker.offset = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_file_picker_mouse(&mut self, mouse: MouseEvent) {
        let area = self.file_picker_area();
        let position = Position::new(mouse.column, mouse.row);
        if !area.contains(position) {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => self.move_file_picker_selection(1),
            MouseEventKind::ScrollUp => self.move_file_picker_selection(-1),
            MouseEventKind::Down(MouseButton::Left) => {
                let first_result = area.y.saturating_add(4);
                if mouse.row >= first_result && mouse.row < area.bottom().saturating_sub(1) {
                    let offset = self.file_picker.as_ref().map_or(0, |picker| picker.offset);
                    let selected = offset.saturating_add(usize::from(mouse.row.saturating_sub(first_result)));
                    if selected < self.file_picker_entries().len() {
                        if let Some(picker) = &mut self.file_picker {
                            picker.selected = selected;
                        }
                        self.select_file_picker_entry();
                    }
                }
            }
            _ => {}
        }
    }

    fn move_file_picker_selection(&mut self, delta: isize) {
        let count = self.file_picker_entries().len();
        let height = self.file_picker_result_height();
        let Some(picker) = &mut self.file_picker else {
            return;
        };
        picker.selected = picker
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        if picker.selected < picker.offset {
            picker.offset = picker.selected;
        } else if picker.selected >= picker.offset.saturating_add(height) {
            picker.offset = picker.selected.saturating_add(1).saturating_sub(height);
        }
    }

    fn select_file_picker_entry(&mut self) {
        let selected = self.file_picker.as_ref().map_or(0, |picker| picker.selected);
        let Some(entry) = self.file_picker_entries().get(selected).cloned() else {
            return;
        };
        self.file_picker = None;
        self.selected_file = entry.file;
        self.focus = Focus::Review;
        self.jump_to_selected_file();
    }

    fn file_picker_result_height(&self) -> usize {
        usize::from(self.file_picker_area().height.saturating_sub(5)).max(1)
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
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return false;
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
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => self.commit_editor(),
            KeyCode::Enter => self.edit_body(|editor| editor.push('\n')),
            KeyCode::Esc => {
                self.editor = None;
                self.close_editor_after_save = false;
                self.interaction_error = None;
            }
            KeyCode::Backspace => self.edit_body(NoteEditor::backspace),
            KeyCode::Tab => {
                if let Some(editor) = &mut self.editor {
                    editor.focus_next_field();
                }
            }
            KeyCode::BackTab => {
                if let Some(editor) = &mut self.editor {
                    editor.focus_previous_field();
                }
            }
            KeyCode::Up => self.cycle_editor_field(false),
            KeyCode::Down => self.cycle_editor_field(true),
            KeyCode::Char(character) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.edit_body(|editor| editor.push(character));
            }
            _ => {}
        }
    }

    fn edit_body(&mut self, edit: impl FnOnce(&mut NoteEditor)) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.focused_field() != EditorField::Body {
            return;
        }
        self.save_error = None;
        self.close_editor_after_save = false;
        edit(editor);
    }

    fn cycle_editor_field(&mut self, forward: bool) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        self.save_error = None;
        self.close_editor_after_save = false;
        match (editor.focused_field(), forward) {
            (EditorField::Severity, true) => editor.cycle_severity(),
            (EditorField::Severity, false) => editor.cycle_severity_backward(),
            (EditorField::Kind, true) => editor.cycle_annotation_kind(),
            (EditorField::Kind, false) => editor.cycle_annotation_kind_backward(),
            (EditorField::Body, _) => {}
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

    fn focus_live_note(&mut self, note_id: &str) -> Result<(), &'static str> {
        let Some(review) = &self.review else {
            return Err("not_found");
        };
        let Some(note) = review.notes().iter().position(|note| note.id().as_str() == note_id) else {
            return Err("not_found");
        };
        let target = match &self.state {
            AppState::Ready(stream) => (0..stream.len())
                .find(|row| matches!(stream.row(*row), Some(RowKey::Note { note: found, .. }) if found == note)),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let Some(target) = target else {
            return Err("not_found");
        };
        self.jump_to_row(target);
        Ok(())
    }

    fn focus_live_location(
        &mut self, path: &[u8], side: AnchorSide, start_line: u64, end_line: u64,
    ) -> Result<(), &'static str> {
        if start_line == 0 || end_line < start_line {
            return Err("invalid_request");
        }
        let target = match &self.state {
            AppState::Ready(stream) => stream.changeset().files().iter().enumerate().find_map(|(file, diff)| {
                let matches_side = match side {
                    AnchorSide::Old => diff.old_side().is_some_and(|value| value.path.as_bytes() == path),
                    AnchorSide::New => diff.new_side().is_some_and(|value| value.path.as_bytes() == path),
                };
                if !matches_side {
                    return None;
                }
                let FileContent::Text { hunks } = diff.content() else {
                    return None;
                };
                hunks.iter().enumerate().find_map(|(hunk, hunk_data)| {
                    let contains_start = hunk_data
                        .lines()
                        .iter()
                        .any(|line| line_number_on_side(line, side).is_some_and(|number| number.get() == start_line));
                    let contains_end = hunk_data
                        .lines()
                        .iter()
                        .any(|line| line_number_on_side(line, side).is_some_and(|number| number.get() == end_line));
                    (contains_start && contains_end).then_some((file, hunk))
                })
            }),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let Some((file, hunk)) = target else {
            return Err("not_found");
        };
        let row = match &self.state {
            AppState::Ready(stream) => stream
                .hunk_positions()
                .find_map(|(row, found_file, found_hunk)| (found_file == file && found_hunk == hunk).then_some(row)),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        self.selected_file = file;
        let Some(row) = row else {
            return Err("not_found");
        };
        self.jump_to_row(row);
        self.ensure_sidebar_selection_visible();
        Ok(())
    }

    /// Moves a live walkthrough to its next or previous visible finding.
    pub fn move_walkthrough(&mut self, forward: bool) -> Result<(), &'static str> {
        let has_notes = matches!(&self.state, AppState::Ready(stream) if stream.visible_keys(0, stream.len()).any(|key| matches!(key, RowKey::Note { .. })));
        if !has_notes {
            return Err("not_found");
        }
        self.jump_note(forward);
        Ok(())
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
        let start = self.areas().review.width.saturating_sub(NOTE_ACTIONS_WIDTH);
        if column < start {
            return;
        }
        match column - start {
            1..=3 => self.edit_selected_note(),
            4..=6 => self.change_selected_note_status(NoteStatus::Open),
            7..=9 => self.change_selected_note_status(NoteStatus::Resolved),
            10..=12 => self.change_selected_note_status(NoteStatus::Dismissed),
            13..=15 => self.change_selected_note_status(NoteStatus::AcceptedRisk),
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
        let (collapsed_files, collapsed_hunks) =
            collapsed_row_indices(changeset, &self.collapsed_files, &self.collapsed_hunks);
        let layout = self.layout_mode.resolve(self.areas().review.width);
        let wrap_width = self.wrap_lines.then_some(self.areas().review.width);
        let replacement = if let Some(review) = &self.review {
            let visible = visible_note_indices(review, self.note_filter);
            ReviewStream::with_notes_presentation_collapsed(
                changeset,
                review.notes(),
                &visible,
                layout,
                self.context_lines,
                wrap_width,
                CollapseState::new(&collapsed_files, &collapsed_hunks),
            )
        } else {
            ReviewStream::with_presentation_collapsed(
                changeset,
                layout,
                self.context_lines,
                wrap_width,
                &collapsed_files,
                &collapsed_hunks,
            )
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
            if self.expand_for_anchor(anchor) {
                self.rebuild_stream(Some(anchor));
            } else {
                self.set_context(MAX_CONTEXT_LINES);
            }
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

    fn expand_for_anchor(&mut self, anchor: RowAnchor) -> bool {
        let (file, hunk) = match anchor {
            RowAnchor::File { file } | RowAnchor::Binary { file } => (file, None),
            RowAnchor::Hunk { file, hunk }
            | RowAnchor::Line { file, hunk, .. }
            | RowAnchor::MissingNewline { file, hunk, .. } => (file, Some(hunk)),
            RowAnchor::Note { .. } => return false,
        };
        let collapsed = match &self.state {
            AppState::Ready(stream) => file_path_bytes(stream.file(file)).map(|path| {
                let hunk = hunk.map(|hunk| CollapsedHunk {
                    path: path.clone(),
                    fingerprint: stream.hunk(file, hunk).fingerprint(),
                });
                (path, hunk)
            }),
            AppState::Loading | AppState::Empty | AppState::Error(_) => None,
        };
        let Some((path, hunk)) = collapsed else {
            return false;
        };
        let mut changed = self.collapsed_files.remove(&path);
        if let Some(hunk) = hunk {
            changed |= self.collapsed_hunks.remove(&hunk);
        }
        changed
    }

    fn prune_collapsed_state(&mut self) {
        let AppState::Ready(stream) = &self.state else {
            self.collapsed_files.clear();
            self.collapsed_hunks.clear();
            return;
        };
        let file_paths = stream
            .changeset()
            .files()
            .iter()
            .filter_map(file_path_bytes)
            .collect::<BTreeSet<_>>();
        let hunks = stream
            .changeset()
            .files()
            .iter()
            .flat_map(|diff| {
                let path = file_path_bytes(diff);
                match diff.content() {
                    FileContent::Text { hunks } => hunks
                        .iter()
                        .filter_map(move |hunk| {
                            path.clone()
                                .map(|path| CollapsedHunk { path, fingerprint: hunk.fingerprint() })
                        })
                        .collect::<Vec<_>>(),
                    FileContent::Binary => Vec::new(),
                }
            })
            .collect::<BTreeSet<_>>();
        self.collapsed_files.retain(|path| file_paths.contains(path));
        self.collapsed_hunks.retain(|hunk| hunks.contains(hunk));
    }

    fn file_finding_summaries(&self) -> Vec<FileFindingSummary> {
        let AppState::Ready(stream) = &self.state else {
            return Vec::new();
        };
        let mut files = BTreeMap::new();
        for (index, file) in stream.changeset().files().iter().enumerate() {
            if let Some(side) = file.old_side() {
                files.insert(side.path.as_bytes().to_vec(), index);
            }
            if let Some(side) = file.new_side() {
                files.insert(side.path.as_bytes().to_vec(), index);
            }
        }
        let mut summaries = vec![FileFindingSummary::default(); stream.changeset().files().len()];
        for note in self.review.as_ref().into_iter().flat_map(|review| review.notes()) {
            let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
            let Some(summary) = files
                .get(anchor.path().as_bytes())
                .and_then(|file| summaries.get_mut(*file))
            else {
                continue;
            };
            if note.status() == NoteStatus::Open {
                summary.open += 1;
                if summary
                    .highest_open_severity
                    .is_none_or(|current| severity_rank(note.severity()) > severity_rank(current))
                {
                    summary.highest_open_severity = Some(note.severity());
                }
            } else {
                summary.completed += 1;
            }
        }
        summaries
    }
}

fn collapsed_row_indices(
    changeset: &Changeset, collapsed_files: &BTreeSet<Vec<u8>>, collapsed_hunks: &BTreeSet<CollapsedHunk>,
) -> (BTreeSet<usize>, BTreeSet<(usize, usize)>) {
    let mut files = BTreeSet::new();
    let mut hunks = BTreeSet::new();
    for (file_index, file) in changeset.files().iter().enumerate() {
        let Some(path) = file_path_bytes(file) else {
            continue;
        };
        if collapsed_files.contains(&path) {
            files.insert(file_index);
            continue;
        }
        let FileContent::Text { hunks: source_hunks } = file.content() else {
            continue;
        };
        for (hunk_index, hunk) in source_hunks.iter().enumerate() {
            if collapsed_hunks.contains(&CollapsedHunk { path: path.clone(), fingerprint: hunk.fingerprint() }) {
                hunks.insert((file_index, hunk_index));
            }
        }
    }
    (files, hunks)
}

fn hidden_file_line_count(file: &mire_core::FileDiff) -> usize {
    match file.content() {
        FileContent::Text { hunks } => hunks.iter().map(|hunk| hunk.lines().len()).sum(),
        FileContent::Binary => 1,
    }
}

fn file_path_display(file: &mire_core::FileDiff) -> String {
    let old = file
        .old_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    let new = file
        .new_side()
        .map(|side| String::from_utf8_lossy(side.path.as_bytes()));
    match (old, new) {
        (Some(old), Some(new)) if old != new => format!("{old} -> {new}"),
        (_, Some(new)) => new.into_owned(),
        (Some(old), None) => old.into_owned(),
        (None, None) => "<unknown>".to_owned(),
    }
}

fn file_change_counts(file: &mire_core::FileDiff) -> Option<(usize, usize)> {
    let FileContent::Text { hunks } = file.content() else {
        return None;
    };
    let mut additions = 0;
    let mut deletions = 0;
    for line in hunks.iter().flat_map(|hunk| hunk.lines()) {
        match line.kind() {
            mire_core::LineKind::Addition => additions += 1,
            mire_core::LineKind::Deletion => deletions += 1,
            mire_core::LineKind::Context => {}
        }
    }
    Some((additions, deletions))
}

const fn severity_rank(severity: NoteSeverity) -> u8 {
    match severity {
        NoteSeverity::Note => 0,
        NoteSeverity::Low => 1,
        NoteSeverity::Medium => 2,
        NoteSeverity::High => 3,
        NoteSeverity::Critical => 4,
    }
}

fn line_number_on_side(line: &mire_core::DiffLine, side: AnchorSide) -> Option<mire_core::LineNumber> {
    match side {
        AnchorSide::Old => line.old_line(),
        AnchorSide::New => line.new_line(),
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
    let anchor = note.current_anchor().unwrap_or_else(|| note.anchor());
    changeset
        .files()
        .iter()
        .position(|file| {
            file.old_side().is_some_and(|side| side.path == *anchor.path())
                || file.new_side().is_some_and(|side| side.path == *anchor.path())
        })
        .or_else(|| (!changeset.files().is_empty()).then_some(0))
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
    fn sidebar_toggle_expands_the_review_and_returns_focus_to_it() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(64, 12);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

        assert!(!app.sidebar_visible());
        assert_eq!(app.focus(), Focus::Review);
        assert_eq!(app.areas().sidebar.width, 0);
        assert_eq!(app.areas().sidebar_divider.width, 0);
        assert_eq!(app.areas().review, app.areas().body);

        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert!(app.sidebar_visible());
        assert!(app.areas().sidebar.width > 0);
    }

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
    fn escape_unwinds_transient_states_without_quitting_the_review() {
        let review = review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        app.resize(100, 12);
        for _ in 0..3 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(app.selection_label().is_some());
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(app.editor().is_some());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.editor().is_none());
        assert!(app.selection_label().is_some());

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.selection_label().is_none());
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(app.filter_visible());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.filter_visible());

        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.search_input());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.search_input());
        assert!(app.search_query().is_empty());

        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.help_visible());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.help_visible());
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus(), Focus::Sidebar);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.focus(), Focus::Review);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.should_quit());
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
    fn collapse_controls_hide_source_rows_and_restore_by_file_and_hunk_identity() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(80, 6);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.file_collapsed(0));
        let AppState::Ready(stream) = app.state() else {
            panic!("review stream is ready");
        };
        assert!(
            !stream
                .visible_keys(0, stream.len())
                .any(|row| matches!(row, RowKey::Hunk { file: 0, .. }))
        );

        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.file_collapsed(0));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.hunk_collapsed(0, 0));
        let position = app.position();

        let mut reloaded = App::ready(&changeset);
        reloaded.resize(120, 12);
        reloaded.restore_position(&position);
        assert!(reloaded.hunk_collapsed(0, 0));
        let AppState::Ready(stream) = reloaded.state() else {
            panic!("reloaded review stream is ready");
        };
        assert!(!stream.visible_keys(0, stream.len()).any(|row| matches!(
            row,
            RowKey::UnifiedLine { file: 0, hunk: 0, .. } | RowKey::SplitLine { file: 0, hunk: 0, .. }
        )));

        let mut mouse = App::ready(&changeset);
        let review = mouse.areas().review;
        mouse.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: review.x,
            row: review.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(mouse.file_collapsed(0));
    }

    #[test]
    fn file_picker_filters_incrementally_and_jumps_with_keyboard_and_mouse() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.resize(80, 16);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(app.file_picker_visible());
        for character in "second".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        assert_eq!(app.file_picker_entries().len(), 1);
        assert_eq!(app.file_picker_entries()[0].file, 1);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.file_picker_visible());
        assert_eq!(app.selected_file(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        let picker = app.file_picker_area();
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: picker.x.saturating_add(1),
            row: picker.y.saturating_add(4),
            modifiers: KeyModifiers::NONE,
        });
        assert!(!app.file_picker_visible());
        assert_eq!(app.selected_file(), 0);
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
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

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
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
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
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
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
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
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
            column: review_area.x + review_area.width - NOTE_ACTIONS_WIDTH + 7,
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

    #[test]
    fn editor_keeps_multiline_paste_and_classification_across_focus_changes() {
        let review = review();
        let mut app = App::review_with_options(&review, AppOptions::default());
        for _ in 0..3 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        app.handle_paste("First line\nSecond line");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_paste(" ignored");

        let editor = app.editor().unwrap();
        assert_eq!(editor.body(), "First line\nSecond line\n");
        assert_eq!(editor.severity(), NoteSeverity::Low);
        assert_eq!(editor.annotation_kind(), mire_core::AnnotationKind::Defect);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert!(app.save_requested());
    }

    fn review() -> Review {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap()
    }
}
