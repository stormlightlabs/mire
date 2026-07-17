use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use mire_core::{
    Author, Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, PatchError, PatchLimits, Review, parse_patch,
};
use mire_tui::ThemeFamily;
use thiserror::Error;

use crate::git::{self, DiffRequest, GitError, ShowRequest};
use crate::protocol::{
    ContextSelection, NoteBatch, ProtocolError, context_json, import_error_json, import_result_json, notes_json,
    notes_markdown,
};
use crate::review_file::{DEFAULT_MAX_REVIEW_FILE_BYTES, ReviewFileError, read_review, write_review_atomic};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ThemeArgument {
    #[default]
    Auto,
    Iceberg,
    Eldritch,
    Catppuccin,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export bounded context from a durable review.
    Context(ContextArgs),
    /// Review worktree or revision differences from Git.
    Diff(DiffArgs),
    /// Normalize patch input for inspection.
    Patch(PatchArgs),
    /// Import, list, or export durable review notes.
    Notes(NotesArgs),
    /// Open a durable review file.
    Review(ReviewArgs),
    /// Review one Git commit.
    Show(ShowArgs),
}

#[derive(Debug, Subcommand)]
enum NotesCommand {
    /// Atomically append a schema-versioned note batch.
    Import(NoteImportArgs),
    /// List notes as deterministic JSON.
    List(NoteListArgs),
    /// Export notes as JSON or standalone Markdown.
    Export(NoteExportArgs),
}

#[derive(Debug, Error)]
enum AppError {
    #[error("cannot read patch from {input:?}: {source}")]
    InputIo { input: OsString, source: io::Error },
    #[error("cannot parse patch: {0}")]
    Patch(PatchError),
    #[error("cannot load Git changeset: {0}")]
    Git(GitError),
    #[error("cannot load review: {0}")]
    ReviewFile(ReviewFileError),
    #[error("cannot read protocol input from {input:?}: {source}")]
    ProtocolInputIo { input: OsString, source: io::Error },
    #[error("protocol input from {input:?} exceeds the {limit}-byte limit")]
    ProtocolInputTooLarge { input: OsString, limit: usize },
    #[error("invalid note batch JSON: {0}")]
    ProtocolJson(serde_json::Error),
    #[error("review protocol failed: {0}")]
    Protocol(ProtocolError),
    #[error("note import was rejected")]
    ImportRejected { report: Vec<u8> },
    #[error("cannot write JSON output: {0}")]
    Output(serde_json::Error),
    #[error("cannot write command output: {0}")]
    OutputIo(io::Error),
    #[error("terminal interface failed: {0}")]
    Terminal(io::Error),
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
            Self::Output(_) | Self::OutputIo(_) | Self::Terminal(_) => 5,
            Self::Git(_) => 6,
            Self::ReviewFile(_) => 7,
            Self::ProtocolInputIo { .. }
            | Self::ProtocolInputTooLarge { .. }
            | Self::ProtocolJson(_)
            | Self::Protocol(_)
            | Self::ImportRejected { .. } => 8,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "mire", version, about = "a terminal difftool")]
struct Cli {
    /// Viewer & highlighter theme.
    #[arg(long, global = true, value_enum, default_value_t)]
    theme: ThemeArgument,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Debug)]
struct ContextArgs {
    /// JSON review file to inspect.
    review: OsString,
    /// Include the complete normalized patch capture.
    #[arg(long, conflicts_with = "file", requires = "max_bytes")]
    patch: bool,
    /// Include one complete normalized file diff.
    #[arg(long, value_name = "PATH", conflicts_with = "patch", requires = "max_bytes")]
    file: Option<OsString>,
    /// Maximum serialized bytes for an expanded context request.
    #[arg(long, value_name = "BYTES")]
    max_bytes: Option<usize>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, default_value = "json")]
    format: Option<OutputFormat>,
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
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    language: Option<String>,
}

#[derive(Args, Debug)]
struct PatchArgs {
    /// Patch file to read, or - for standard input.
    input: OsString,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    language: Option<String>,
}

#[derive(Args, Debug)]
struct NotesArgs {
    #[command(subcommand)]
    command: NotesCommand,
}

#[derive(Args, Debug)]
struct NoteImportArgs {
    /// JSON review file to update atomically.
    review: OsString,
    /// Note batch JSON file, or - for standard input.
    input: OsString,
}

#[derive(Args, Debug)]
struct NoteListArgs {
    /// JSON review file to inspect.
    review: OsString,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, default_value = "json")]
    format: Option<OutputFormat>,
}

#[derive(Args, Debug)]
struct NoteExportArgs {
    /// JSON review file to export.
    review: OsString,
    /// Export format.
    #[arg(long, value_enum, default_value = "json")]
    format: OutputFormat,
}

#[derive(Args, Debug)]
struct ReviewArgs {
    /// JSON review file to open.
    input: OsString,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    format: Option<OutputFormat>,
}

#[derive(Args, Debug)]
struct ShowArgs {
    /// Commit to show; defaults to HEAD.
    revision: Option<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH")]
    paths: Vec<OsString>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    language: Option<String>,
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
            if let AppError::ImportRejected { report } = &error {
                let _ = io::stderr().lock().write_all(report);
            } else {
                let _ = writeln!(io::stderr().lock(), "mire: {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}

fn execute(cli: Cli) -> Result<(), AppError> {
    let theme = ThemeFamily::from(cli.theme);
    let (changeset, format, language) = match cli.command {
        Command::Context(ContextArgs { review, patch, file, max_bytes, format }) => {
            let _ = format;
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            let selection = if patch {
                ContextSelection::Patch
            } else if let Some(path) = file.as_deref() {
                ContextSelection::File(path)
            } else {
                ContextSelection::Manifest
            };
            return write_bytes(&context_json(&review, selection, max_bytes).map_err(AppError::Protocol)?);
        }
        Command::Diff(DiffArgs { staged, revisions, paths, format, language }) => (
            git::load_diff(DiffRequest { staged, revisions, paths }).map_err(AppError::Git)?,
            format,
            language,
        ),
        Command::Patch(PatchArgs { input, format, language }) => (load_patch(&input)?, format, language),
        Command::Notes(NotesArgs { command }) => return execute_notes(command),
        Command::Review(ReviewArgs { input, format }) => {
            let review_path = Path::new(&input).to_owned();
            let review = read_review(&review_path).map_err(AppError::ReviewFile)?;
            return if format.is_some() || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                write_review(&review)
            } else {
                mire_tui::run_review_with_options(
                    &review,
                    mire_tui::AppOptions { language_override: None, theme, human_author: Some(local_author()) },
                    |updated| write_review_atomic(&review_path, updated),
                )
                .map_err(AppError::Terminal)
            };
        }
        Command::Show(ShowArgs { revision, paths, format, language }) => (
            git::load_show(ShowRequest { revision, paths }).map_err(AppError::Git)?,
            format,
            language,
        ),
    };
    if format.is_some() || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        write_changeset(&changeset)
    } else {
        mire_tui::run_with_options(
            &changeset,
            mire_tui::AppOptions { language_override: language, theme, human_author: None },
        )
        .map_err(AppError::Terminal)
    }
}

fn local_author() -> Author {
    let identifier = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .unwrap_or_else(|| "local-human".to_owned());
    Author::new(identifier, None).expect("environment author was bounded or replaced with a static fallback")
}

impl From<ThemeArgument> for ThemeFamily {
    fn from(value: ThemeArgument) -> Self {
        match value {
            ThemeArgument::Auto => Self::Auto,
            ThemeArgument::Iceberg => Self::Iceberg,
            ThemeArgument::Eldritch => Self::Eldritch,
            ThemeArgument::Catppuccin => Self::Catppuccin,
        }
    }
}

fn write_review(review: &Review) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, review).map_err(AppError::Output)?;
    writeln!(output).map_err(AppError::OutputIo)
}

fn execute_notes(command: NotesCommand) -> Result<(), AppError> {
    match command {
        NotesCommand::Import(NoteImportArgs { review, input }) => import_notes(&review, &input),
        NotesCommand::List(NoteListArgs { review, format }) => {
            let _ = format;
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            write_bytes(&notes_json(&review).map_err(AppError::Protocol)?)
        }
        NotesCommand::Export(NoteExportArgs { review, format }) => {
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            let bytes = match format {
                OutputFormat::Json => notes_json(&review).map_err(AppError::Protocol)?,
                OutputFormat::Markdown => notes_markdown(&review),
            };
            write_bytes(&bytes)
        }
    }
}

fn import_notes(review_path: &OsStr, input: &OsStr) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let bytes = read_protocol_input(input, DEFAULT_MAX_REVIEW_FILE_BYTES)?;
    let batch: NoteBatch = serde_json::from_slice(&bytes).map_err(AppError::ProtocolJson)?;
    let notes = batch.into_notes().map_err(AppError::Protocol)?;
    let imported = notes.len();
    let updated = match review.import_notes(notes) {
        Ok(updated) => updated,
        Err(error) => {
            let report = import_error_json(&error).map_err(AppError::Protocol)?;
            return Err(AppError::ImportRejected { report });
        }
    };
    write_review_atomic(Path::new(review_path), &updated).map_err(AppError::ReviewFile)?;
    write_bytes(&import_result_json(&updated, imported).map_err(AppError::Protocol)?)
}

fn parse_json_format(value: &str) -> Result<OutputFormat, String> {
    if value == "json" {
        Ok(OutputFormat::Json)
    } else {
        Err(format!("unsupported format {value:?}; possible values: json"))
    }
}

fn parse_language(value: &str) -> Result<String, String> {
    const LANGUAGES: &[&str] = &[
        "bash",
        "sh",
        "css",
        "html",
        "javascript",
        "js",
        "json",
        "markdown",
        "md",
        "plain",
        "plaintext",
        "python",
        "py",
        "rust",
        "rs",
        "toml",
        "tsx",
        "typescript",
        "ts",
        "yaml",
        "yml",
    ];
    if LANGUAGES.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(format!(
            "unsupported language {value:?}; supported values: {}",
            LANGUAGES.join(", ")
        ))
    }
}

fn load_patch(input: &OsStr) -> Result<Changeset, AppError> {
    let bytes = read_input(input, DEFAULT_MAX_PATCH_BYTES)?;
    parse_patch(&bytes, ChangesetSource::Patch { label: None }, PatchLimits::default()).map_err(AppError::Patch)
}

fn write_changeset(changeset: &Changeset) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, changeset).map_err(AppError::Output)?;
    writeln!(output).map_err(AppError::OutputIo)
}

fn write_bytes(bytes: &[u8]) -> Result<(), AppError> {
    io::stdout().lock().write_all(bytes).map_err(AppError::OutputIo)
}

fn read_protocol_input(input: &OsStr, limit: usize) -> Result<Vec<u8>, AppError> {
    let bytes = if input == "-" {
        read_bounded(io::stdin().lock(), limit)
            .map_err(|source| AppError::ProtocolInputIo { input: input.to_owned(), source })?
    } else {
        let file = File::open(Path::new(input))
            .map_err(|source| AppError::ProtocolInputIo { input: input.to_owned(), source })?;
        read_bounded(file, limit).map_err(|source| AppError::ProtocolInputIo { input: input.to_owned(), source })?
    };
    if bytes.len() > limit {
        return Err(AppError::ProtocolInputTooLarge { input: input.to_owned(), limit });
    }
    Ok(bytes)
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
        assert!(matches!(arguments.format, Some(OutputFormat::Json)));
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

    #[test]
    fn clap_accepts_supported_language_overrides_and_rejects_unknown_ones() {
        let cli = Cli::try_parse_from(["mire", "patch", "-", "--language", "typescript"]).unwrap();
        let Command::Patch(arguments) = cli.command else {
            panic!("patch command is parsed");
        };
        assert_eq!(arguments.language.as_deref(), Some("typescript"));

        let error = Cli::try_parse_from(["mire", "patch", "-", "--language", "brainfuck"]).unwrap_err();
        assert!(error.to_string().contains("unsupported language"));
    }

    #[test]
    fn clap_accepts_every_theme_before_or_after_interactive_subcommands() {
        for theme in ["auto", "iceberg", "eldritch", "catppuccin"] {
            let before = Cli::try_parse_from(["mire", "--theme", theme, "patch", "-"]).unwrap();
            assert_eq!(ThemeFamily::from(before.theme).as_str(), theme);

            let after = Cli::try_parse_from(["mire", "patch", "-", "--theme", theme]).unwrap();
            assert_eq!(ThemeFamily::from(after.theme).as_str(), theme);
        }
    }

    #[test]
    fn clap_defaults_to_auto_and_lists_allowed_themes_for_invalid_input() {
        let cli = Cli::try_parse_from(["mire", "patch", "-"]).unwrap();
        assert!(matches!(cli.theme, ThemeArgument::Auto));

        let error = Cli::try_parse_from(["mire", "patch", "-", "--theme", "dracula"]).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("invalid value 'dracula'"));
        assert!(message.contains("possible values: auto, iceberg, eldritch, catppuccin"));
    }
}
