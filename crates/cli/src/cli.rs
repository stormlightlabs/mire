//! Command-line grammar shared by Mire and its generated documentation.

use std::ffi::OsString;

use clap::{Args, Parser, Subcommand, ValueEnum, ValueHint};
use mire_core::{AnnotationKind, Fingerprint, NoteSeverity};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ThemeArgument {
    #[default]
    Auto,
    Iceberg,
    Eldritch,
    Catppuccin,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Json,
    Markdown,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Export context from a review for an agent or tool.
    Context(ContextArgs),
    /// Review worktree changes or compare Git revisions.
    Diff(DiffArgs),
    /// Review a patch file or standard input.
    Patch(PatchArgs),
    /// Add or update one review finding.
    Note(NoteArgs),
    /// Apply, import, list, or export review findings.
    Notes(NotesArgs),
    /// Open, create, or refresh a review file.
    Review(ReviewArgs),
    /// Serve a review in a local browser.
    Serve(ServeArgs),
    /// Review a Git commit.
    Show(ShowArgs),
    /// Install and print the bundled agent skill path.
    Skill(SkillArgs),
    /// Inspect or control an open local Mire session.
    Session(SessionArgs),
    /// Review a Git comparison and reload it when it changes.
    Watch(WatchArgs),
}

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    /// Add a finding at a source location.
    Add(NoteAddArgs),
    /// Mark one finding as resolved.
    Resolve(NoteDispositionArgs),
    /// Dismiss one finding.
    Dismiss(NoteDispositionArgs),
    /// Accept the risk for a finding.
    AcceptRisk(NoteDispositionArgs),
}

#[derive(Debug, Subcommand)]
pub enum NotesCommand {
    /// Apply location-based findings from standard input.
    Apply(NoteApplyArgs),
    /// Import a full finding batch.
    Import(NoteImportArgs),
    /// List findings as JSON.
    List(NoteListArgs),
    /// Export findings as JSON or Markdown.
    Export(NoteExportArgs),
}

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Create a review from a Git comparison.
    Init(ReviewInitArgs),
    /// Refresh a source-backed review and re-anchor its findings.
    Refresh(ReviewRefreshArgs),
    /// Report review progress without opening the terminal interface.
    Status(ReviewStatusArgs),
    /// Export the captured changeset without review metadata.
    Export(ReviewExportArgs),
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Install and print the bundled agent skill path.
    Path,
}

#[derive(Debug, Parser)]
#[command(name = "mire", version, about = "Review code changes in the terminal.")]
pub struct Cli {
    /// Theme for the interactive viewer.
    #[arg(long, global = true, value_enum, default_value_t)]
    pub theme: ThemeArgument,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    /// JSON review file to inspect.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Include the complete normalized patch capture.
    #[arg(long, conflicts_with = "file", requires = "max_bytes")]
    pub patch: bool,
    /// Include one complete normalized file diff.
    #[arg(
        long,
        value_name = "PATH",
        value_hint = ValueHint::AnyPath,
        conflicts_with_all = ["patch", "hunk"],
        requires = "max_bytes"
    )]
    pub file: Option<OsString>,
    /// Include one hunk selected from the manifest.
    #[arg(
        long,
        value_name = "FINGERPRINT",
        value_parser = parse_fingerprint,
        conflicts_with_all = ["patch", "file"],
        requires = "max_bytes"
    )]
    pub hunk: Option<Fingerprint>,
    /// Maximum serialized bytes for an expanded context request.
    #[arg(long, value_name = "BYTES")]
    pub max_bytes: Option<usize>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, default_value = "json")]
    pub format: Option<OutputFormat>,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    pub staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    pub revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH", value_hint = ValueHint::AnyPath)]
    pub paths: Vec<OsString>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    pub format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    pub language: Option<String>,
    /// Reload the interactive review when the repository changes.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Args, Debug)]
pub struct PatchArgs {
    /// Patch file to read, or - for standard input.
    #[arg(value_hint = ValueHint::FilePath)]
    pub input: OsString,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    pub format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    pub language: Option<String>,
    /// Reload the interactive review when the patch file changes.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Args, Debug)]
pub struct NoteArgs {
    #[command(subcommand)]
    pub command: NoteCommand,
}

#[derive(Args, Debug)]
pub struct NotesArgs {
    #[command(subcommand)]
    pub command: NotesCommand,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ProvenanceArgument {
    Agent,
    Analyzer,
    Interchange,
}

#[derive(Args, Debug)]
pub struct NoteAddArgs {
    /// JSON review file to update.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Revision read before creating the finding.
    #[arg(long)]
    pub revision: u64,
    /// Repository-relative source path.
    #[arg(long, value_hint = ValueHint::AnyPath)]
    pub file: OsString,
    /// Line on the old side of the diff.
    #[arg(long, conflicts_with = "new_line", required_unless_present = "new_line")]
    pub old_line: Option<u64>,
    /// Line on the new side of the diff.
    #[arg(long, conflicts_with = "old_line", required_unless_present = "old_line")]
    pub new_line: Option<u64>,
    /// Final line of the finding range, inclusive.
    #[arg(long)]
    pub end_line: Option<u64>,
    /// Identifier for the finding author.
    #[arg(long)]
    pub author: String,
    /// Optional display name for the finding author.
    #[arg(long)]
    pub author_name: Option<String>,
    /// Source that produced the finding.
    #[arg(long, value_enum)]
    pub provenance: ProvenanceArgument,
    /// Name of the agent, analyzer, or interchange format.
    #[arg(long)]
    pub producer: String,
    /// Finding severity.
    #[arg(long, value_parser = parse_severity)]
    pub severity: NoteSeverity,
    /// Finding kind.
    #[arg(long = "kind", value_parser = parse_annotation_kind)]
    pub annotation_kind: AnnotationKind,
    /// Finding text.
    #[arg(long)]
    pub body: String,
}

#[derive(Args, Debug)]
pub struct NoteDispositionArgs {
    /// JSON review file to update.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Identifier of the finding to update.
    pub note_id: String,
    /// Revision read before changing the finding.
    #[arg(long)]
    pub revision: u64,
    /// Identifier for the person changing the finding.
    #[arg(long)]
    pub author: String,
    /// Optional display name for the person changing the finding.
    #[arg(long)]
    pub author_name: Option<String>,
}

#[derive(Args, Debug)]
pub struct NoteApplyArgs {
    /// JSON review file to update.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Read the location batch from standard input.
    #[arg(long, required = true)]
    pub stdin: bool,
}

#[derive(Args, Debug)]
pub struct NoteImportArgs {
    /// JSON review file to update atomically.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Note batch JSON file, or - for standard input.
    #[arg(value_hint = ValueHint::FilePath)]
    pub input: OsString,
    /// Review revision observed before constructing the batch.
    #[arg(long)]
    pub revision: u64,
}

#[derive(Args, Debug)]
pub struct NoteListArgs {
    /// JSON review file to inspect.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, default_value = "json")]
    pub format: Option<OutputFormat>,
}

#[derive(Args, Debug)]
pub struct NoteExportArgs {
    /// JSON review file to export.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Export format.
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args, Debug)]
#[command(
    arg_required_else_help = true,
    after_help = "Create and open a review:\n  mire review init review.json main...HEAD -- src\n  mire review review.json --watch"
)]
pub struct ReviewArgs {
    /// JSON review file to open.
    #[arg(value_hint = ValueHint::FilePath)]
    pub input: Option<OsString>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format, requires = "input")]
    pub format: Option<OutputFormat>,
    /// Reload the interactive review when the review file changes.
    #[arg(long, requires = "input")]
    pub watch: bool,
    #[command(subcommand)]
    pub command: Option<ReviewCommand>,
}

#[derive(Args, Debug)]
pub struct ReviewRefreshArgs {
    /// Existing source-backed review file to refresh.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
}

#[derive(Args, Debug)]
pub struct ReviewStatusArgs {
    /// JSON review file to inspect.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Emit deterministic JSON for scripts and agents.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    pub format: Option<OutputFormat>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ReviewExportFormat {
    Patch,
}

#[derive(Args, Debug)]
pub struct ReviewExportArgs {
    /// JSON review file to export.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Export format.
    #[arg(long, value_enum)]
    pub format: ReviewExportFormat,
    /// Write the complete export by atomically replacing this file.
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub output: Option<OsString>,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// JSON review file to serve.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Loopback port to listen on.
    #[arg(long)]
    pub port: Option<u16>,
    /// Open the session URL in the default browser.
    #[arg(long)]
    pub open: bool,
}

#[derive(Args, Debug)]
pub struct ReviewInitArgs {
    /// New JSON review file to create.
    #[arg(value_hint = ValueHint::FilePath)]
    pub review: OsString,
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    pub staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    pub revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH", value_hint = ValueHint::AnyPath)]
    pub paths: Vec<OsString>,
}

#[derive(Args, Debug)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List local interactive sessions.
    List,
    /// Inspect one local session's presentation state.
    Inspect(SessionTarget),
    /// Focus a finding or changed source location.
    Focus(SessionFocusArgs),
    /// Move to the next visible finding.
    Next(SessionTarget),
    /// Move to the previous visible finding.
    Previous(SessionTarget),
    /// Request the normal reload path for a watched session.
    Reload(SessionTarget),
    /// Start, stop, or advance a coordinated walkthrough.
    Walkthrough(SessionWalkthroughArgs),
}

#[derive(Args, Debug)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Args, Debug)]
pub struct SessionTarget {
    /// Identifier from `mire session list`.
    pub session: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum LiveSideArgument {
    Old,
    New,
}

#[derive(Args, Debug)]
pub struct SessionFocusArgs {
    /// Identifier from `mire session list`.
    pub session: String,
    /// Stable finding identifier.
    #[arg(long, conflicts_with = "file", required_unless_present = "file")]
    pub note: Option<String>,
    /// Repository-relative source path.
    #[arg(long, value_hint = ValueHint::AnyPath, requires_all = ["side", "start_line"])]
    pub file: Option<OsString>,
    /// Changed-file side for a location request.
    #[arg(long, value_enum, requires = "file")]
    pub side: Option<LiveSideArgument>,
    /// Inclusive first line in the selected side.
    #[arg(long, requires = "file")]
    pub start_line: Option<u64>,
    /// Inclusive final line in the selected side.
    #[arg(long, requires = "file")]
    pub end_line: Option<u64>,
}

#[derive(Debug, Subcommand)]
pub enum WalkthroughCommand {
    /// Start a walkthrough.
    Start(SessionTarget),
    /// Advance the walkthrough to the next visible finding.
    Next(SessionTarget),
    /// Move the walkthrough to the previous visible finding.
    Previous(SessionTarget),
    /// End the walkthrough.
    Stop(SessionTarget),
}

#[derive(Args, Debug)]
pub struct SessionWalkthroughArgs {
    #[command(subcommand)]
    pub command: WalkthroughCommand,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Commit to show; defaults to HEAD.
    pub revision: Option<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH", value_hint = ValueHint::AnyPath)]
    pub paths: Vec<OsString>,
    /// Structured output format.
    #[arg(long, value_name = "FORMAT", value_parser = parse_json_format)]
    pub format: Option<OutputFormat>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    pub language: Option<String>,
    /// Reload the interactive review when the repository changes.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Compare the staged index with HEAD.
    #[arg(long, conflicts_with = "revisions")]
    pub staged: bool,
    /// Git revision or revision range to compare.
    #[arg(value_name = "REVISION")]
    pub revisions: Vec<OsString>,
    /// Repository-relative paths, supplied after --.
    #[arg(last = true, value_name = "PATH", value_hint = ValueHint::AnyPath)]
    pub paths: Vec<OsString>,
    /// Override syntax detection for the interactive viewer.
    #[arg(long, value_parser = parse_language)]
    pub language: Option<String>,
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
