use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use mire_core::{
    AnchorSide, Author, BytePath, BytePathError, Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, LineNumber,
    LineRange, MAX_REVIEW_DESCRIPTION_CHARS, NoteId, NoteInput, NoteStatus, PatchError, PatchLimits, PatchWriteError,
    Provenance, Review, ReviewError, ReviewRevision, normalize_description, parse_patch, validate_review_metadata,
    write_patch,
};
use mire_tui::ThemeFamily;
use thiserror::Error;

use crate::git::{self, DiffRequest, GitError, ShowRequest};
use crate::history::{self, keep_missing_copies};
use crate::live_session::{self, LiveSession, LiveSessionError};
use crate::output_file::{OutputFileError, write_output_atomic};
use crate::protocol::{
    ContextSelection, LocationBatch, NoteBatch, ProtocolError, apply_error_json, check_schema_major,
    checkpoint_rejection_json, context_json, file_checkpoint_json, file_history_json, import_error_json,
    import_result_json, notes_json, notes_markdown, protocol_error_json, review_status_json, review_status_text,
    revision_conflict_json, viewed_cleared_json, viewed_rejection_json, viewed_result_json,
};
use crate::refresh::{RefreshError, refresh_review as refresh_review_file, reload_review};
use crate::review_file::{
    DEFAULT_MAX_REVIEW_FILE_BYTES, ReviewFileError, create_review_atomic, read_review, update_review_viewed,
    write_review_atomic_if_revision,
};
use crate::serve::{self, ServeError};
use crate::skill::{self, SkillError};
use crate::walkthrough::{
    BatchError, WalkthroughBatch, apply_result_json, clear_result_json, location_rejection_json, rejection_json,
    walkthrough_json, walkthrough_text,
};
use crate::watch::{WatchError, WatchSet};

use crate::cli::*;

/// Byte bound on description input: room for a maximum description of
/// four-byte characters plus trailing whitespace and line-ending slack.
const MAX_DESCRIPTION_INPUT_BYTES: usize = MAX_REVIEW_DESCRIPTION_CHARS * 4 + 4096;

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
    #[error("cannot write review: {0}")]
    WriteReviewFile(ReviewFileError),
    #[error("cannot initialize review: {0}")]
    InitializeReview(ReviewError),
    #[error("cannot resolve review destination {path:?}: {source}")]
    ReviewDestination { path: PathBuf, source: io::Error },
    #[error("review destination is not a safe repository-relative path: {0}")]
    ReviewDestinationPath(BytePathError),
    #[error("cannot refresh review: {0}")]
    RefreshReview(ReviewError),
    #[error("review has no reloadable source binding")]
    NonRefreshableReview,
    #[error("cannot read protocol input from {input:?}: {source}")]
    ProtocolInputIo { input: OsString, source: io::Error },
    #[error("protocol input from {input:?} exceeds the {limit}-byte limit")]
    ProtocolInputTooLarge { input: OsString, limit: usize },
    #[error("invalid note batch JSON: {0}")]
    ProtocolJson(serde_json::Error),
    #[error("invalid walkthrough batch JSON: {0}")]
    WalkthroughJson(serde_json::Error),
    #[error("review protocol failed: {0}")]
    Protocol(ProtocolError),
    #[error("note mutation was rejected")]
    MutationRejected { report: Vec<u8> },
    #[error("cannot write JSON output: {0}")]
    Output(serde_json::Error),
    #[error("cannot write command output: {0}")]
    OutputIo(io::Error),
    #[error("cannot export patch: {0}")]
    PatchExport(PatchWriteError),
    #[error("cannot write export output: {0}")]
    ExportOutput(OutputFileError),
    #[error("cannot locate bundled skill: {0}")]
    Skill(SkillError),
    #[error("live-session operation failed: {0}")]
    LiveSession(LiveSessionError),
    #[error("terminal interface failed: {0}")]
    Terminal(io::Error),
    #[error("watch mode requires an interactive terminal and cannot be combined with structured output")]
    WatchRequiresTerminal,
    #[error("watch mode cannot read a patch from standard input")]
    WatchStdin,
    #[error(
        "mire patch cannot read the description from standard input; pass a file or process substitution, such as --description-file <(gh pr view --json body --jq .body)"
    )]
    DescriptionStdin,
    #[error("cannot read description from {input:?}: {source}")]
    DescriptionIo { input: OsString, source: io::Error },
    #[error("description from {input:?} exceeds the {limit}-byte input limit")]
    DescriptionTooLarge { input: OsString, limit: usize },
    #[error("description from {input:?} is not valid UTF-8")]
    DescriptionEncoding { input: OsString },
    #[error("invalid title or description: {0}")]
    Metadata(ReviewError),
    #[error("cannot create web review runtime: {0}")]
    WebReviewRuntime(io::Error),
    #[error("cannot serve review: {0}")]
    Serve(ServeError),
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
            Self::InputIo { .. }
            | Self::DescriptionStdin
            | Self::DescriptionIo { .. }
            | Self::DescriptionTooLarge { .. }
            | Self::DescriptionEncoding { .. }
            | Self::Metadata(_) => 3,
            Self::Patch(_) => 4,
            Self::Output(_)
            | Self::OutputIo(_)
            | Self::PatchExport(_)
            | Self::ExportOutput(_)
            | Self::Skill(_)
            | Self::LiveSession(_)
            | Self::Terminal(_)
            | Self::WebReviewRuntime(_)
            | Self::Serve(_)
            | Self::WatchRequiresTerminal
            | Self::WatchStdin
            | Self::Watch(_) => 5,
            Self::Git(_) => 6,
            Self::ReviewFile(_)
            | Self::CreateReviewFile(_)
            | Self::WriteReviewFile(_)
            | Self::InitializeReview(_)
            | Self::ReviewDestination { .. }
            | Self::ReviewDestinationPath(_)
            | Self::RefreshReview(_)
            | Self::NonRefreshableReview => 7,
            Self::ProtocolInputIo { .. }
            | Self::ProtocolInputTooLarge { .. }
            | Self::ProtocolJson(_)
            | Self::WalkthroughJson(_)
            | Self::Protocol(_)
            | Self::MutationRejected { .. } => 8,
        }
    }
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
    let command = cli.command.unwrap_or(Command::Diff(DiffArgs {
        staged: false,
        revisions: Vec::new(),
        paths: Vec::new(),
        format: None,
        language: None,
        watch: false,
    }));
    match command {
        Command::Context(ContextArgs { review, patch, file, hunk, max_bytes, reader, format }) => {
            let _ = format;
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            let selection = if patch {
                ContextSelection::Patch
            } else if let Some(path) = file.as_deref() {
                ContextSelection::File(path)
            } else if let Some(fingerprint) = hunk {
                ContextSelection::Hunk(fingerprint)
            } else {
                ContextSelection::Manifest { reader: reader.as_deref() }
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
        Command::Patch(PatchArgs { input, format, language, watch, title, description_file }) => {
            if watch && input == "-" {
                return Err(AppError::WatchStdin);
            }
            if description_file.as_deref() == Some(OsStr::new("-")) {
                return Err(AppError::DescriptionStdin);
            }
            let metadata = load_metadata(title, description_file.as_deref())?;
            let changeset = load_patch(&input)?;
            let source =
                watch.then(|| ReloadSource::Patch { watch_path: watched_file_parent(Path::new(&input)), input });
            run_changeset_with_metadata(changeset, format, language, theme, source, metadata)
        }
        Command::Note(NoteArgs { command }) => execute_note(command),
        Command::Notes(NotesArgs { command }) => execute_notes(command),
        Command::Review(ReviewArgs { input, format, watch, command }) => match command {
            Some(ReviewCommand::Init(arguments)) => initialize_review(arguments),
            Some(ReviewCommand::Refresh(arguments)) => refresh_review(arguments),
            Some(ReviewCommand::Status(arguments)) => report_review_status(arguments),
            Some(ReviewCommand::Export(arguments)) => export_review_patch(arguments),
            None => open_review(
                input.expect("clap requires a review path when no review subcommand is present"),
                format,
                watch,
                theme,
            ),
        },
        Command::Serve(arguments) => serve_review(arguments),
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
        Command::Skill(SkillArgs { command: SkillCommand::Path }) => {
            let path = skill::installed_path().map_err(AppError::Skill)?;
            writeln!(io::stdout().lock(), "{}", path.display()).map_err(AppError::OutputIo)
        }
        Command::Session(arguments) => execute_session(arguments),
        Command::Walkthrough(WalkthroughArgs { command }) => execute_walkthrough(command),
        Command::File(FileArgs { command }) => execute_file(command),
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

fn serve_review(arguments: ServeArgs) -> Result<(), AppError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(AppError::WebReviewRuntime)?;
    runtime.block_on(serve::run(arguments)).map_err(AppError::Serve)
}

fn initialize_review(arguments: ReviewInitArgs) -> Result<(), AppError> {
    let ReviewInitArgs { review: destination, staged, revisions, title, description_file, paths } = arguments;
    let (title, description) = load_metadata(title, description_file.as_deref())?;
    let (changeset, mut source_binding) =
        git::load_diff_with_binding(DiffRequest { staged, revisions, paths }).map_err(AppError::Git)?;
    if let Some(path) = review_destination_exclusion(Path::new(&destination), &source_binding)? {
        source_binding = source_binding.with_excluded_path(path);
    }
    let review = Review::new(
        ReviewRevision::new(1).map_err(AppError::InitializeReview)?,
        changeset,
        Vec::new(),
        Vec::new(),
    )
    .and_then(|review| review.with_source_binding(source_binding))
    .and_then(|review| review.with_metadata(title, description))
    .map_err(AppError::InitializeReview)?;
    let review = keep_missing_copies(review).map_err(AppError::Git)?;
    create_review_atomic(Path::new(&destination), &review).map_err(AppError::CreateReviewFile)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "review: {:?}", Path::new(&destination)).map_err(AppError::OutputIo)?;
    writeln!(output, "changeset: {}", review.changeset().fingerprint()).map_err(AppError::OutputIo)?;
    writeln!(output, "revision: {}", review.revision().get()).map_err(AppError::OutputIo)
}

fn review_destination_exclusion(
    destination: &Path, binding: &mire_core::SourceBinding,
) -> Result<Option<BytePath>, AppError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent)
        .map_err(|source| AppError::ReviewDestination { path: destination.to_owned(), source })?;
    let Some(file_name) = destination.file_name() else {
        return Ok(None);
    };
    let absolute = parent.join(file_name);
    let root = git::bound_repository_root(binding).map_err(AppError::Git)?;
    let Ok(relative) = absolute.strip_prefix(root) else {
        return Ok(None);
    };
    BytePath::new(relative.as_os_str().as_encoded_bytes())
        .map(Some)
        .map_err(AppError::ReviewDestinationPath)
}

fn refresh_review(arguments: ReviewRefreshArgs) -> Result<(), AppError> {
    let result = refresh_review_file(Path::new(&arguments.review)).map_err(map_refresh_error)?;
    write_refresh_result(
        if result.changed() { "refreshed" } else { "unchanged" },
        result.review(),
    )
}

fn map_refresh_error(error: RefreshError) -> AppError {
    match error {
        RefreshError::Review(error) => AppError::ReviewFile(error),
        RefreshError::UnavailableSource => AppError::NonRefreshableReview,
        RefreshError::Git(error) => AppError::Git(error),
        RefreshError::Reanchor(error) => AppError::RefreshReview(error),
    }
}

fn write_refresh_result(status: &str, review: &Review) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "status: {status}").map_err(AppError::OutputIo)?;
    writeln!(output, "changeset: {}", review.changeset().fingerprint()).map_err(AppError::OutputIo)?;
    writeln!(output, "revision: {}", review.revision().get()).map_err(AppError::OutputIo)
}

fn report_review_status(arguments: ReviewStatusArgs) -> Result<(), AppError> {
    let review = read_review(Path::new(&arguments.review)).map_err(AppError::ReviewFile)?;
    let bytes = match arguments.format {
        Some(OutputFormat::Json) => review_status_json(&review).map_err(AppError::Protocol)?,
        Some(OutputFormat::Markdown) => unreachable!("review status only accepts JSON output"),
        None => review_status_text(&review),
    };
    write_bytes(&bytes)
}

fn export_review_patch(arguments: ReviewExportArgs) -> Result<(), AppError> {
    let ReviewExportArgs { review, format: ReviewExportFormat::Patch, output } = arguments;
    let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
    let bytes = write_patch(review.changeset()).map_err(AppError::PatchExport)?;
    if let Some(output) = output {
        write_output_atomic(Path::new(&output), &bytes).map_err(AppError::ExportOutput)
    } else {
        write_bytes(&bytes)
    }
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
        let mut review_watcher = WatchSet::new(&watched_file_parent(&review_path), false).map_err(AppError::Watch)?;
        let mut source_watcher = review
            .source_binding()
            .map(|binding| {
                let root = git::bound_repository_root(binding).map_err(AppError::Git)?;
                WatchSet::new(&root, true).map(Some).map_err(AppError::Watch)
            })
            .transpose()?
            .flatten();
        let mut session = LiveSession::start(true).map_err(AppError::LiveSession)?;
        mire_tui::run_review_watch_with_live_control(
            review,
            mire_tui::AppOptions {
                language_override: None,
                theme,
                human_author: Some(local_author()),
                ..mire_tui::AppOptions::default()
            },
            |write| save_terminal_write(&review_path, write),
            |force| {
                let review_due = review_watcher.reload_due();
                if let Some(error) = review_watcher.take_error() {
                    return mire_tui::WatchUpdate::Failed(error.to_string());
                }
                let source_due = source_watcher.as_mut().is_some_and(WatchSet::reload_due);
                if let Some(error) = source_watcher.as_mut().and_then(WatchSet::take_error) {
                    return mire_tui::WatchUpdate::Failed(error.to_string());
                }
                if !force && !review_due && !source_due {
                    return mire_tui::WatchUpdate::Unchanged;
                }
                let latest = match read_review(&review_path) {
                    Ok(review) => review,
                    Err(error) => return mire_tui::WatchUpdate::Fatal(error.to_string()),
                };
                if !source_due {
                    return mire_tui::WatchUpdate::Loaded(latest);
                }
                if latest.source_binding().is_none() {
                    source_watcher = None;
                    return mire_tui::WatchUpdate::Loaded(latest);
                }
                let refreshed = match reload_review(&latest) {
                    Ok(review) => review,
                    Err(error) => return mire_tui::WatchUpdate::Failed(error.to_string()),
                };
                if refreshed == latest {
                    return mire_tui::WatchUpdate::Loaded(latest);
                }
                match write_review_atomic_if_revision(&review_path, latest.revision(), &refreshed) {
                    Ok(()) => mire_tui::WatchUpdate::Loaded(refreshed),
                    Err(ReviewFileError::RevisionConflict { .. } | ReviewFileError::Locked { .. }) => {
                        match read_review(&review_path) {
                            Ok(review) => mire_tui::WatchUpdate::Loaded(review),
                            Err(error) => mire_tui::WatchUpdate::Fatal(error.to_string()),
                        }
                    }
                    Err(error) => mire_tui::WatchUpdate::Failed(error.to_string()),
                }
            },
            session.take_control(),
        )
        .map_err(AppError::Terminal)
    } else {
        let mut session = LiveSession::start(false).map_err(AppError::LiveSession)?;
        mire_tui::run_review_with_live_control(
            &review,
            mire_tui::AppOptions {
                language_override: None,
                theme,
                human_author: Some(local_author()),
                ..mire_tui::AppOptions::default()
            },
            |write| save_terminal_write(&review_path, write),
            session.take_control(),
        )
        .map_err(AppError::Terminal)
    }
}

/// Writes one terminal change: a new revision through the revision check, or
/// a viewed mark applied to the review file as stored.
fn save_terminal_write(review_path: &Path, write: mire_tui::ReviewWrite<'_>) -> io::Result<()> {
    match write {
        mire_tui::ReviewWrite::Revision(updated) => {
            let expected = ReviewRevision::new(updated.revision().get().saturating_sub(1))
                .map_err(|error| io::Error::other(error.to_string()))?;
            write_review_atomic_if_revision(review_path, expected, updated)
        }
        mire_tui::ReviewWrite::Viewed(change) => {
            update_review_viewed(review_path, |review| change.apply(review)).map(|_| ())
        }
    }
    .map_err(|error| io::Error::other(error.to_string()))
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
    run_changeset_with_metadata(changeset, format, language, theme, source, (None, None))
}

fn run_changeset_with_metadata(
    changeset: Changeset, format: Option<OutputFormat>, language: Option<String>, theme: ThemeFamily,
    source: Option<ReloadSource>, (title, description): (Option<String>, Option<String>),
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
        let mut session = LiveSession::start(true).map_err(AppError::LiveSession)?;
        mire_tui::run_watch_with_live_control(
            changeset,
            mire_tui::AppOptions { language_override: language, theme, human_author: None, title, description },
            |force| {
                let reload_due = watcher.reload_due();
                if let Some(error) = watcher.take_error() {
                    return mire_tui::WatchUpdate::Failed(error.to_string());
                }
                if !force && !reload_due {
                    return mire_tui::WatchUpdate::Unchanged;
                }
                match source.load() {
                    Ok(changeset) => mire_tui::WatchUpdate::Loaded(changeset),
                    Err(error) => mire_tui::WatchUpdate::Failed(error.to_string()),
                }
            },
            session.take_control(),
        )
        .map_err(AppError::Terminal)
    } else {
        let mut session = LiveSession::start(false).map_err(AppError::LiveSession)?;
        mire_tui::run_with_live_control(
            &changeset,
            mire_tui::AppOptions { language_override: language, theme, human_author: None, title, description },
            session.take_control(),
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
    check_schema_major(&bytes).map_err(AppError::Protocol)?;
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
        ReviewFileError::RevisionConflict { expected, actual } => revision_conflict_json(expected, actual)
            .map(|report| AppError::MutationRejected { report })
            .unwrap_or_else(AppError::Protocol),
        other => AppError::WriteReviewFile(other),
    })
}

fn import_notes(review_path: &OsStr, input: &OsStr, expected_revision: u64) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let bytes = read_protocol_input(input, DEFAULT_MAX_REVIEW_FILE_BYTES)?;
    check_schema_major(&bytes).map_err(AppError::Protocol)?;
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

fn execute_file(command: FileCommand) -> Result<(), AppError> {
    let (review_path, file, reader, mark) = match command {
        FileCommand::History(arguments) => return file_history(arguments),
        FileCommand::Viewed(FileViewedArgs { review, file, fingerprint, revision, reader }) => {
            (review, Some(file), reader, Some((fingerprint, revision)))
        }
        FileCommand::Unviewed(FileUnviewedArgs { review, file, all: _, reader }) => (review, file, reader, None),
    };
    let reader = reader.unwrap_or_else(|| local_author().id().to_owned());
    let Some(file) = file else {
        return clear_all_viewed(Path::new(&review_path), &reader);
    };
    let path = BytePath::new(file.as_encoded_bytes()).map_err(|error| viewed_rejection("invalid_path", &error))?;
    let result = update_review_viewed(Path::new(&review_path), |review| match mark {
        Some((fingerprint, revision)) => {
            review.mark_viewed(&reader, path.clone(), fingerprint, ReviewRevision::new(revision)?)
        }
        None => review.unmark_viewed(&reader, &path),
    });
    let updated = viewed_update(result)?;
    let status = if mark.is_some() { "viewed" } else { "unviewed" };
    write_bytes(&viewed_result_json(&updated, &reader, &path, status).map_err(AppError::Protocol)?)
}

fn file_history(arguments: FileHistoryArgs) -> Result<(), AppError> {
    let FileHistoryArgs { review, file, revision } = arguments;
    let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
    let path = BytePath::new(file.as_encoded_bytes())
        .map_err(|error| checkpoint_rejection("invalid_path", &error.to_string()))?;
    let Some(revision) = revision else {
        let held = history::held_blobs(&review, &history::history_objects(&review)).ok();
        let entries = review
            .file_history(&path)
            .into_iter()
            .map(|(checkpoint, row)| (checkpoint, row, history::is_available(&review, row, held.as_ref())))
            .collect::<Vec<_>>();
        return write_bytes(&file_history_json(&review, &path, &entries).map_err(AppError::Protocol)?);
    };
    let revision =
        ReviewRevision::new(revision).map_err(|error| checkpoint_rejection("invalid_revision", &error.to_string()))?;
    let (checkpoint_revision, row, rebuilt) = history::file_at_revision(&review, &path, revision)
        .map_err(|failure| checkpoint_rejection(failure.code(), failure.message()))?;
    write_bytes(
        &file_checkpoint_json(
            &review,
            &path,
            revision,
            checkpoint_revision,
            row,
            rebuilt.source.as_str(),
            &rebuilt.content,
        )
        .map_err(AppError::Protocol)?,
    )
}

fn checkpoint_rejection(code: &str, error: &str) -> AppError {
    checkpoint_rejection_json(code, error)
        .map(|report| AppError::MutationRejected { report })
        .unwrap_or_else(AppError::Protocol)
}

fn clear_all_viewed(review_path: &Path, reader: &str) -> Result<(), AppError> {
    let mut cleared = 0;
    let result = update_review_viewed(review_path, |review| {
        let (updated, count) = review.unmark_all_viewed(reader)?;
        cleared = count;
        Ok(updated)
    });
    let updated = viewed_update(result)?;
    write_bytes(&viewed_cleared_json(&updated, reader, cleared).map_err(AppError::Protocol)?)
}

fn viewed_update(result: Result<Review, ReviewFileError>) -> Result<Review, AppError> {
    match result {
        Ok(updated) => Ok(updated),
        Err(ReviewFileError::MarkRejected(error)) => Err(viewed_rejection(error.error_code(), &error)),
        Err(error) => Err(AppError::WriteReviewFile(error)),
    }
}

fn viewed_rejection(code: &str, error: &dyn std::fmt::Display) -> AppError {
    viewed_rejection_json(code, &error.to_string())
        .map(|report| AppError::MutationRejected { report })
        .unwrap_or_else(AppError::Protocol)
}

fn execute_walkthrough(command: WalkthroughCommand) -> Result<(), AppError> {
    match command {
        WalkthroughCommand::Apply(WalkthroughApplyArgs { review, stdin: _ }) => apply_walkthrough(&review),
        WalkthroughCommand::Show(WalkthroughShowArgs { review, format }) => {
            let review = read_review(Path::new(&review)).map_err(AppError::ReviewFile)?;
            let bytes = match format {
                Some(_) => walkthrough_json(&review).map_err(AppError::Protocol)?,
                None => walkthrough_text(&review),
            };
            write_bytes(&bytes)
        }
        WalkthroughCommand::Clear(WalkthroughClearArgs { review, revision }) => clear_walkthrough(&review, revision),
    }
}

fn apply_walkthrough(review_path: &OsStr) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let bytes = read_protocol_input(OsStr::new("-"), DEFAULT_MAX_REVIEW_FILE_BYTES)?;
    check_schema_major(&bytes).map_err(AppError::Protocol)?;
    let batch: WalkthroughBatch = serde_json::from_slice(&bytes).map_err(AppError::WalkthroughJson)?;
    let write = batch.into_write().map_err(|error| match error {
        BatchError::Protocol(error) => AppError::Protocol(error),
        BatchError::Review(error) => walkthrough_rejected(&error),
        BatchError::InvalidLocation { index, error } => location_rejection_json(index, &error)
            .map(|report| AppError::MutationRejected { report })
            .unwrap_or_else(AppError::Protocol),
    })?;
    let expected = ReviewRevision::new(write.revision).map_err(|error| walkthrough_rejected(&error))?;
    let updated = review
        .write_walkthrough(expected, write.input)
        .map_err(|error| walkthrough_rejected(&error))?;
    write_mutation(Path::new(review_path), write.revision, &updated)?;
    write_bytes(&apply_result_json(&updated).map_err(AppError::Protocol)?)
}

fn clear_walkthrough(review_path: &OsStr, revision: u64) -> Result<(), AppError> {
    let review = read_review(Path::new(review_path)).map_err(AppError::ReviewFile)?;
    let expected = ReviewRevision::new(revision).map_err(|error| walkthrough_rejected(&error))?;
    let updated = review
        .clear_walkthrough(expected)
        .map_err(|error| walkthrough_rejected(&error))?;
    let cleared = updated.revision() != review.revision();
    if cleared {
        write_mutation(Path::new(review_path), revision, &updated)?;
    }
    write_bytes(&clear_result_json(&updated, cleared).map_err(AppError::Protocol)?)
}

fn walkthrough_rejected(error: &ReviewError) -> AppError {
    rejection_json(error)
        .map(|report| AppError::MutationRejected { report })
        .unwrap_or_else(AppError::Protocol)
}

fn execute_session(arguments: SessionArgs) -> Result<(), AppError> {
    let response = match arguments.command {
        SessionCommand::List => serde_json::json!({
            "schema_version": { "major": 1, "minor": 0 },
            "status": "ok",
            "sessions": live_session::list_sessions().map_err(AppError::LiveSession)?,
        }),
        SessionCommand::Inspect(SessionTarget { session }) => {
            live_session::request_session(&session, mire_tui::LiveAction::Inspect).map_err(AppError::LiveSession)?
        }
        SessionCommand::Focus(SessionFocusArgs { session, note, file, side, start_line, end_line }) => {
            let action = if let Some(note_id) = note {
                mire_tui::LiveAction::FocusNote { note_id }
            } else {
                let path = file.expect("clap requires a location path when no note is supplied");
                let side = match side.expect("clap requires a location side") {
                    LiveSideArgument::Old => AnchorSide::Old,
                    LiveSideArgument::New => AnchorSide::New,
                };
                let start_line = start_line.expect("clap requires a location start line");
                mire_tui::LiveAction::FocusLocation {
                    path: path.as_encoded_bytes().to_vec(),
                    side,
                    start_line,
                    end_line: end_line.unwrap_or(start_line),
                }
            };
            live_session::request_session(&session, action).map_err(AppError::LiveSession)?
        }
        SessionCommand::Next(SessionTarget { session }) => {
            live_session::request_session(&session, mire_tui::LiveAction::Next).map_err(AppError::LiveSession)?
        }
        SessionCommand::Previous(SessionTarget { session }) => {
            live_session::request_session(&session, mire_tui::LiveAction::Previous).map_err(AppError::LiveSession)?
        }
        SessionCommand::Reload(SessionTarget { session }) => {
            live_session::request_session(&session, mire_tui::LiveAction::Reload).map_err(AppError::LiveSession)?
        }
        SessionCommand::Walkthrough(SessionWalkthroughArgs { command }) => {
            let (session, action) = match command {
                SessionWalkthroughCommand::Start(SessionTarget { session }) => {
                    (session, mire_tui::WalkthroughAction::Start)
                }
                SessionWalkthroughCommand::Next(SessionTarget { session }) => {
                    (session, mire_tui::WalkthroughAction::Next)
                }
                SessionWalkthroughCommand::Previous(SessionTarget { session }) => {
                    (session, mire_tui::WalkthroughAction::Previous)
                }
                SessionWalkthroughCommand::Stop(SessionTarget { session }) => {
                    (session, mire_tui::WalkthroughAction::Stop)
                }
            };
            live_session::request_session(&session, mire_tui::LiveAction::Walkthrough { action })
                .map_err(AppError::LiveSession)?
        }
    };
    let mut output = serde_json::to_vec(&response).map_err(AppError::Output)?;
    output.push(b'\n');
    write_bytes(&output)
}

/// Reads and checks a caller-supplied title and description.
///
/// The description is normalized with [`normalize_description`] before its
/// length is checked, so line endings, stray control characters and trailing
/// whitespace do not count against the limit. A description that is empty
/// after normalizing counts as none.
fn load_metadata(
    title: Option<String>, description_file: Option<&OsStr>,
) -> Result<(Option<String>, Option<String>), AppError> {
    let description = match description_file {
        Some(input) => {
            let bytes = read_bounded_input(input, MAX_DESCRIPTION_INPUT_BYTES)
                .map_err(|source| AppError::DescriptionIo { input: input.to_owned(), source })?;
            if bytes.len() > MAX_DESCRIPTION_INPUT_BYTES {
                return Err(AppError::DescriptionTooLarge {
                    input: input.to_owned(),
                    limit: MAX_DESCRIPTION_INPUT_BYTES,
                });
            }
            let text =
                String::from_utf8(bytes).map_err(|_| AppError::DescriptionEncoding { input: input.to_owned() })?;
            normalize_description(&text)
        }
        None => None,
    };
    validate_review_metadata(title.as_deref(), description.as_deref()).map_err(AppError::Metadata)?;
    Ok((title, description))
}

fn read_bounded_input(input: &OsStr, limit: usize) -> io::Result<Vec<u8>> {
    if input == "-" {
        read_bounded(io::stdin().lock(), limit)
    } else {
        read_bounded(File::open(Path::new(input))?, limit)
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
    if let Ok(metadata) = file.metadata()
        && metadata.len() > limit as u64
    {
        let actual = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        return Err(AppError::Patch(PatchError::InputTooLarge { actual, limit }));
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
        let Some(Command::Patch(arguments)) = cli.command else {
            panic!("patch command is parsed");
        };
        assert_eq!(arguments.input, "-");
        assert!(matches!(arguments.format, Some(OutputFormat::Json)));
    }

    #[test]
    fn clap_defaults_to_the_worktree_diff() {
        let cli = Cli::try_parse_from(["mire"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn clap_separates_revisions_and_paths() {
        let cli = Cli::try_parse_from(["mire", "diff", "main...HEAD", "--", "src/lib.rs"]).unwrap();
        let Some(Command::Diff(arguments)) = cli.command else {
            panic!("diff command is parsed");
        };
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src/lib.rs"]);
    }

    #[test]
    fn clap_accepts_review_initialization_opening_status_and_serving() {
        let cli = Cli::try_parse_from(["mire", "review", "init", "review.json", "main...HEAD", "--", "src"]).unwrap();
        let Some(Command::Review(ReviewArgs { command: Some(ReviewCommand::Init(arguments)), .. })) = cli.command
        else {
            panic!("review init command is parsed");
        };
        assert_eq!(arguments.review, "review.json");
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src"]);

        let cli = Cli::try_parse_from(["mire", "review", "review.json", "--watch"]).unwrap();
        let Some(Command::Review(arguments)) = cli.command else {
            panic!("review command is parsed");
        };
        assert_eq!(arguments.input.as_deref(), Some(OsStr::new("review.json")));
        assert!(arguments.watch);

        let cli = Cli::try_parse_from(["mire", "review", "status", "review.json", "--format", "json"]).unwrap();
        let Some(Command::Review(ReviewArgs { command: Some(ReviewCommand::Status(arguments)), .. })) = cli.command
        else {
            panic!("review status command is parsed");
        };
        assert_eq!(arguments.review, "review.json");
        assert!(matches!(arguments.format, Some(OutputFormat::Json)));

        let cli = Cli::try_parse_from([
            "mire",
            "review",
            "export",
            "review.json",
            "--format",
            "patch",
            "--output",
            "review.patch",
        ])
        .unwrap();
        let Some(Command::Review(ReviewArgs { command: Some(ReviewCommand::Export(arguments)), .. })) = cli.command
        else {
            panic!("review export command is parsed");
        };
        assert_eq!(arguments.review, "review.json");
        assert!(matches!(arguments.format, ReviewExportFormat::Patch));
        assert_eq!(arguments.output.as_deref(), Some(OsStr::new("review.patch")));

        let cli = Cli::try_parse_from(["mire", "serve", "review.json", "--port", "3737", "--open"]).unwrap();
        let Some(Command::Serve(arguments)) = cli.command else {
            panic!("serve command is parsed");
        };
        assert_eq!(arguments.review, "review.json");
        assert_eq!(arguments.port, Some(3737));
        assert!(arguments.open);
    }

    #[test]
    fn clap_accepts_standalone_and_command_specific_watch_modes() {
        let cli = Cli::try_parse_from(["mire", "watch", "main...HEAD", "--", "src"]).unwrap();
        let Some(Command::Watch(arguments)) = cli.command else {
            panic!("watch command is parsed");
        };
        assert_eq!(arguments.revisions, ["main...HEAD"]);
        assert_eq!(arguments.paths, ["src"]);

        let cli = Cli::try_parse_from(["mire", "patch", "changes.patch", "--watch"]).unwrap();
        let Some(Command::Patch(arguments)) = cli.command else {
            panic!("patch command is parsed");
        };
        assert!(arguments.watch);
    }

    #[test]
    fn clap_accepts_supported_language_overrides_and_rejects_unknown_ones() {
        let cli = Cli::try_parse_from(["mire", "patch", "-", "--language", "typescript"]).unwrap();
        let Some(Command::Patch(arguments)) = cli.command else {
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
