use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use mire_core::Changeset;

use crate::stream::ReviewStream;

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
#[derive(Debug)]
pub struct App<'a> {
    state: AppState<'a>,
    scroll: usize,
    should_quit: bool,
}

impl<'a> App<'a> {
    /// Creates the initial loading state.
    pub const fn loading() -> Self {
        Self { state: AppState::Loading, scroll: 0, should_quit: false }
    }

    /// Creates an error state with a user-facing message.
    pub fn error(message: impl Into<String>) -> Self {
        Self { state: AppState::Error(message.into()), scroll: 0, should_quit: false }
    }

    /// Creates an empty or ready application from an immutable changeset.
    pub fn ready(changeset: &'a Changeset) -> Self {
        let state = if changeset.files().is_empty() {
            AppState::Empty
        } else {
            AppState::Ready(ReviewStream::new(changeset))
        };
        Self { state, scroll: 0, should_quit: false }
    }

    /// Returns the content currently displayed by the application.
    pub const fn state(&self) -> &AppState<'_> {
        &self.state
    }

    /// Returns the first visible row in the continuous stream.
    pub const fn scroll(&self) -> usize {
        self.scroll
    }

    /// Reports whether the user requested that the application close.
    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Applies one keyboard event and clamps scrolling to the current viewport.
    pub fn handle_key(&mut self, key: KeyEvent, viewport_height: usize) {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(1, viewport_height),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::PageDown => self.scroll_by(viewport_height.max(1), viewport_height),
            KeyCode::PageUp => self.scroll_up(viewport_height.max(1)),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll(viewport_height),
            _ => {}
        }
    }

    fn max_scroll(&self, viewport_height: usize) -> usize {
        match &self.state {
            AppState::Ready(stream) => stream.len().saturating_sub(viewport_height.max(1)),
            AppState::Loading | AppState::Empty | AppState::Error(_) => 0,
        }
    }

    fn scroll_by(&mut self, amount: usize, viewport_height: usize) {
        self.scroll = self.scroll.saturating_add(amount).min(self.max_scroll(viewport_height));
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use mire_core::{ChangesetSource, PatchLimits, parse_patch};

    use super::*;

    const PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n";

    #[test]
    fn scrolling_is_bounded_by_the_stream_and_viewport() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut app = App::ready(&changeset);
        app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), 2);
        let AppState::Ready(stream) = app.state() else {
            panic!("non-empty patch is ready");
        };
        let stream_len = stream.len();
        assert_eq!(app.scroll(), stream_len - 2);

        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), 2);
        assert_eq!(app.scroll(), stream_len.saturating_sub(4));
    }
}
