use std::io::{self, Stdout, stdout};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use mire_core::{Changeset, Review};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::{App, AppOptions, Theme, WatchUpdate, render};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut output = stdout();
        if let Err(error) = execute!(output, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(output)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let mut output = stdout();
                let _ = execute!(output, DisableMouseCapture, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn run(changeset: &Changeset, options: AppOptions) -> io::Result<()> {
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_loop(&mut session.terminal, changeset, options, &theme)
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_review<F, E>(review: &Review, options: AppOptions, mut save: F) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_review_loop(&mut session.terminal, review, options, &theme, &mut save)
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_watch<F>(changeset: Changeset, options: AppOptions, mut reload: F) -> io::Result<()>
where
    F: FnMut() -> WatchUpdate<Changeset>,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_watch_loop(&mut session.terminal, changeset, options, &theme, &mut reload)
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn run_review_watch<F, E, R>(review: Review, options: AppOptions, mut save: F, mut reload: R) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut() -> WatchUpdate<Review>,
{
    let theme = Theme::detect(options.theme);
    let mut session = TerminalSession::enter()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_review_watch_loop(&mut session.terminal, review, options, &theme, &mut save, &mut reload)
    }));
    drop(session);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, changeset: &Changeset, options: AppOptions, theme: &Theme,
) -> io::Result<()> {
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::ready_with_options(changeset, options);
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            Event::Resize(width, height) => app.resize(width, height),
            Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
        }
    }
    Ok(())
}

fn run_watch_loop<F>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut changeset: Changeset, options: AppOptions, theme: &Theme,
    reload: &mut F,
) -> io::Result<()>
where
    F: FnMut() -> WatchUpdate<Changeset>,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::ready_with_options(&changeset, options.clone());
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    let mut position = app.position();
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        handle_terminal_event(&mut app)?;
        match reload() {
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
            }
            WatchUpdate::Failed(error) => {
                if matches!(app.state(), crate::AppState::Ready(_)) {
                    position = app.position();
                }
                drop(app);
                app = App::error_with_options(error, options.clone());
                let size = terminal.size()?;
                app.resize(size.width, size.height);
            }
            WatchUpdate::Fatal(error) => return Err(io::Error::other(error)),
        }
    }
    Ok(())
}

fn run_review_loop<F, E>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, review: &Review, options: AppOptions, theme: &Theme,
    save: &mut F,
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
        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            Event::Resize(width, height) => app.resize(width, height),
            Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
        }
        if app.save_requested() {
            let result = app
                .review()
                .ok_or_else(|| "editable session lost its review state".to_owned())
                .and_then(|review| save(review).map_err(|error| error.to_string()));
            app.finish_save(result);
        }
    }
    Ok(())
}

fn run_review_watch_loop<F, E, R>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut review: Review, options: AppOptions, theme: &Theme,
    save: &mut F, reload: &mut R,
) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut() -> WatchUpdate<Review>,
{
    terminal.draw(|frame| render(frame, &App::loading(), theme))?;
    let mut app = App::review_with_options(&review, options.clone());
    let size = terminal.size()?;
    app.resize(size.width, size.height);
    let mut position = app.position();
    let mut pending = WatchUpdate::Unchanged;
    while !app.should_quit() {
        terminal.draw(|frame| render(frame, &app, theme))?;
        handle_terminal_event(&mut app)?;
        finish_requested_save(&mut app, save);
        if matches!(pending, WatchUpdate::Unchanged) {
            pending = reload();
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
            }
            WatchUpdate::Failed(error) => {
                if matches!(app.state(), crate::AppState::Ready(_)) {
                    position = app.position();
                }
                drop(app);
                app = App::error_with_options(error, options.clone());
                let size = terminal.size()?;
                app.resize(size.width, size.height);
            }
            WatchUpdate::Fatal(error) => return Err(io::Error::other(error)),
        }
    }
    Ok(())
}

fn handle_terminal_event(app: &mut App<'_>) -> io::Result<()> {
    if !event::poll(EVENT_POLL_INTERVAL)? {
        return Ok(());
    }
    match event::read()? {
        Event::Key(key) => app.handle_key(key),
        Event::Mouse(mouse) => app.handle_mouse(mouse),
        Event::Resize(width, height) => app.resize(width, height),
        Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
    }
    Ok(())
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
