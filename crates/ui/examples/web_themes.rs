//! Prints the browser review's theme stylesheet, built from the terminal palettes.
//!
//! `just web-themes` writes this output to `packages/app/src/themes.css`.

fn main() {
    print!("{}", mire_tui::web_stylesheet());
}
