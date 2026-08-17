use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// A failure while atomically replacing an export file.
#[derive(Debug, Error)]
pub enum OutputFileError {
    /// The destination cannot have a sibling temporary file.
    #[error("output path {path:?} cannot be atomically replaced")]
    InvalidDestination { path: PathBuf },
    /// A sibling temporary file could not be created or synchronized.
    #[error("cannot prepare atomic output write beside {path:?}: {source}")]
    Prepare { path: PathBuf, source: io::Error },
    /// The completed temporary file could not replace the destination.
    #[error("cannot atomically replace output file {path:?}: {source}")]
    Replace { path: PathBuf, source: io::Error },
    /// The destination directory could not be synchronized after replacement.
    #[error("output file {path:?} was replaced, but its directory could not be synchronized: {source}")]
    SyncDirectory { path: PathBuf, source: io::Error },
}

/// Writes complete export bytes to a sibling temporary file, then atomically replaces `path`.
pub fn write_output_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), OutputFileError> {
    let path = path.as_ref();
    let (parent, file_name) = destination_parts(path)?;
    let (temporary_path, mut temporary) = create_temporary(parent, file_name, path)?;
    if let Err(source) = temporary.write_all(bytes).and_then(|()| temporary.sync_all()) {
        drop(temporary);
        let _ = fs::remove_file(&temporary_path);
        return Err(OutputFileError::Prepare { path: path.to_owned(), source });
    }
    drop(temporary);

    if let Err(source) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(OutputFileError::Replace { path: path.to_owned(), source });
    }
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| OutputFileError::SyncDirectory { path: path.to_owned(), source })
}

fn destination_parts(path: &Path) -> Result<(&Path, &OsStr), OutputFileError> {
    let parent = match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
        Some(parent) => parent,
        None => return Err(OutputFileError::InvalidDestination { path: path.to_owned() }),
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| OutputFileError::InvalidDestination { path: path.to_owned() })?;
    Ok((parent, file_name))
}

fn create_temporary(parent: &Path, file_name: &OsStr, destination: &Path) -> Result<(PathBuf, File), OutputFileError> {
    for _ in 0..100 {
        let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!(
            ".{}.mire-export-{}-{id}",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temporary_path) {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(OutputFileError::Prepare { path: destination.to_owned(), source }),
        }
    }
    Err(OutputFileError::Prepare {
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

    #[test]
    fn replacement_writes_complete_bytes_without_leaving_a_temporary_file() {
        let directory = std::env::temp_dir().join(format!("mire-output-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let path = directory.join("review.patch");
        fs::write(&path, b"before\n").unwrap();

        write_output_atomic(&path, b"after\n").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"after\n");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
