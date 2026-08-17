//! Command-line orchestration and review-file persistence for Mire.

mod cli;
mod command;
mod git;
mod live_session;
mod output_file;
mod protocol;
mod review_file;
mod serve;
mod skill;
mod watch;

pub use command::run;
pub use git::load_bound_diff;
pub use review_file::{
    DEFAULT_MAX_REVIEW_FILE_BYTES, ReviewFileError, create_review_atomic, read_review, write_review_atomic,
    write_review_atomic_if_revision,
};
