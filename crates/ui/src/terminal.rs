use std::io::{self, Stdout, stdout};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use mire_core::{Changeset, Review};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};

use crate::{
    App, AppOptions, LiveAction, LiveControl, LiveResponse, ReviewWrite, Theme, WalkthroughAction, WatchUpdate, render,
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut output = stdout();
        if let Err(error) = execute!(
            output,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
            ),
        ) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(output)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let mut output = stdout();
                let _ = execute!(
                    output,
                    PopKeyboardEnhancementFlags,
                    DisableBracketedPaste,
                    DisableMouseCapture,
                    LeaveAlternateScreen
                );
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

pub fn run(changeset: &Changeset, options: AppOptions, control: Option<LiveControl>) -> io::Result<()> {
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_loop(&mut session.terminal, changeset, options, &theme, control.as_ref())
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_review<F, E>(
    review: &Review, options: AppOptions, mut save: F, control: Option<LiveControl>,
) -> io::Result<()>
where
    F: FnMut(ReviewWrite<'_>) -> Result<(), E>,
    E: std::fmt::Display,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_review_loop(
            &mut session.terminal,
            review,
            options,
            &theme,
            &mut save,
            control.as_ref(),
        )
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_watch<F>(
    changeset: Changeset, options: AppOptions, mut reload: F, control: Option<LiveControl>,
) -> io::Result<()>
where
    F: FnMut(bool) -> WatchUpdate<Changeset>,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_watch_loop(
            &mut session.terminal,
            changeset,
            options,
            &theme,
            &mut poll_event,
            &mut reload,
            control.as_ref(),
        )
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_review_watch<F, E, R>(
    review: Review, options: AppOptions, mut save: F, mut reload: R, control: Option<LiveControl>,
) -> io::Result<()>
where
    F: FnMut(ReviewWrite<'_>) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut(bool) -> WatchUpdate<Review>,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_review_watch_loop(
            &mut session.terminal,
            review,
            options,
            &theme,
            &mut poll_event,
            &mut save,
            &mut reload,
            control.as_ref(),
        )
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, changeset: &Changeset, options: AppOptions, theme: &Theme,
    control: Option<&LiveControl>,
) -> io::Result<()> {
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::ready_with_options(changeset, options);
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        let _ = handle_terminal_event(&mut app, poll_event()?, control, false);
    }
    Ok(())
}

/// Runs a watched changeset session until the quit key.
///
/// `next_event` waits briefly for one terminal event. A key handled in the
/// same pass as a reload ends the session before the reload rebuilds the app,
/// so the quit key is never dropped with the old app.
fn run_watch_loop<B, N, F>(
    terminal: &mut Terminal<B>, mut changeset: Changeset, options: AppOptions, theme: &Theme, next_event: &mut N,
    reload: &mut F, control: Option<&LiveControl>,
) -> io::Result<()>
where
    B: Backend,
    io::Error: From<B::Error>,
    N: FnMut() -> io::Result<Option<Event>>,
    F: FnMut(bool) -> WatchUpdate<Changeset>,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::ready_with_options(&changeset, options.clone());
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    let mut position = app.position();
    app.set_watch_state(crate::WatchState::Watching, None);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        let requested_reload = handle_terminal_event(&mut app, next_event()?, control, true);
        if app.should_quit() {
            break;
        }
        match reload(requested_reload) {
            WatchUpdate::Unchanged => {}
            WatchUpdate::Loaded(updated) => {
                if matches!(app.state(), crate::AppState::Ready(_)) {
                    position = app.position();
                }
                drop(app);
                changeset = updated;
                app = App::ready_with_options(&changeset, options.clone());
                let size = terminal.size()?;
                app.resize(size.width, size.height);
                app.restore_position(&position);
                app.set_watch_state(crate::WatchState::Refreshed, None);
            }
            WatchUpdate::Failed(error) => app.set_watch_state(crate::WatchState::Failed, Some(error)),
            WatchUpdate::Fatal(error) => return Err(io::Error::other(error)),
        }
    }
    Ok(())
}

fn run_review_loop<F, E>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, review: &Review, options: AppOptions, theme: &Theme,
    save: &mut F, control: Option<&LiveControl>,
) -> io::Result<()>
where
    F: FnMut(ReviewWrite<'_>) -> Result<(), E>,
    E: std::fmt::Display,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::review_with_options(review, options);
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        let _ = handle_terminal_event(&mut app, poll_event()?, control, false);
        finish_requested_save(&mut app, save);
    }
    Ok(())
}

/// Runs a watched review session until the quit key, ending it before a
/// reload in the same pass can rebuild the app and drop the quit key.
#[allow(clippy::too_many_arguments)]
fn run_review_watch_loop<B, N, F, E, R>(
    terminal: &mut Terminal<B>, mut review: Review, options: AppOptions, theme: &Theme, next_event: &mut N,
    save: &mut F, reload: &mut R, control: Option<&LiveControl>,
) -> io::Result<()>
where
    B: Backend,
    io::Error: From<B::Error>,
    N: FnMut() -> io::Result<Option<Event>>,
    F: FnMut(ReviewWrite<'_>) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut(bool) -> WatchUpdate<Review>,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::review_with_options(&review, options.clone());
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    let mut position = app.position();
    let mut pending = WatchUpdate::Unchanged;
    app.set_watch_state(crate::WatchState::Watching, None);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        let requested_reload = handle_terminal_event(&mut app, next_event()?, control, true);
        finish_requested_save(&mut app, save);
        if app.should_quit() {
            break;
        }
        if requested_reload || matches!(pending, WatchUpdate::Unchanged) {
            pending = reload(requested_reload);
        }
        if !app.can_reload() {
            continue;
        }
        match std::mem::replace(&mut pending, WatchUpdate::Unchanged) {
            WatchUpdate::Unchanged => {}
            WatchUpdate::Loaded(updated) => {
                if matches!(app.state(), crate::AppState::Ready(_)) {
                    position = app.position();
                }
                drop(app);
                review = updated;
                app = App::review_with_options(&review, options.clone());
                let size = terminal.size()?;
                app.resize(size.width, size.height);
                app.restore_position(&position);
                app.set_watch_state(crate::WatchState::Refreshed, None);
            }
            WatchUpdate::Failed(error) => app.set_watch_state(crate::WatchState::Failed, Some(error)),
            WatchUpdate::Fatal(error) => return Err(io::Error::other(error)),
        }
    }
    Ok(())
}

/// Waits up to [`EVENT_POLL_INTERVAL`] for one terminal event.
fn poll_event() -> io::Result<Option<Event>> {
    if event::poll(EVENT_POLL_INTERVAL)? { event::read().map(Some) } else { Ok(None) }
}

/// Applies one terminal event, if any, then the queued live requests, and
/// reports whether a live client asked for a reload.
fn handle_terminal_event(
    app: &mut App<'_>, event: Option<Event>, control: Option<&LiveControl>, reload_available: bool,
) -> bool {
    match event {
        Some(Event::Key(key)) => app.handle_key(key),
        Some(Event::Mouse(mouse)) => app.handle_mouse(mouse),
        Some(Event::Resize(width, height)) => app.resize(width, height),
        Some(Event::Paste(text)) => app.handle_paste(&text),
        Some(Event::FocusGained | Event::FocusLost) | None => {}
    }
    handle_live_requests(app, control, reload_available)
}

fn handle_live_requests(app: &mut App<'_>, control: Option<&LiveControl>, reload_available: bool) -> bool {
    let Some(control) = control else {
        return false;
    };
    let mut reload_requested = false;
    loop {
        let request = match control.try_recv() {
            Ok(request) => request,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        };
        let response = match request.action {
            LiveAction::Inspect => LiveResponse::State(app.live_state()),
            LiveAction::Reload if reload_available && !app.live_control_busy() => {
                reload_requested = true;
                LiveResponse::ReloadRequested
            }
            LiveAction::Reload if app.live_control_busy() => LiveResponse::Error { code: "interaction_busy" },
            LiveAction::Reload => LiveResponse::Error { code: "reload_unavailable" },
            LiveAction::Walkthrough { action } => match action {
                WalkthroughAction::Start => match app.start_walkthrough() {
                    Ok(()) => LiveResponse::State(app.live_state()),
                    Err(code) => LiveResponse::Error { code },
                },
                WalkthroughAction::Stop => match app.stop_walkthrough() {
                    Ok(()) => LiveResponse::State(app.live_state()),
                    Err(code) => LiveResponse::Error { code },
                },
                WalkthroughAction::Next => match app.move_walkthrough(true) {
                    Ok(()) => LiveResponse::State(app.live_state()),
                    Err(code) => LiveResponse::Error { code },
                },
                WalkthroughAction::Previous => match app.move_walkthrough(false) {
                    Ok(()) => LiveResponse::State(app.live_state()),
                    Err(code) => LiveResponse::Error { code },
                },
            },
            action => match app.apply_live_action(&action) {
                Ok(()) => LiveResponse::State(app.live_state()),
                Err(code) => LiveResponse::Error { code },
            },
        };
        let _ = request.response.send(response);
    }
    reload_requested
}

fn finish_requested_save<F, E>(app: &mut App<'_>, save: &mut F)
where
    F: FnMut(ReviewWrite<'_>) -> Result<(), E>,
    E: std::fmt::Display,
{
    if let Some(change) = app.viewed_change().cloned() {
        let result = save(ReviewWrite::Viewed(&change)).map_err(|error| error.to_string());
        app.finish_viewed(result);
    }
    if !app.save_requested() {
        return;
    }
    let result = app
        .review()
        .ok_or_else(|| "editable session lost its review state".to_owned())
        .and_then(|review| save(ReviewWrite::Revision(review)).map_err(|error| error.to_string()));
    app.finish_save(result);
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mire_core::{ChangesetSource, PatchLimits, ReviewRevision, parse_patch};
    use ratatui::backend::{ClearType, TestBackend, WindowSize};
    use ratatui::buffer::Cell;
    use ratatui::layout::{Position, Size};

    use super::*;

    const PATCH: &[u8] = b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";

    /// A [`TestBackend`] with the I/O error type the session loops expect.
    struct IoBackend(TestBackend);

    impl Backend for IoBackend {
        type Error = io::Error;

        fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
        where
            I: Iterator<Item = (u16, u16, &'a Cell)>,
        {
            self.0.draw(content).map_err(|never| match never {})
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.0.hide_cursor().map_err(|never| match never {})
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.0.show_cursor().map_err(|never| match never {})
        }

        fn get_cursor_position(&mut self) -> io::Result<Position> {
            self.0.get_cursor_position().map_err(|never| match never {})
        }

        fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
            self.0.set_cursor_position(position).map_err(|never| match never {})
        }

        fn clear(&mut self) -> io::Result<()> {
            self.0.clear().map_err(|never| match never {})
        }

        fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
            self.0.clear_region(clear_type).map_err(|never| match never {})
        }

        fn size(&self) -> io::Result<Size> {
            self.0.size().map_err(|never| match never {})
        }

        fn window_size(&mut self) -> io::Result<WindowSize> {
            self.0.window_size().map_err(|never| match never {})
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush().map_err(|never| match never {})
        }
    }

    #[test]
    fn quit_ends_a_watch_session_when_a_reload_lands_in_the_same_pass() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let mut terminal = Terminal::new(IoBackend(TestBackend::new(80, 24))).unwrap();
        let mut events = quit_then_idle();
        let mut reloads = 0;
        let mut reload = |_| {
            reloads += 1;
            assert_eq!(reloads, 1, "the reload rebuilt the app and dropped the quit key");
            WatchUpdate::Loaded(changeset.clone())
        };

        run_watch_loop(
            &mut terminal,
            changeset.clone(),
            AppOptions::default(),
            &Theme::plain(),
            &mut events,
            &mut reload,
            None,
        )
        .unwrap();

        assert_eq!(reloads, 0);
    }

    #[test]
    fn quit_ends_a_review_watch_session_when_a_reload_lands_in_the_same_pass() {
        let changeset = parse_patch(PATCH, ChangesetSource::Patch { label: None }, PatchLimits::default()).unwrap();
        let review = Review::new(ReviewRevision::new(1).unwrap(), changeset, Vec::new(), Vec::new()).unwrap();
        let mut terminal = Terminal::new(IoBackend(TestBackend::new(80, 24))).unwrap();
        let mut events = quit_then_idle();
        let mut save = |_: ReviewWrite<'_>| Ok::<(), String>(());
        let mut reloads = 0;
        let mut reload = |_| {
            reloads += 1;
            assert_eq!(reloads, 1, "the reload rebuilt the app and dropped the quit key");
            WatchUpdate::Loaded(review.clone())
        };

        run_review_watch_loop(
            &mut terminal,
            review.clone(),
            AppOptions::default(),
            &Theme::plain(),
            &mut events,
            &mut save,
            &mut reload,
            None,
        )
        .unwrap();

        assert_eq!(reloads, 0);
    }

    /// Yields the quit key once, then no further events.
    fn quit_then_idle() -> impl FnMut() -> io::Result<Option<Event>> {
        let mut quit = Some(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
        move || Ok(quit.take())
    }
}
