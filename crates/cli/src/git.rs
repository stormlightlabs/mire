use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use mire_core::{
    BytePath, BytePathError, ByteString, Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, GitOperation, PatchError,
    PatchLimits, parse_patch,
};
use thiserror::Error;

const MAX_GIT_METADATA_BYTES: usize = 1024 * 1024;
const MAX_GIT_STDERR_BYTES: usize = 1024 * 1024;

type Result<T> = std::result::Result<T, GitError>;

/// A stable failure from discovering or reading a Git repository.
#[derive(Debug, Error)]
pub enum GitError {
    /// The Git executable could not be found.
    #[error("Git is not available: {0}")]
    MissingGit(io::Error),
    /// Git could not be started or awaited.
    #[error("cannot run Git: {0}")]
    Spawn(io::Error),
    /// A piped output stream could not be read.
    #[error("cannot read Git {stream}: {source}")]
    ReadOutput { stream: &'static str, source: io::Error },
    /// A piped output stream exceeded its configured byte limit.
    #[error("Git {stream} exceeded the {limit}-byte limit")]
    OutputTooLarge { stream: &'static str, limit: usize },
    /// The current directory is not in a Git worktree.
    #[error("the current directory is not inside a Git repository")]
    NotRepository,
    /// The repository has no worktree to review.
    #[error("bare Git repositories cannot be reviewed as worktrees")]
    BareRepository,
    /// Git rejected the requested operation.
    #[error("Git {operation} failed{status}: {message}")]
    CommandFailed {
        operation: &'static str,
        status: String,
        message: String,
    },
    /// A path filter cannot be represented as a safe repository-relative path.
    #[error("path filter {path:?} is not a repository-relative path: {source}")]
    InvalidPath { path: OsString, source: BytePathError },
    /// Git returned a repository path that cannot be represented on this platform.
    #[cfg(not(unix))]
    #[error("Git returned a repository path that is not valid on this platform")]
    InvalidRepositoryPath,
    /// Git returned an untracked path that cannot be represented on this platform.
    #[cfg(not(unix))]
    #[error("Git returned an invalid untracked path")]
    InvalidUntrackedPath,
    /// Git emitted a patch outside Mire's supported patch contract.
    #[error("cannot parse Git patch output: {0}")]
    Patch(PatchError),
}

/// A Git-backed diff request from the command line.
#[derive(Debug)]
pub struct DiffRequest {
    /// Whether Git should compare the staged index with `HEAD`.
    pub staged: bool,
    /// Revision arguments forwarded as distinct operating-system strings.
    pub revisions: Vec<OsString>,
    /// Repository-relative path filters forwarded after `--`.
    pub paths: Vec<OsString>,
}

/// A Git-backed commit review request from the command line.
#[derive(Debug)]
pub struct ShowRequest {
    /// The commit to show, or `None` to use `HEAD`.
    pub revision: Option<OsString>,
    /// Repository-relative path filters forwarded after `--`.
    pub paths: Vec<OsString>,
}

#[derive(Debug)]
struct GitRepository {
    root: PathBuf,
}

#[derive(Debug)]
struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
struct StreamCapture {
    bytes: Vec<u8>,
    exceeded_limit: bool,
}

/// Loads a worktree or revision comparison through native Git.
pub fn load_diff(request: DiffRequest) -> Result<Changeset> {
    let repository = discover_repository()?;
    let paths = model_paths(&request.paths)?;
    let source = if request.revisions.is_empty() {
        ChangesetSource::Git { operation: GitOperation::Worktree { staged: request.staged, paths } }
    } else {
        ChangesetSource::Git {
            operation: GitOperation::Diff {
                revisions: request
                    .revisions
                    .iter()
                    .map(|value| ByteString::new(value.as_encoded_bytes()))
                    .collect(),
                paths,
            },
        }
    };

    let mut arguments = diff_options();
    if request.staged {
        arguments.push(OsString::from("--cached"));
    }
    arguments.extend(request.revisions);
    push_paths(&mut arguments, &request.paths);
    let output = run_git(Some(&repository.root), &arguments, DEFAULT_MAX_PATCH_BYTES)?;
    ensure_success("diff", &output, &[0])?;

    let mut patch = output.stdout;
    if !request.staged
        && matches!(
            &source,
            ChangesetSource::Git { operation: GitOperation::Worktree { .. } }
        )
    {
        append_untracked_patches(&repository, &request.paths, &mut patch)?;
    }
    parse_patch(&patch, source, PatchLimits::default()).map_err(GitError::Patch)
}

/// Loads one commit through native Git.
pub fn load_show(request: ShowRequest) -> Result<Changeset> {
    let repository = discover_repository()?;
    let paths = model_paths(&request.paths)?;
    let revision = request.revision.unwrap_or_else(|| OsString::from("HEAD"));
    let source = ChangesetSource::Git {
        operation: GitOperation::Show { revision: ByteString::new(revision.as_encoded_bytes()), paths },
    };
    let mut arguments = diff_options();
    arguments[0] = OsString::from("show");
    arguments.insert(1, OsString::from("--format="));
    arguments.push(revision);
    push_paths(&mut arguments, &request.paths);
    let output = run_git(Some(&repository.root), &arguments, DEFAULT_MAX_PATCH_BYTES)?;
    ensure_success("show", &output, &[0])?;
    parse_patch(&output.stdout, source, PatchLimits::default()).map_err(GitError::Patch)
}

fn discover_repository() -> Result<GitRepository> {
    let bare = run_git(
        None,
        &[OsString::from("rev-parse"), OsString::from("--is-bare-repository")],
        MAX_GIT_METADATA_BYTES,
    )?;
    if !bare.status.success() {
        return Err(GitError::NotRepository);
    }
    if trim_ascii(&bare.stdout) == b"true" {
        return Err(GitError::BareRepository);
    }

    let root = run_git(
        None,
        &[OsString::from("rev-parse"), OsString::from("--show-toplevel")],
        MAX_GIT_METADATA_BYTES,
    )?;
    ensure_success("repository discovery", &root, &[0])?;
    let root = path_from_git_bytes(trim_ascii(&root.stdout))?;
    Ok(GitRepository { root })
}

fn append_untracked_patches(repository: &GitRepository, paths: &[OsString], patch: &mut Vec<u8>) -> Result<()> {
    let mut arguments = vec![
        OsString::from("ls-files"),
        OsString::from("--others"),
        OsString::from("--exclude-standard"),
        OsString::from("--full-name"),
        OsString::from("-z"),
    ];
    push_paths(&mut arguments, paths);
    let output = run_git(Some(&repository.root), &arguments, DEFAULT_MAX_PATCH_BYTES)?;
    ensure_success("list untracked files", &output, &[0])?;

    for path in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let path = os_string_from_git_bytes(path)?;
        let remaining = DEFAULT_MAX_PATCH_BYTES.saturating_sub(patch.len());
        let mut arguments = diff_options();
        arguments.push(OsString::from("--no-index"));
        arguments.push(OsString::from("--"));
        arguments.push(null_device().as_os_str().to_owned());
        arguments.push(path);
        let output = run_git(Some(&repository.root), &arguments, remaining)?;
        ensure_success("diff untracked file", &output, &[0, 1])?;
        patch.extend_from_slice(&output.stdout);
    }
    Ok(())
}

fn run_git(cwd: Option<&Path>, arguments: &[OsString], stdout_limit: usize) -> Result<GitOutput> {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .args(arguments)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            GitError::MissingGit(source)
        } else {
            GitError::Spawn(source)
        }
    })?;
    let stdout = child.stdout.take().expect("piped Git stdout is available");
    let stderr = child.stderr.take().expect("piped Git stderr is available");
    let stdout_reader = thread::spawn(move || read_stream(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || read_stream(stderr, MAX_GIT_STDERR_BYTES));
    let status = child.wait().map_err(GitError::Spawn)?;
    let stdout = stdout_reader
        .join()
        .expect("Git stdout reader does not panic")
        .map_err(|source| GitError::ReadOutput { stream: "stdout", source })?;
    let stderr = stderr_reader
        .join()
        .expect("Git stderr reader does not panic")
        .map_err(|source| GitError::ReadOutput { stream: "stderr", source })?;
    if stdout.exceeded_limit {
        return Err(GitError::OutputTooLarge { stream: "stdout", limit: stdout_limit });
    }
    if stderr.exceeded_limit {
        return Err(GitError::OutputTooLarge { stream: "stderr", limit: MAX_GIT_STDERR_BYTES });
    }
    Ok(GitOutput { status, stdout: stdout.bytes, stderr: stderr.bytes })
}

fn read_stream(stream: impl Read, limit: usize) -> io::Result<StreamCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    stream.take(limit.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    let exceeded_limit = bytes.len() > limit;
    if exceeded_limit {
        bytes.truncate(limit);
    }
    Ok(StreamCapture { bytes, exceeded_limit })
}

fn ensure_success(operation: &'static str, output: &GitOutput, accepted: &[i32]) -> Result<()> {
    if output.status.code().is_some_and(|code| accepted.contains(&code)) {
        return Ok(());
    }
    let status = output
        .status
        .code()
        .map_or_else(String::new, |code| format!(" with status {code}"));
    let message = String::from_utf8_lossy(trim_ascii(&output.stderr));
    let message = if message.is_empty() { "no diagnostic output".to_owned() } else { message.into_owned() };
    Err(GitError::CommandFailed { operation, status, message })
}

fn diff_options() -> Vec<OsString> {
    [
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--binary",
        "--full-index",
        "--find-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn push_paths(arguments: &mut Vec<OsString>, paths: &[OsString]) {
    if !paths.is_empty() {
        arguments.push(OsString::from("--"));
        arguments.extend(paths.iter().cloned());
    }
}

fn model_paths(paths: &[OsString]) -> Result<Vec<BytePath>> {
    paths
        .iter()
        .map(|path| {
            BytePath::new(path.as_encoded_bytes())
                .map_err(|source| GitError::InvalidPath { path: path.clone(), source })
        })
        .collect()
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

#[cfg(unix)]
fn path_from_git_bytes(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
fn path_from_git_bytes(bytes: &[u8]) -> Result<PathBuf> {
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(|_| GitError::InvalidRepositoryPath)
}

#[cfg(unix)]
fn os_string_from_git_bytes(bytes: &[u8]) -> Result<OsString> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn os_string_from_git_bytes(bytes: &[u8]) -> Result<OsString> {
    String::from_utf8(bytes.to_vec())
        .map(OsString::from)
        .map_err(|_| GitError::InvalidUntrackedPath)
}

#[cfg(unix)]
fn null_device() -> &'static Path {
    Path::new("/dev/null")
}

#[cfg(windows)]
fn null_device() -> &'static Path {
    Path::new("NUL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_retains_at_most_the_limit() {
        let capture = read_stream(&b"abcdef"[..], 3).unwrap();
        assert_eq!(capture.bytes, b"abc");
        assert!(capture.exceeded_limit);
    }

    #[test]
    fn path_filters_reject_traversal() {
        let error = model_paths(&[OsString::from("../outside")]).unwrap_err();
        assert!(matches!(
            error,
            GitError::InvalidPath { source: BytePathError::TraversalComponent, .. }
        ));
    }
}
