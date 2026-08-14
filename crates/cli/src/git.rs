use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use mire_core::{
    BytePath, BytePathError, ByteString, Changeset, ChangesetSource, DEFAULT_MAX_PATCH_BYTES, FilesystemIdentity,
    GitOperation, PatchError, PatchLimits, RepositoryIdentity, SourceBinding, parse_patch,
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
    /// A bound repository path cannot be decoded on this platform.
    #[cfg(not(unix))]
    #[error("the bound repository path cannot be represented on this platform")]
    InvalidBoundRepositoryPath,
    /// A bound revision or path filter cannot be decoded on this platform.
    #[error("the bound Git comparison contains bytes that cannot be represented on this platform")]
    InvalidBoundComparison,
    /// A bound repository path no longer names a readable filesystem entry.
    #[error("bound repository entry {path:?} is unavailable: {source}")]
    RepositoryUnavailable { path: PathBuf, source: io::Error },
    /// The worktree resolves to a different location than the bound repository.
    #[error("bound repository moved: expected {expected:?}, found {actual:?}")]
    RepositoryMoved { expected: PathBuf, actual: PathBuf },
    /// A repository entry no longer has the identity recorded at initialization.
    #[error("bound repository was replaced: {entry} identity changed")]
    RepositoryReplaced { entry: &'static str },
    /// The platform did not return a stable filesystem identity.
    #[cfg(windows)]
    #[error("cannot determine a stable filesystem identity for {path:?}")]
    RepositoryIdentityUnavailable { path: PathBuf },
    /// The binding contains an operation that is not a repeatable comparison.
    #[error("bound Git source is not a worktree or revision comparison")]
    InvalidBoundOperation,
    /// The captured source binding violated a core review invariant.
    #[error("cannot create source binding: {0}")]
    SourceBinding(mire_core::ReviewError),
}

/// A Git-backed diff request from the command line.
#[derive(Clone, Debug)]
pub struct DiffRequest {
    /// Whether Git should compare the staged index with `HEAD`.
    pub staged: bool,
    /// Revision arguments forwarded as distinct operating-system strings.
    pub revisions: Vec<OsString>,
    /// Repository-relative path filters forwarded after `--`.
    pub paths: Vec<OsString>,
}

/// A Git-backed commit review request from the command line.
#[derive(Clone, Debug)]
pub struct ShowRequest {
    /// The commit to show, or `None` to use `HEAD`.
    pub revision: Option<OsString>,
    /// Repository-relative path filters forwarded after `--`.
    pub paths: Vec<OsString>,
}

#[derive(Debug)]
struct GitRepository {
    root: PathBuf,
    git_directory: PathBuf,
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
    load_diff_in_repository(&repository, &request)
}

/// Loads a comparison and records the validated local source needed to repeat it.
pub fn load_diff_with_binding(request: DiffRequest) -> Result<(Changeset, SourceBinding)> {
    let repository = discover_repository()?;
    let source = diff_source(&request)?;
    let ChangesetSource::Git { operation } = &source else {
        unreachable!("diff sources are always Git operations");
    };
    let binding =
        SourceBinding::git(repository_identity(&repository)?, operation.clone()).map_err(GitError::SourceBinding)?;
    let changeset = load_diff_in_repository_with_source(&repository, &request, source)?;
    validate_repository_binding(&binding)?;
    Ok((changeset, binding))
}

/// Repeats a bound comparison only after validating its paths and repository identity.
pub fn load_bound_diff(binding: &SourceBinding) -> Result<Changeset> {
    let repository = validate_repository_binding(binding)?;
    let (_, operation) = binding.git_parts();
    let request = request_from_operation(operation)?;
    let changeset = load_diff_in_repository(&repository, &request)?;
    validate_repository_binding(binding)?;
    Ok(changeset)
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

/// Returns the root watched for Git-backed review changes.
pub fn repository_root() -> Result<PathBuf> {
    discover_repository().map(|repository| repository.root)
}

fn discover_repository() -> Result<GitRepository> {
    discover_repository_at(None)
}

fn discover_repository_at(cwd: Option<&Path>) -> Result<GitRepository> {
    let bare = run_git(
        cwd,
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
        cwd,
        &[OsString::from("rev-parse"), OsString::from("--show-toplevel")],
        MAX_GIT_METADATA_BYTES,
    )?;
    ensure_success("repository discovery", &root, &[0])?;
    let root = canonicalize_repository_path(path_from_git_bytes(trim_ascii(&root.stdout))?)?;

    let git_directory = run_git(
        Some(&root),
        &[OsString::from("rev-parse"), OsString::from("--absolute-git-dir")],
        MAX_GIT_METADATA_BYTES,
    )?;
    ensure_success("Git directory discovery", &git_directory, &[0])?;
    let git_directory = canonicalize_repository_path(path_from_git_bytes(trim_ascii(&git_directory.stdout))?)?;
    Ok(GitRepository { root, git_directory })
}

fn diff_source(request: &DiffRequest) -> Result<ChangesetSource> {
    let paths = model_paths(&request.paths)?;
    let operation = if request.revisions.is_empty() {
        GitOperation::Worktree { staged: request.staged, paths }
    } else {
        GitOperation::Diff {
            revisions: request
                .revisions
                .iter()
                .map(|value| ByteString::new(value.as_encoded_bytes()))
                .collect(),
            paths,
        }
    };
    Ok(ChangesetSource::Git { operation })
}

fn load_diff_in_repository(repository: &GitRepository, request: &DiffRequest) -> Result<Changeset> {
    let source = diff_source(request)?;
    load_diff_in_repository_with_source(repository, request, source)
}

fn load_diff_in_repository_with_source(
    repository: &GitRepository, request: &DiffRequest, source: ChangesetSource,
) -> Result<Changeset> {
    let mut arguments = diff_options();
    if request.staged {
        arguments.push(OsString::from("--cached"));
    }
    arguments.extend(request.revisions.iter().cloned());
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
        append_untracked_patches(repository, &request.paths, &mut patch)?;
    }
    parse_patch(&patch, source, PatchLimits::default()).map_err(GitError::Patch)
}

fn request_from_operation(operation: &GitOperation) -> Result<DiffRequest> {
    match operation {
        GitOperation::Worktree { staged, paths } => Ok(DiffRequest {
            staged: *staged,
            revisions: Vec::new(),
            paths: paths.iter().map(model_path_to_os_string).collect::<Result<_>>()?,
        }),
        GitOperation::Diff { revisions, paths } => Ok(DiffRequest {
            staged: false,
            revisions: revisions.iter().map(byte_string_to_os_string).collect::<Result<_>>()?,
            paths: paths.iter().map(model_path_to_os_string).collect::<Result<_>>()?,
        }),
        GitOperation::Show { .. } => Err(GitError::InvalidBoundOperation),
    }
}

fn repository_identity(repository: &GitRepository) -> Result<RepositoryIdentity> {
    Ok(RepositoryIdentity::new(
        path_to_byte_string(&repository.root)?,
        path_to_byte_string(&repository.git_directory)?,
        filesystem_identity(&repository.root)?,
        filesystem_identity(&repository.git_directory)?,
    ))
}

fn validate_repository_binding(binding: &SourceBinding) -> Result<GitRepository> {
    let (expected, operation) = binding.git_parts();
    let _ = request_from_operation(operation)?;
    let expected_root = path_from_binding_bytes(expected.root().as_bytes())?;
    let expected_git_directory = path_from_binding_bytes(expected.git_directory().as_bytes())?;

    if filesystem_identity(&expected_root)? != *expected.root_filesystem() {
        return Err(GitError::RepositoryReplaced { entry: "worktree root" });
    }
    if filesystem_identity(&expected_git_directory)? != *expected.git_directory_filesystem() {
        return Err(GitError::RepositoryReplaced { entry: "Git directory" });
    }

    let repository = discover_repository_at(Some(&expected_root))?;
    if repository.root != expected_root {
        return Err(GitError::RepositoryMoved { expected: expected_root, actual: repository.root });
    }
    if repository.git_directory != expected_git_directory {
        return Err(GitError::RepositoryReplaced { entry: "Git directory" });
    }
    Ok(repository)
}

fn canonicalize_repository_path(path: PathBuf) -> Result<PathBuf> {
    fs::canonicalize(&path).map_err(|source| GitError::RepositoryUnavailable { path, source })
}

#[cfg(unix)]
fn filesystem_identity(path: &Path) -> Result<FilesystemIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata =
        fs::metadata(path).map_err(|source| GitError::RepositoryUnavailable { path: path.to_owned(), source })?;
    Ok(FilesystemIdentity::unix(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn filesystem_identity(path: &Path) -> Result<FilesystemIdentity> {
    use std::os::windows::fs::MetadataExt;

    let metadata =
        fs::metadata(path).map_err(|source| GitError::RepositoryUnavailable { path: path.to_owned(), source })?;
    let volume_serial_number = metadata
        .volume_serial_number()
        .ok_or_else(|| GitError::RepositoryIdentityUnavailable { path: path.to_owned() })?;
    let file_index = metadata
        .file_index()
        .ok_or_else(|| GitError::RepositoryIdentityUnavailable { path: path.to_owned() })?;
    Ok(FilesystemIdentity::windows(volume_serial_number, file_index))
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

#[cfg(unix)]
fn path_from_binding_bytes(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(unix)]
fn path_to_byte_string(path: &Path) -> Result<ByteString> {
    use std::os::unix::ffi::OsStrExt;

    Ok(ByteString::new(path.as_os_str().as_bytes()))
}

#[cfg(unix)]
fn byte_string_to_os_string(value: &ByteString) -> Result<OsString> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(value.as_bytes().to_vec()))
}

#[cfg(unix)]
fn model_path_to_os_string(path: &BytePath) -> Result<OsString> {
    use std::os::unix::ffi::OsStringExt;

    BytePath::new(path.as_bytes().to_vec()).map_err(|_| GitError::InvalidBoundComparison)?;
    Ok(OsString::from_vec(path.as_bytes().to_vec()))
}

#[cfg(not(unix))]
fn path_from_git_bytes(bytes: &[u8]) -> Result<PathBuf> {
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(|_| GitError::InvalidRepositoryPath)
}

#[cfg(not(unix))]
fn path_from_binding_bytes(bytes: &[u8]) -> Result<PathBuf> {
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(|_| GitError::InvalidBoundRepositoryPath)
}

#[cfg(not(unix))]
fn path_to_byte_string(path: &Path) -> Result<ByteString> {
    path.to_str()
        .map(ByteString::from)
        .ok_or(GitError::InvalidRepositoryPath)
}

#[cfg(not(unix))]
fn byte_string_to_os_string(value: &ByteString) -> Result<OsString> {
    String::from_utf8(value.as_bytes().to_vec())
        .map(OsString::from)
        .map_err(|_| GitError::InvalidBoundComparison)
}

#[cfg(not(unix))]
fn model_path_to_os_string(path: &BytePath) -> Result<OsString> {
    BytePath::new(path.as_bytes().to_vec()).map_err(|_| GitError::InvalidBoundComparison)?;
    String::from_utf8(path.as_bytes().to_vec())
        .map(OsString::from)
        .map_err(|_| GitError::InvalidBoundComparison)
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
