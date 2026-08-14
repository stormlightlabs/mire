use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use mire_core::{
    AnchorSide, AnnotationKind, Author, BytePath, Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, Fingerprint,
    LineNumber, LineRange, NoteId, NoteInput, NoteSeverity, NoteStatus, PatchError, PatchLimits, Provenance, Review,
    ReviewError, ReviewRevision, parse_patch,
};
use mire_tui::ThemeFamily;
use thiserror::Error;

use crate::git::{self, DiffRequest, GitError, ShowRequest};
use crate::protocol::{
    ContextSelection, LocationBatch, NoteBatch, ProtocolError, apply_error_json, context_json, import_error_json,
    import_result_json, notes_json, notes_markdown, protocol_error_json,
};
use crate::review_file::{
    DEFAULT_MAX_REVIEW_FILE_BYTES, ReviewFileError, create_review_atomic, read_review, write_review_atomic_if_revision,
};
use crate::watch::{WatchError, WatchSet};

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
    /// Add or disposition one durable review note.
    Note(NoteArgs),
    /// Apply, import, list, or export durable review notes.
    Notes(NotesArgs),
    /// Open a durable review file.
    Review(ReviewArgs),
    /// Review one Git commit.
    Show(ShowArgs),
    /// Watch a Git worktree or revision comparison for changes.
    Watch(WatchArgs),
}

#[derive(Debug, Subcommand)]
enum NoteCommand {
    /// Add one finding from a source location.
    Add(NoteAddArgs),
    /// Mark one finding as resolved.
    Resolve(NoteDispositionArgs),
    /// Dismiss one finding.
    Dismiss(NoteDispositionArgs),
    /// Accept the remaining risk from one finding.
    AcceptRisk(NoteDispositionArgs),
}

#[derive(Debug, Subcommand)]
enum NotesCommand {
    /// Atomically apply a schema-versioned location batch from standard input.
    Apply(NoteApplyArgs),
    /// Atomically append a schema-versioned full-note batch.
    Import(NoteImportArgs),
    /// List notes as deterministic JSON.
    List(NoteListArgs),
    /// Export notes as JSON or standalone Markdown.
    Export(NoteExportArgs),
}

#[derive(Debug, Subcommand)]
enum ReviewCommand {
    /// Create a durable review from a Git comparison.
    Init(ReviewInitArgs),
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
    #[error("cannot create review: {0}")]
    CreateReviewFile(ReviewFileError),
    #[error("cannot initialize review: {0}")]
    InitializeReview(ReviewError),
    #[error("cannot read protocol input from {input:?}: {source}")]
    ProtocolInputIo { input: OsString, source: io::Error },
    #[error("protocol input from {input:?} exceeds the {limit}-byte limit")]
    ProtocolInputTooLarge { input: OsString, limit: usize },
    #[error("invalid note batch JSON: {0}")]
    ProtocolJson(serde_json::Error),
    #[error("review protocol failed: {0}")]
    Protocol(ProtocolError),
    #[error("note mutation was rejected")]
    MutationRejected { report: Vec<u8> },
    #[error("cannot write JSON output: {0}")]
    Output(serde_json::Error),
    #[error("cannot write command output: {0}")]
    OutputIo(io::Error),
    #[error("terminal interface failed: {0}")]
    Terminal(io::Error),
    #[error("watch mode requires an interactive terminal and cannot be combined with structured output")]
    WatchRequiresTerminal,
    #[error("watch mode cannot read a patch from standard input")]
    WatchStdin,
    #[error("cannot start watch mode: {0}")]
    Watch(WatchError),
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
            Self::Output(_)
            | Self::OutputIo(_)
            | Self::Terminal(_)
            | Self::WatchRequiresTerminal
            | Self::WatchStdin
            | Self::Watch(_) => 5,
            Self::Git(_) => 6,
            Self::ReviewFile(_) | Self::CreateReviewFile(_) | Self::InitializeReview(_) => 7,
            Self::ProtocolInputIo { .. }
            | Self::ProtocolInputTooLarge { .. }
            | Self::ProtocolJson(_)
            | Self::Protocol(_)
            | Self::MutationRejected { .. } => 8,
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
    #[arg(long, value_name = "PATH", conflicts_with_all = ["patch", "hunk"], requires = "max_bytes")]
    file: Option<OsString>,
    /// Include one hunk selected from the manifest.
    #[arg(
        long,
        value_name = "FINGERPRINT",
        value_parser = parse_fingerprint,
        conflicts_with_all = ["patch", "file"],
        requires = "max_bytes"
    )]
    hunk: Option<Fingerprint>,
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
    /// Reload the interactive review when the repository changes.
    #[arg(long)]
    watch: bool,
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
    /// Reload the interactive review when the patch file changes.
    #[arg(long)]
    watch: bool,
}

#[derive(Args, Debug)]
struct NoteArgs {
    #[command(subcommand)]
    command: NoteCommand,
}

#[derive(Args, Debug)]
struct NotesArgs {
    #[command(subcommand)]
    command: NotesCommand,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProvenanceArgument {
    Agent,
    Analyzer,
    Interchange,
}

#[derive(Args, Debug)]
struct NoteAddArgs {
    review: OsString,
    #[arg(long)]
    revision: u64,
    #[arg(long)]
    file: OsString,
    #[arg(long, conflicts_with = "new_line", required_unless_present = "new_line")]
    old_line: Option<u64>,
    #[arg(long, conflicts_with = "old_line", required_unless_present = "old_line")]
    new_line: Option<u64>,
    #[arg(long)]
    end_line: Option<u64>,
    #[arg(long)]
    author: String,
    #[arg(long)]
    author_name: Option<String>,
    #[arg(long, value_enum)]
    provenance: ProvenanceArgument,
    #[arg(long)]
    producer: String,
    #[arg(long, value_parser = parse_severity)]
    severity: NoteSeverity,
    #[arg(long = "kind", value_parser = parse_annotation_kind)]
    annotation_kind: AnnotationKind,
    #[arg(long)]
    body: String,
}

#[derive(Args, Debug)]
struct NoteDispositionArgs {
    review: OsString,
    note_id: String,
    #[arg(long)]
    revision: u64,
    #[arg(long)]
    author: String,
    #[arg(long)]
    author_name: Option<String>,
}

#[derive(Args, Debug)]
struct NoteApplyArgs {
    review: OsString,
    /// Read the location batch from standard input.
    #[arg(long, required = true)]
    stdin: bool,
}

#[derive(Args, Debug)]
struct NoteImportArgs {
    /// JSON review file to update atomically.
    review: OsString,
    /// Note batch JSON file, or - for standard input.
    input: OsString,
    /// Review revision observed before constructing the batch.
    #[arg(long)]
    revision: u64,
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
#[command(
    arg_required_else_help = true,
    after_help = "Create and open a review:\n  mire review init review.json main...HEAD -- src\n  mire review review.json --watch"
)]
struct ReviewArgs {
    /// JSON review file to open.
    input: Option<OsString>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, requires = "input")]
    format: Option<OutputFormat>,
    /// Reload the interactive review when the review file changes.
    #[arg(long, requires = "input")]
    watch: bool,
    #[command(subcommand)]
    command: Option<ReviewCommand>,
}

#[derive(Args, Debug)]
struct ReviewInitArgs {
    /// New JSON review file to create.
    review: OsString,
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH")]
    paths: Vec<OsString>,
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
    /// Reload the interactive review when the repository changes.
    #[arg(long)]
    watch: bool,
}

#[derive(Args, Debug)]
struct WatchArgs {
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH")]
    paths: Vec<OsString>,
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
            if let AppError::MutationRejected { report } = &error {
                let _ = io::stderr().lock().write_all(report);
            } else if let AppError::Protocol(protocol_error) = &error {
                match protocol_error_json(protocol_error) {
                    Ok(report) => {
                        let _ = io::stderr().lock().write_all(&report);
                    }
                    Err(_) => {
                        let _ = writeln!(io::stderr().lock(), "mire: {error}");
                    }
                }
            } else {
                let _ = writeln!(io::stderr().lock(), "mire: {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}

fn execute(cli: Cli) -> Result<(), AppError> {
    let theme = ThemeFamily::from(cli.theme);
    match cli.command {
        Command::Context(ContextArgs { review, patch, file, hunk, max_bytes, format }) => {
            let _ = format;
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            let selection = if patch {
                ContextSelection::Patch
            } else if let Some(path) = file.as_deref() {
                ContextSelection::File(path)
            } else if let Some(fingerprint) = hunk {
                ContextSelection::Hunk(fingerprint)
            } else {
                ContextSelection::Manifest
            };
            write_bytes(&context_json(&review, selection, max_bytes).map_err(AppError::Protocol)?)
        }
        Command::Diff(DiffArgs { staged, revisions, paths, format, language, watch }) => {
            let request = DiffRequest { staged, revisions, paths };
            let changeset = git::load_diff(request.clone()).map_err(AppError::Git)?;
            let source = if watch {
                Some(ReloadSource::GitDiff { root: git::repository_root().map_err(AppError::Git)?, request })
            } else {
                None
            };
            run_changeset(changeset, format, language, theme, source)
        }
        Command::Patch(PatchArgs { input, format, language, watch }) => {
            if watch && input == "-" {
                return Err(AppError::WatchStdin);
            }
            let changeset = load_patch(&input)?;
            let source =
                watch.then(|| ReloadSource::Patch { watch_path: watched_file_parent(Path::new(&input)), input });
            run_changeset(changeset, format, language, theme, source)
        }
        Command::Note(NoteArgs { command }) => execute_note(command),
        Command::Notes(NotesArgs { command }) => execute_notes(command),
        Command::Review(ReviewArgs { input, format, watch, command }) => match command {
            Some(ReviewCommand::Init(arguments)) => initialize_review(arguments),
            None => open_review(
                input.expect("clap requires a review path when no review subcommand is present"),
                format,
                watch,
                theme,
            ),
        },
        Command::Show(ShowArgs { revision, paths, format, language, watch }) => {
            let request = ShowRequest { revision, paths };
            let changeset = git::load_show(request.clone()).map_err(AppError::Git)?;
            let source = if watch {
                Some(ReloadSource::GitShow { root: git::repository_root().map_err(AppError::Git)?, request })
            } else {
                None
            };
            run_changeset(changeset, format, language, theme, source)
        }
        Command::Watch(WatchArgs { staged, revisions, paths, language }) => {
            let request = DiffRequest { staged, revisions, paths };
            let changeset = git::load_diff(request.clone()).map_err(AppError::Git)?;
            let root = git::repository_root().map_err(AppError::Git)?;
            run_changeset(
                changeset,
                None,
                language,
                theme,
                Some(ReloadSource::GitDiff { root, request }),
            )
        }
    }
}

fn initialize_review(arguments: ReviewInitArgs) -> Result<(), AppError> {
    let ReviewInitArgs { review: destination, staged, revisions, paths } = arguments;
    let changeset = git::load_diff(DiffRequest { staged, revisions, paths }).map_err(AppError::Git)?;
    let review = Review::new(
        ReviewRevision::new(1).map_err(AppError::InitializeReview)?,
        changeset,
        Vec::new(),
        Vec::new(),
    )
    .map_err(AppError::InitializeReview)?;
    create_review_atomic(Path::new(&destination), &review).map_err(AppError::CreateReviewFile)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "review: {:?}", Path::new(&destination)).map_err(AppError::OutputIo)?;
    writeln!(output, "changeset: {}", review.changeset().fingerprint()).map_err(AppError::OutputIo)?;
    writeln!(output, "revision: {}", review.revision().get()).map_err(AppError::OutputIo)
}

fn open_review(input: OsString, format: Option<OutputFormat>, watch: bool, theme: ThemeFamily) -> Result<(), AppError> {
    let review_path = Path::new(&input).to_owned();
    let review = read_review(&review_path).map_err(AppError::ReviewFile)?;
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    if watch && (format.is_some() || !interactive) {
        return Err(AppError::WatchRequiresTerminal);
    }
    if format.is_some() || !interactive {
        write_review(&review)
    } else if watch {
        let mut watcher = WatchSet::new(&watched_file_parent(&review_path), false).map_err(AppError::Watch)?;
        mire_tui::run_review_watch_with_options(
            review,
            mire_tui::AppOptions { language_override: None, theme, human_author: Some(local_author()) },
            |updated| {
                let expected = ReviewRevision::new(updated.revision().get().saturating_sub(1))
                    .map_err(|error| io::Error::other(error.to_string()))?;
                write_review_atomic_if_revision(&review_path, expected, updated)
                    .map_err(|error| io::Error::other(error.to_string()))
            },
            || {
                if !watcher.reload_due() {
                    return mire_tui::WatchUpdate::Unchanged;
                }
                match read_review(&review_path) {
                    Ok(review) => mire_tui::WatchUpdate::Loaded(review),
                    Err(error) => mire_tui::WatchUpdate::Failed(error.to_string()),
                }
            },
        )
        .map_err(AppError::Terminal)
    } else {
        mire_tui::run_review_with_options(
            &review,
            mire_tui::AppOptions { language_override: None, theme, human_author: Some(local_author()) },
            |updated| {
                let expected = ReviewRevision::new(updated.revision().get().saturating_sub(1))
                    .map_err(|error| io::Error::other(error.to_string()))?;
                write_review_atomic_if_revision(&review_path, expected, updated)
                    .map_err(|error| io::Error::other(error.to_string()))
            },
        )
        .map_err(AppError::Terminal)
    }
}

#[derive(Debug)]
enum ReloadSource {
    GitDiff { root: PathBuf, request: DiffRequest },
    GitShow { root: PathBuf, request: ShowRequest },
    Patch { watch_path: PathBuf, input: OsString },
}

impl ReloadSource {
    fn watch_path(&self) -> (&Path, bool) {
        match self {
            Self::GitDiff { root, .. } | Self::GitShow { root, .. } => (root, true),
            Self::Patch { watch_path, .. } => (watch_path, false),
        }
    }

    fn load(&self) -> Result<Changeset, AppError> {
        match self {
            Self::GitDiff { request, .. } => git::load_diff(request.clone()).map_err(AppError::Git),
            Self::GitShow { request, .. } => git::load_show(request.clone()).map_err(AppError::Git),
            Self::Patch { input, .. } => load_patch(input),
        }
    }
}

fn run_changeset(
    changeset: Changeset, format: Option<OutputFormat>, language: Option<String>, theme: ThemeFamily,
    source: Option<ReloadSource>,
) -> Result<(), AppError> {
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    if source.is_some() && (format.is_some() || !interactive) {
        return Err(AppError::WatchRequiresTerminal);
    }
    if format.is_some() || !interactive {
        write_changeset(&changeset)
    } else if let Some(source) = source {
        let (path, recursive) = source.watch_path();
        let mut watcher = WatchSet::new(path, recursive).map_err(AppError::Watch)?;
        mire_tui::run_watch_with_options(
            changeset,
            mire_tui::AppOptions { language_override: language, theme, human_author: None },
            || {
                if !watcher.reload_due() {
                    return mire_tui::WatchUpdate::Unchanged;
                }
                match source.load() {
                    Ok(changeset) => mire_tui::WatchUpdate::Loaded(changeset),
                    Err(error) => mire_tui::WatchUpdate::Failed(error.to_string()),
                }
            },
        )
        .map_err(AppError::Terminal)
    } else {
        mire_tui::run_with_options(
            &changeset,
            mire_tui::AppOptions { language_override: language, theme, human_author: None },
        )
        .map_err(AppError::Terminal)
    }
}

fn watched_file_parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_owned()
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

fn execute_note(command: NoteCommand) -> Result<(), AppError> {
    match command {
        NoteCommand::Add(arguments) => add_note(arguments),
        NoteCommand::Resolve(arguments) => disposition_note(arguments, NoteStatus::Resolved),
        NoteCommand::Dismiss(arguments) => disposition_note(arguments, NoteStatus::Dismissed),
        NoteCommand::AcceptRisk(arguments) => disposition_note(arguments, NoteStatus::AcceptedRisk),
    }
}

fn execute_notes(command: NotesCommand) -> Result<(), AppError> {
    match command {
        NotesCommand::Apply(NoteApplyArgs { review, stdin: _ }) => apply_notes(&review),
        NotesCommand::Import(NoteImportArgs { review, input, revision }) => import_notes(&review, &input, revision),
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

fn add_note(arguments: NoteAddArgs) -> Result<(), AppError> {
    let NoteAddArgs {
        review: review_path,
        revision,
        file,
        old_line,
        new_line,
        end_line,
        author,
        author_name,
        provenance,
        producer,
        severity,
        annotation_kind,
        body,
    } = arguments;
    let review = read_review(Path::new(&review_path)).map_err(AppError::ReviewFile)?;
    let (side, start_value) = match (old_line, new_line) {
        (Some(line), None) => (AnchorSide::Old, line),
        (None, Some(line)) => (AnchorSide::New, line),
        _ => unreachable!("clap requires exactly one side"),
    };
    let start = LineNumber::new(start_value).ok_or_else(|| {
        AppError::Protocol(ProtocolError::InvalidInput(
            "line number must be greater than zero".to_owned(),
        ))
    })?;
    let end_value = end_line.unwrap_or(start_value);
    let end = LineNumber::new(end_value).ok_or_else(|| {
        AppError::Protocol(ProtocolError::InvalidInput(
            "line number must be greater than zero".to_owned(),
        ))
    })?;
    let range = LineRange::new(start, end).map_err(AppError::InitializeReview)?;
    let path = BytePath::new(file.as_encoded_bytes().to_vec())
        .map_err(|error| AppError::Protocol(ProtocolError::InvalidInput(error.to_string())))?;
    let author = Author::new(author, author_name).map_err(AppError::InitializeReview)?;
    let provenance = match provenance {
        ProvenanceArgument::Agent => Provenance::Agent { producer },
        ProvenanceArgument::Analyzer => Provenance::Analyzer { producer },
        ProvenanceArgument::Interchange => Provenance::Interchange { format: producer, producer: None },
    };
    let input = NoteInput::new(path, side, range, author, provenance, severity, annotation_kind, body)
        .map_err(AppError::InitializeReview)?;
    let updated = review.apply_notes(vec![input]).map_err(|error| {
        apply_error_json(&error)
            .map(|report| AppError::MutationRejected { report })
            .unwrap_or_else(AppError::Protocol)
    })?;
    write_mutation(Path::new(&review_path), revision, &updated)?;
    write_bytes(&import_result_json(&updated, 1).map_err(AppError::Protocol)?)
}

fn apply_notes(review_path: &OsStr) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let bytes = read_protocol_input(OsStr::new("-"), DEFAULT_MAX_REVIEW_FILE_BYTES)?;
    let batch: LocationBatch = serde_json::from_slice(&bytes).map_err(AppError::ProtocolJson)?;
    let (revision, inputs) = batch.into_inputs().map_err(AppError::Protocol)?;
    let applied = inputs.len();
    let updated = review.apply_notes(inputs).map_err(|error| {
        apply_error_json(&error)
            .map(|report| AppError::MutationRejected { report })
            .unwrap_or_else(AppError::Protocol)
    })?;
    write_mutation(Path::new(review_path), revision, &updated)?;
    write_bytes(&import_result_json(&updated, applied).map_err(AppError::Protocol)?)
}

fn disposition_note(arguments: NoteDispositionArgs, status: NoteStatus) -> Result<(), AppError> {
    let NoteDispositionArgs { review: review_path, note_id, revision, author, author_name } = arguments;
    let review = read_review(Path::new(&review_path)).map_err(AppError::ReviewFile)?;
    let note_id = NoteId::new(note_id).map_err(AppError::InitializeReview)?;
    let author = Author::new(author, author_name).map_err(AppError::InitializeReview)?;
    let updated = review
        .change_note_status(&note_id, status, author)
        .map_err(AppError::InitializeReview)?;
    write_mutation(Path::new(&review_path), revision, &updated)?;
    write_bytes(&notes_json(&updated).map_err(AppError::Protocol)?)
}

fn write_mutation(path: &Path, expected_revision: u64, review: &Review) -> Result<(), AppError> {
    let expected = ReviewRevision::new(expected_revision).map_err(AppError::InitializeReview)?;
    write_review_atomic_if_revision(path, expected, review).map_err(|error| match error {
        ReviewFileError::RevisionConflict { expected, actual } => {
            let report = serde_json::to_vec(&serde_json::json!({
                "schema_version": { "major": 1, "minor": 1 },
                "status": "rejected",
                "failures": [{
                    "code": "revision_conflict",
                    "error": format!("review revision conflict: expected {expected}, found {actual}"),
                    "expected": expected,
                    "actual": actual,
                }],
            }))
            .unwrap_or_else(|_| b"{\"status\":\"rejected\"}".to_vec());
            AppError::MutationRejected { report: [report, b"\n".to_vec()].concat() }
        }
        other => AppError::ReviewFile(other),
    })
}

fn import_notes(review_path: &OsStr, input: &OsStr, expected_revision: u64) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let bytes = read_protocol_input(input, DEFAULT_MAX_REVIEW_FILE_BYTES)?;
    let batch: NoteBatch = serde_json::from_slice(&bytes).map_err(AppError::ProtocolJson)?;
    let notes = batch.into_notes().map_err(AppError::Protocol)?;
    let imported = notes.len();
    let updated = match review.import_notes(notes) {
        Ok(updated) => updated,
        Err(error) => {
            let report = import_error_json(&error).map_err(AppError::Protocol)?;
            return Err(AppError::MutationRejected { report });
        }
    };
    write_mutation(Path::new(review_path), expected_revision, &updated)?;
    write_bytes(&import_result_json(&updated, imported).map_err(AppError::Protocol)?)
}

fn parse_fingerprint(value: &str) -> Result<Fingerprint, String> {
    serde_json::from_str(&format!("\"{value}\"")).map_err(|error| error.to_string())
}

fn parse_severity(value: &str) -> Result<NoteSeverity, String> {
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| format!("unsupported severity {value:?}; possible values: note, low, medium, high, critical"))
}

fn parse_annotation_kind(value: &str) -> Result<AnnotationKind, String> {
    serde_json::from_str(&format!("\"{value}\"")).map_err(|_| {
        format!("unsupported annotation kind {value:?}; possible values: comment, defect, suggestion, question")
    })
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
    fn clap_accepts_review_initialization_and_opening() {
        let cli = Cli::try_parse_from(["mire", "review", "init", "review.json", "main...HEAD", "--", "src"]).unwrap();
        let Command::Review(ReviewArgs { command: Some(ReviewCommand::Init(arguments)), .. }) = cli.command else {
            panic!("review init command is parsed");
        };
        assert_eq!(arguments.review, "review.json");
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src"]);

        let cli = Cli::try_parse_from(["mire", "review", "review.json", "--watch"]).unwrap();
        let Command::Review(arguments) = cli.command else {
            panic!("review command is parsed");
        };
        assert_eq!(arguments.input.as_deref(), Some(OsStr::new("review.json")));
        assert!(arguments.watch);
    }

    #[test]
    fn clap_accepts_standalone_and_command_specific_watch_modes() {
        let cli = Cli::try_parse_from(["mire", "watch", "main...HEAD", "--", "src"]).unwrap();
        let Command::Watch(arguments) = cli.command else {
            panic!("watch command is parsed");
        };
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src"]);

        let cli = Cli::try_parse_from(["mire", "patch", "changes.patch", "--watch"]).unwrap();
        let Command::Patch(arguments) = cli.command else {
            panic!("patch command is parsed");
        };
        assert!(arguments.watch);
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
