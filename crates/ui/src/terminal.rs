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
use ratatui::backend::CrosstermBackend;

use crate::{App, AppOptions, LiveAction, LiveControl, LiveResponse, Theme, WalkthroughAction, WatchUpdate, render};

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
    F: FnMut(&Review) -> Result<(), E>,
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
    F: FnMut(&Review) -> Result<(), E>,
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
        let _ = handle_terminal_event(&mut app, control, false)?;
    }
    Ok(())
}

fn run_watch_loop<F>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut changeset: Changeset, options: AppOptions, theme: &Theme,
    reload: &mut F, control: Option<&LiveControl>,
) -> io::Result<()>
where
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
        let requested_reload = handle_terminal_event(&mut app, control, true)?;
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
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::review_with_options(review, options);
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        let _ = handle_terminal_event(&mut app, control, false)?;
        finish_requested_save(&mut app, save);
    }
    Ok(())
}

fn run_review_watch_loop<F, E, R>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut review: Review, options: AppOptions, theme: &Theme,
    save: &mut F, reload: &mut R, control: Option<&LiveControl>,
) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
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
        let requested_reload = handle_terminal_event(&mut app, control, true)?;
        finish_requested_save(&mut app, save);
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

fn handle_terminal_event(app: &mut App<'_>, control: Option<&LiveControl>, reload_available: bool) -> io::Result<bool> {
    if event::poll(EVENT_POLL_INTERVAL)? {
        match event::read()? {
            Event::Key(key) => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            Event::Resize(width, height) => app.resize(width, height),
            Event::Paste(text) => app.handle_paste(&text),
            Event::FocusGained | Event::FocusLost => {}
        }
    }
    Ok(handle_live_requests(app, control, reload_available))
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
                WalkthroughAction::Stop => {
                    app.stop_walkthrough();
                    LiveResponse::State(app.live_state())
                }
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
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
{
    if !app.save_requested() {
        return;
    }
    let result = app
        .review()
        .ok_or_else(|| "editable session lost its review state".to_owned())
        .and_then(|review| save(review).map_err(|error| error.to_string()));
    app.finish_save(result);
}
