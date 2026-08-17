use thiserror::Error;

use crate::{Changeset, ChangesetSource};

mod parser;
mod writer;

/// Default upper bound for one raw patch: 64 MiB.
pub const DEFAULT_MAX_PATCH_BYTES: usize = 64 * 1024 * 1024;

pub type Result<T> = std::result::Result<T, PatchError>;

/// A stable failure category for untrusted patch input.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PatchError {
    /// Input exceeded the configured byte limit before parsing began.
    #[error("patch is {actual} bytes; limit is {limit} bytes")]
    InputTooLarge { actual: usize, limit: usize },
    /// The selected parser cannot decode the patch without replacing bytes.
    #[error("patch is not valid {encoding} near byte {offset}")]
    Encoding {
        /// Name of the encoding required at this boundary.
        encoding: &'static str,
        /// Byte offset nearest the decoding failure.
        offset: usize,
    },
    /// Input did not conform to the supported Git or unified patch grammar.
    #[error("invalid patch near byte {offset}: {reason}")]
    Malformed {
        /// Byte offset nearest the invalid input.
        offset: usize,
        /// Actionable description without embedding untrusted patch content.
        reason: String,
    },
    /// Parsed data violated Mire's normalized changeset invariants.
    #[error("invalid normalized patch near byte {offset}: {reason}")]
    InvalidModel {
        /// Byte offset nearest the invalid data.
        offset: usize,
        /// Model invariant that rejected the data.
        reason: String,
    },
}

/// Explicit resource limits applied before patch parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchLimits {
    /// Maximum accepted input size in bytes.
    pub max_bytes: usize,
}

impl Default for PatchLimits {
    fn default() -> Self {
        Self { max_bytes: DEFAULT_MAX_PATCH_BYTES }
    }
}

/// Borrowed patch bytes that have passed the input-size boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchInput<'a> {
    bytes: &'a [u8],
}

impl<'a> PatchInput<'a> {
    /// Applies patch input limits without decoding or parsing the bytes.
    pub fn new(bytes: &'a [u8], limits: PatchLimits) -> Result<Self> {
        if bytes.len() > limits.max_bytes {
            return Err(PatchError::InputTooLarge { actual: bytes.len(), limit: limits.max_bytes });
        }
        Ok(Self { bytes })
    }

    /// Returns the original, byte-for-byte patch input.
    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns UTF-8 text for parsers that cannot preserve arbitrary bytes.
    pub fn as_utf8(self) -> Result<&'a str> {
        std::str::from_utf8(self.bytes)
            .map_err(|error| PatchError::Encoding { encoding: "UTF-8", offset: error.valid_up_to() })
    }
}

/// Parses bounded UTF-8 patch bytes into Mire's normalized changeset model.
pub use writer::{PatchWriteError, write_patch};

pub fn parse_patch(bytes: &[u8], source: ChangesetSource, limits: PatchLimits) -> Result<Changeset> {
    let input = PatchInput::new(bytes, limits)?;
    parser::parse(input.as_utf8()?, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_at_limit_is_accepted_without_text_conversion() {
        let bytes = [0xff, b'\n'];
        let input =
            PatchInput::new(&bytes, PatchLimits { max_bytes: 2 }).expect("input exactly at the limit is accepted");
        assert_eq!(input.as_bytes(), bytes);
    }

    #[test]
    fn oversized_input_has_an_explicit_error() {
        let error =
            PatchInput::new(b"123", PatchLimits { max_bytes: 2 }).expect_err("input over the limit is rejected");
        assert_eq!(error, PatchError::InputTooLarge { actual: 3, limit: 2 });
        assert_eq!(error.to_string(), "patch is 3 bytes; limit is 2 bytes");
    }

    #[test]
    fn invalid_utf8_is_rejected_without_replacement() {
        let input = PatchInput::new(b"valid\n\xff", PatchLimits::default()).expect("input is within the byte limit");
        let error = input
            .as_utf8()
            .expect_err("invalid UTF-8 cannot be passed to a text parser");
        assert_eq!(error, PatchError::Encoding { encoding: "UTF-8", offset: 6 });
    }
}
