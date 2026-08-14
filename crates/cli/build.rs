#[path = "src/cli.rs"]
mod cli;

use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use clap::CommandFactory;
use clap_complete::{Shell, generate_to};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=src/cli.rs");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("Cargo did not set CARGO_MANIFEST_DIR"))?,
    );
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::other("CLI package must be in the workspace crates directory"))?;
    let assets_dir = workspace_root.join("assets");
    let man_dir = assets_dir.join("man");
    let completion_dir = assets_dir.join("comp");

    fs::create_dir_all(&man_dir)?;
    fs::create_dir_all(&completion_dir)?;

    let mut man_file = File::create(man_dir.join("mire.1"))?;
    clap_mangen::Man::new(cli::Cli::command()).render(&mut man_file)?;

    for shell in [Shell::Bash, Shell::Elvish, Shell::Fish, Shell::PowerShell, Shell::Zsh] {
        generate_to(shell, &mut cli::Cli::command(), "mire", &completion_dir)?;
    }

    Ok(())
}
