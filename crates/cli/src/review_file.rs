use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mire_core::{Review, ReviewError, ReviewRevision};
use thiserror::Error;

/// Default upper bound for one serialized review file: 128 MiB.
pub const DEFAULT_MAX_REVIEW_FILE_BYTES: usize = 128 * 1024 * 1024;

static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// A recoverable review-file read or atomic-write failure.
#[derive(Debug, Error)]
pub enum ReviewFileError {
    /// The review path has no file name or parent directory.
    #[error("review path {path:?} cannot be atomically replaced")]
    InvalidDestination { path: PathBuf },
    /// The review file could not be opened, read, or inspected.
    #[error("cannot read review file {path:?}: {source}")]
    Read { path: PathBuf, source: io::Error },
    /// The review file exceeds the explicit input limit.
    #[error("review file {path:?} is {actual} bytes; limit is {limit} bytes")]
    InputTooLarge { path: PathBuf, actual: u64, limit: usize },
    /// JSON syntax, schema, or review invariants rejected the document.
    #[error("invalid review file {path:?}: {source}")]
    InvalidJson { path: PathBuf, source: serde_json::Error },
    /// In-memory review validation failed before any filesystem change.
    #[error("review validation failed before writing {path:?}: {source}")]
    InvalidReview { path: PathBuf, source: ReviewError },
    /// The validated review could not be serialized before any filesystem change.
    #[error("cannot serialize review for {path:?}: {source}")]
    Serialize { path: PathBuf, source: serde_json::Error },
    /// A new review cannot replace an existing filesystem entry.
    #[error("review file {path:?} already exists")]
    AlreadyExists { path: PathBuf },
    /// A sibling temporary file could not be created or written.
    #[error("cannot prepare atomic review write beside {path:?}: {source}")]
    Prepare { path: PathBuf, source: io::Error },
    /// The review changed after a caller read it.
    #[error("review revision conflict: expected {expected}, found {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    /// Another mutation currently owns the review lock.
    #[error("review file {path:?} is being updated by another process")]
    Locked { path: PathBuf },
    /// The completed temporary file could not atomically replace the destination.
    #[error("cannot atomically replace review file {path:?}: {source}")]
    Replace { path: PathBuf, source: io::Error },
    /// The destination directory could not be synchronized after replacement.
    #[error("review file {path:?} was replaced, but its directory could not be synchronized: {source}")]
    SyncDirectory { path: PathBuf, source: io::Error },
}

/// Reads and validates a bounded JSON review file.
pub fn read_review(path: impl AsRef<Path>) -> Result<Review, ReviewFileError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| ReviewFileError::Read { path: path.to_owned(), source })?;
    if let Ok(metadata) = file.metadata() {
        if metadata.len() > DEFAULT_MAX_REVIEW_FILE_BYTES as u64 {
            return Err(ReviewFileError::InputTooLarge {
                path: path.to_owned(),
                actual: metadata.len(),
                limit: DEFAULT_MAX_REVIEW_FILE_BYTES,
            });
        }
    }
    let mut bytes = Vec::new();
    file.take(DEFAULT_MAX_REVIEW_FILE_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| ReviewFileError::Read { path: path.to_owned(), source })?;
    if bytes.len() > DEFAULT_MAX_REVIEW_FILE_BYTES {
        return Err(ReviewFileError::InputTooLarge {
            path: path.to_owned(),
            actual: bytes.len() as u64,
            limit: DEFAULT_MAX_REVIEW_FILE_BYTES,
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| ReviewFileError::InvalidJson { path: path.to_owned(), source })
}

/// Validates, serializes, synchronizes, and atomically replaces a review file.
///
/// Serialization and validation finish before this function creates a sibling
/// temporary file. A failure before the final rename leaves the destination
/// untouched. An abandoned temporary file can be removed after confirming the
/// destination still loads with [`read_review`].
pub fn write_review_atomic(path: impl AsRef<Path>, review: &Review) -> Result<(), ReviewFileError> {
    let path = path.as_ref();
    let serialized = serialize_review(path, review)?;
    let (parent, file_name) = destination_parts(path)?;
    let (temporary_path, temporary) = prepare_temporary(parent, file_name, path, &serialized)?;
    drop(temporary);

    if let Err(source) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(ReviewFileError::Replace { path: path.to_owned(), source });
    }
    sync_directory(parent, path)
}

/// Atomically replaces a review only when its stored revision matches `expected`.
pub fn write_review_atomic_if_revision(
    path: impl AsRef<Path>, expected: ReviewRevision, review: &Review,
) -> Result<(), ReviewFileError> {
    let path = path.as_ref();
    let lock = ReviewLock::acquire(path)?;
    let current = read_review(path)?;
    if current.revision() != expected {
        return Err(ReviewFileError::RevisionConflict { expected: expected.get(), actual: current.revision().get() });
    }
    let result = write_review_atomic(path, review);
    drop(lock);
    result
}

/// Creates a validated review atomically and refuses to replace any existing entry.
///
/// The completed sibling file is linked at the destination only after its
/// contents have been synchronized. Readers therefore see either no destination
/// or the complete review.
pub fn create_review_atomic(path: impl AsRef<Path>, review: &Review) -> Result<(), ReviewFileError> {
    let path = path.as_ref();
    let serialized = serialize_review(path, review)?;
    let (parent, file_name) = destination_parts(path)?;
    let (temporary_path, temporary) = prepare_temporary(parent, file_name, path, &serialized)?;
    drop(temporary);

    if let Err(source) = fs::hard_link(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return if source.kind() == io::ErrorKind::AlreadyExists {
            Err(ReviewFileError::AlreadyExists { path: path.to_owned() })
        } else {
            Err(ReviewFileError::Replace { path: path.to_owned(), source })
        };
    }
    let _ = fs::remove_file(&temporary_path);
    sync_directory(parent, path)
}

struct ReviewLock {
    path: PathBuf,
    _file: File,
}

impl ReviewLock {
    fn acquire(review_path: &Path) -> Result<Self, ReviewFileError> {
        let (parent, file_name) = destination_parts(review_path)?;
        let path = parent.join(format!(".{}.mire-lock", file_name.to_string_lossy()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => Ok(Self { path, _file: file }),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                Err(ReviewFileError::Locked { path: review_path.to_owned() })
            }
            Err(source) => Err(ReviewFileError::Prepare { path: review_path.to_owned(), source }),
        }
    }
}

impl Drop for ReviewLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn serialize_review(path: &Path, review: &Review) -> Result<Vec<u8>, ReviewFileError> {
    review
        .validate()
        .map_err(|source| ReviewFileError::InvalidReview { path: path.to_owned(), source })?;
    let mut serialized =
        serde_json::to_vec(review).map_err(|source| ReviewFileError::Serialize { path: path.to_owned(), source })?;
    serialized.push(b'\n');
    Ok(serialized)
}

fn destination_parts(path: &Path) -> Result<(&Path, &std::ffi::OsStr), ReviewFileError> {
    let parent = match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
        Some(parent) => parent,
        None => return Err(ReviewFileError::InvalidDestination { path: path.to_owned() }),
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| ReviewFileError::InvalidDestination { path: path.to_owned() })?;
    Ok((parent, file_name))
}

fn prepare_temporary(
    parent: &Path, file_name: &std::ffi::OsStr, destination: &Path, serialized: &[u8],
) -> Result<(PathBuf, File), ReviewFileError> {
    let (temporary_path, mut temporary) = create_temporary(parent, file_name, destination)?;
    if let Err(source) = temporary.write_all(serialized).and_then(|()| temporary.sync_all()) {
        drop(temporary);
        let _ = fs::remove_file(&temporary_path);
        return Err(ReviewFileError::Prepare { path: destination.to_owned(), source });
    }
    Ok((temporary_path, temporary))
}

fn sync_directory(parent: &Path, path: &Path) -> Result<(), ReviewFileError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| ReviewFileError::SyncDirectory { path: path.to_owned(), source })
}

fn create_temporary(
    parent: &Path, file_name: &std::ffi::OsStr, destination: &Path,
) -> Result<(PathBuf, File), ReviewFileError> {
    for _ in 0..100 {
        let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!(
            ".{}.mire-write-{}-{id}",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temporary_path) {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(ReviewFileError::Prepare { path: destination.to_owned(), source });
            }
        }
    }
    Err(ReviewFileError::Prepare {
        path: destination.to_owned(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary file",
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mire_core::{Changeset, ChangesetSource, Fingerprint, ReviewRevision};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn atomic_replace_round_trips_and_leaves_no_temporary_file() {
        let directory = test_directory();
        let path = directory.join("review.json");
        let review = review(1);
        write_review_atomic(&path, &review).unwrap();
        assert_eq!(read_review(&path).unwrap(), review);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atomic_create_refuses_to_replace_an_existing_review() {
        let directory = test_directory();
        let path = directory.join("review.json");
        let original = review(1);
        create_review_atomic(&path, &original).unwrap();

        let error = create_review_atomic(&path, &review(2)).unwrap_err();
        assert!(matches!(error, ReviewFileError::AlreadyExists { .. }));
        assert_eq!(read_review(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn an_abandoned_temporary_file_does_not_hide_the_last_valid_review() {
        let directory = test_directory();
        let path = directory.join("review.json");
        let original = review(1);
        write_review_atomic(&path, &original).unwrap();
        fs::write(directory.join(".review.json.mire-write-interrupted"), b"{broken").unwrap();
        assert_eq!(read_review(&path).unwrap(), original);

        let replacement = review(2);
        write_review_atomic(&path, &replacement).unwrap();
        assert_eq!(read_review(&path).unwrap(), replacement);
        fs::remove_dir_all(directory).unwrap();
    }

    fn review(revision: u64) -> Review {
        Review::new(
            ReviewRevision::new(revision).unwrap(),
            Changeset::new(
                ChangesetSource::DirectFiles,
                Vec::new(),
                Fingerprint::new([revision as u8; 32]),
            ),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    fn test_directory() -> PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mire-review-file-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }
}
