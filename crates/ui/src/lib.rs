//! Terminal application state, virtualized review rows, and rendering.

mod app;
mod chrome;
mod layout;
mod live;
mod navigation;
mod note_filter;
mod notes;
mod stream;
mod syntax;
mod terminal;
mod theme;
mod view;

use std::io;

use mire_core::{Changeset, Review};

pub use app::{App, AppOptions, AppState};
pub use live::{
    LiveAction, LiveControl, LiveRequest, LiveResponse, PresentationKind, PresentationState, WalkthroughAction,
};
pub use notes::{EditorTarget, LineSelection, NoteEditor};
pub use stream::{LayoutMode, ReviewStream, RowKind};
pub use theme::{ColorMode, ParseThemeFamilyError, Theme, ThemeFamily, ThemeVariant};
pub use view::render;

/// A filesystem-backed review update produced by the CLI watch boundary.
#[derive(Debug)]
pub enum WatchUpdate<T> {
    /// No debounced reload is ready yet.
    Unchanged,
    /// New content was loaded successfully.
    Loaded(T),
    /// Reloading the bound source failed, but watching should continue.
    Failed(String),
    /// The durable review file became unreadable and the session must stop.
    Fatal(String),
}

/// Opens an interactive review and restores the terminal before returning.
pub fn run(changeset: &Changeset) -> io::Result<()> {
    terminal::run(changeset, AppOptions::default(), None)
}

/// Opens an interactive review with explicit presentation preferences.
pub fn run_with_options(changeset: &Changeset, options: AppOptions) -> io::Result<()> {
    terminal::run(changeset, options, None)
}

/// Opens an interactive review that accepts authenticated local control requests.
pub fn run_with_live_control(changeset: &Changeset, options: AppOptions, control: LiveControl) -> io::Result<()> {
    terminal::run(changeset, options, Some(control))
}

/// Opens an interactive changeset that can be replaced by debounced filesystem updates.
pub fn run_watch_with_options<F>(changeset: Changeset, options: AppOptions, mut reload: F) -> io::Result<()>
where
    F: FnMut() -> WatchUpdate<Changeset>,
{
    terminal::run_watch(changeset, options, move |_| reload(), None)
}

/// Opens a watched changeset that accepts authenticated local control requests.
pub fn run_watch_with_live_control<F>(
    changeset: Changeset, options: AppOptions, reload: F, control: LiveControl,
) -> io::Result<()>
where
    F: FnMut(bool) -> WatchUpdate<Changeset>,
{
    terminal::run_watch(changeset, options, reload, Some(control))
}

/// Opens an editable durable review and saves every accepted note action through the supplied boundary.
pub fn run_review_with_options<F, E>(review: &Review, options: AppOptions, save: F) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
{
    terminal::run_review(review, options, save, None)
}

/// Opens an editable review that accepts authenticated local control requests.
pub fn run_review_with_live_control<F, E>(
    review: &Review, options: AppOptions, save: F, control: LiveControl,
) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
{
    terminal::run_review(review, options, save, Some(control))
}

/// Opens an editable durable review that can be replaced by debounced filesystem updates.
pub fn run_review_watch_with_options<F, E, R>(
    review: Review, options: AppOptions, save: F, mut reload: R,
) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut() -> WatchUpdate<Review>,
{
    terminal::run_review_watch(review, options, save, move |_| reload(), None)
}

/// Opens a watched editable review that accepts authenticated local control requests.
pub fn run_review_watch_with_live_control<F, E, R>(
    review: Review, options: AppOptions, save: F, reload: R, control: LiveControl,
) -> io::Result<()>
where
    F: FnMut(&Review) -> Result<(), E>,
    E: std::fmt::Display,
    R: FnMut(bool) -> WatchUpdate<Review>,
{
    terminal::run_review_watch(review, options, save, reload, Some(control))
}
