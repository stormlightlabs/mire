use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use mire_core::{ChangesetSource, DEFAULT_MAX_PATCH_BYTES, PatchError, PatchLimits, parse_patch};
use thiserror::Error;

const EXIT_INPUT_IO: u8 = 3;
const EXIT_INVALID_PATCH: u8 = 4;
const EXIT_OUTPUT_IO: u8 = 5;

pub(crate) fn run(args: impl Iterator<Item = OsString>) -> ExitCode {
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

#[derive(Debug, Parser)]
#[command(name = "mire", version, about = "Review changesets without modifying them")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Normalize patch input for inspection.
    Patch(PatchArgs),
}

#[derive(Args, Debug)]
struct PatchArgs {
    /// Patch file to read, or - for standard input.
    input: OsString,
    /// Structured output format.
    #[arg(long, value_enum, required = true)]
    format: OutputFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("cannot read patch from {input:?}: {source}")]
    InputIo { input: OsString, source: io::Error },
    #[error("cannot parse patch: {0}")]
    Patch(PatchError),
    #[error("cannot write changeset JSON: {0}")]
    Output(serde_json::Error),
    #[error("cannot finish changeset JSON output: {0}")]
    OutputNewline(io::Error),
}

impl AppError {
    const fn exit_code(&self) -> u8 {
        match self {
            Self::InputIo { .. } => EXIT_INPUT_IO,
            Self::Patch(_) => EXIT_INVALID_PATCH,
            Self::Output(_) | Self::OutputNewline(_) => EXIT_OUTPUT_IO,
        }
    }
}

fn execute(cli: Cli) -> Result<(), AppError> {
    let Command::Patch(PatchArgs { input, format: OutputFormat::Json }) = cli.command;
    let bytes = read_input(&input, DEFAULT_MAX_PATCH_BYTES)?;
    let changeset =
        parse_patch(&bytes, ChangesetSource::Patch { label: None }, PatchLimits::default()).map_err(AppError::Patch)?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, &changeset).map_err(AppError::Output)?;
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
        let Command::Patch(arguments) = cli.command;
        assert_eq!(arguments.input, "-");
        assert!(matches!(arguments.format, OutputFormat::Json));
    }
}
