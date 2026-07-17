use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use mire_core::{Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, PatchError, PatchLimits, parse_patch};
use thiserror::Error;

use crate::git::{self, DiffRequest, GitError, ShowRequest};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Review worktree or revision differences from Git.
    Diff(DiffArgs),
    /// Normalize patch input for inspection.
    Patch(PatchArgs),
    /// Review one Git commit.
    Show(ShowArgs),
}

#[derive(Debug, Error)]
enum AppError {
    #[error("cannot read patch from {input:?}: {source}")]
    InputIo { input: OsString, source: io::Error },
    #[error("cannot parse patch: {0}")]
    Patch(PatchError),
    #[error("cannot load Git changeset: {0}")]
    Git(GitError),
    #[error("cannot write changeset JSON: {0}")]
    Output(serde_json::Error),
    #[error("cannot finish changeset JSON output: {0}")]
    OutputNewline(io::Error),
}

impl From<&AppError> for u8 {
    fn from(value: &AppError) -> Self {
        value.exit_code()
    }
}

impl AppError {
    const fn exit_code(&self) -> u8 {
        match self {
            Self::InputIo { .. } => 3,
            Self::Patch(_) => 4,
            Self::Output(_) | Self::OutputNewline(_) => 5,
            Self::Git(_) => 6,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "mire", version, about = "Review changesets without modifying them")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Debug)]
struct DiffArgs {
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH")]
    paths: Vec<OsString>,
    /// Structured output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct PatchArgs {
    /// Patch file to read, or - for standard input.
    input: OsString,
    /// Structured output format.
    #[arg(long, value_enum, required = true)]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct ShowArgs {
    /// Commit to show; defaults to HEAD.
    revision: Option<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH")]
    paths: Vec<OsString>,
    /// Structured output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
}

pub fn run(args: impl Iterator<Item = OsString>) -> ExitCode {
    let cli = match Cli::try_parse_from(std::iter::once(OsString::from("mire")).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = if error.use_stderr() { 2 } else { 0 };
            let _ = error.print();
            return ExitCode::from(exit_code);
        }
    };
    match execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "mire: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn execute(cli: Cli) -> Result<(), AppError> {
    let changeset = match cli.command {
        Command::Diff(DiffArgs { staged, revisions, paths, format: OutputFormat::Json }) => {
            git::load_diff(DiffRequest { staged, revisions, paths }).map_err(AppError::Git)?
        }
        Command::Patch(PatchArgs { input, format: OutputFormat::Json }) => load_patch(&input)?,
        Command::Show(ShowArgs { revision, paths, format: OutputFormat::Json }) => {
            git::load_show(ShowRequest { revision, paths }).map_err(AppError::Git)?
        }
    };
    write_changeset(&changeset)
}

fn load_patch(input: &OsStr) -> Result<Changeset, AppError> {
    let bytes = read_input(input, DEFAULT_MAX_PATCH_BYTES)?;
    parse_patch(&bytes, ChangesetSource::Patch { label: None }, PatchLimits::default()).map_err(AppError::Patch)
}

fn write_changeset(changeset: &Changeset) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, changeset).map_err(AppError::Output)?;
    writeln!(output).map_err(AppError::OutputNewline)
}

fn read_input(input: &OsStr, limit: usize) -> Result<Vec<u8>, AppError> {
    if input == "-" {
        return read_bounded(io::stdin().lock(), limit)
            .map_err(|source| AppError::InputIo { input: input.to_owned(), source });
    }

    let path = Path::new(input);
    let file = File::open(path).map_err(|source| AppError::InputIo { input: input.to_owned(), source })?;
    if let Ok(metadata) = file.metadata() {
        if metadata.len() > limit as u64 {
            let actual = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
            return Err(AppError::Patch(PatchError::InputTooLarge { actual, limit }));
        }
    }
    read_bounded(file, limit).map_err(|source| AppError::InputIo { input: input.to_owned(), source })
}

fn read_bounded(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let capacity = limit.min(64 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    reader.take(limit.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_stops_one_byte_after_the_limit() {
        let bytes = read_bounded(&b"abcdef"[..], 3).unwrap();
        assert_eq!(bytes, b"abcd");
    }

    #[test]
    fn clap_accepts_stdin_and_the_json_format() {
        let cli = Cli::try_parse_from(["mire", "patch", "-", "--format", "json"]).unwrap();
        let Command::Patch(arguments) = cli.command else {
            panic!("patch command is parsed");
        };
        assert_eq!(arguments.input, "-");
        assert!(matches!(arguments.format, OutputFormat::Json));
    }

    #[test]
    fn clap_separates_revisions_and_paths() {
        let cli = Cli::try_parse_from(["mire", "diff", "main...HEAD", "--", "src/lib.rs"]).unwrap();
        let Command::Diff(arguments) = cli.command else {
            panic!("diff command is parsed");
        };
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src/lib.rs"]);
    }
}
