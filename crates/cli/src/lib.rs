//! Command-line orchestration and review-file persistence for Mire.

mod command;
mod git;
mod review_file;

pub use command::run;
pub use review_file::{DEFAULT_MAX_REVIEW_FILE_BYTES, ReviewFileError, read_review, write_review_atomic};
