//! Terminal application state, virtualized review rows, and rendering.

mod app;
mod chrome;
mod layout;
mod navigation;
mod stream;
mod syntax;
mod terminal;
mod theme;
mod view;

use std::io;

use mire_core::Changeset;

pub use app::{App, AppOptions, AppState};
pub use stream::{LayoutMode, ReviewStream, RowKind};
pub use theme::{ColorMode, ParseThemeFamilyError, Theme, ThemeFamily, ThemeVariant};
pub use view::render;

/// Opens an interactive review and restores the terminal before returning.
pub fn run(changeset: &Changeset) -> io::Result<()> {
    terminal::run(changeset, AppOptions::default())
}

/// Opens an interactive review with explicit presentation preferences.
pub fn run_with_options(changeset: &Changeset, options: AppOptions) -> io::Result<()> {
    terminal::run(changeset, options)
}
