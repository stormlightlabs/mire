//! Terminal application state, virtualized review rows, and rendering.

mod app;
mod stream;
mod terminal;
mod theme;
mod view;

use std::io;

use mire_core::Changeset;

pub use app::{App, AppState};
pub use stream::{ReviewStream, RowKind};
pub use theme::Theme;
pub use view::render;

/// Opens an interactive review and restores the terminal before returning.
pub fn run(changeset: &Changeset) -> io::Result<()> {
    terminal::run(changeset)
}
