//! Error type shared by the asset core.
//!
//! Hand-written rather than derived so the crate keeps a std-only dependency
//! list. Every variant carries a static context string plus, where useful, the
//! byte offset that made the input unacceptable -- Unity files are untrusted
//! input and "it failed" without a position is close to useless.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input does not start with a recognised signature.
    BadSignature { expected: &'static str, offset: usize },
    /// A structure claims more bytes than the file or object holds.
    Truncated { what: &'static str, offset: usize, wanted: usize, available: usize },
    /// A length or count field is outside any plausible range.
    Implausible { what: &'static str, value: u64 },
    /// A declared size disagrees with the data actually present.
    SizeMismatch { what: &'static str, got: usize, want: usize },
    /// A field value cannot be interpreted (bad offset, unknown enum, ...).
    Invalid { what: &'static str, detail: String },
    /// The format variant is understood by name but not implemented yet.
    Unsupported { what: &'static str },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSignature { expected, offset } => {
                write!(f, "bad signature at {offset}: expected {expected}")
            }
            Error::Truncated { what, offset, wanted, available } => write!(
                f,
                "truncated {what} at {offset}: wanted {wanted} bytes, {available} available"
            ),
            Error::Implausible { what, value } => {
                write!(f, "implausible {what}: {value}")
            }
            Error::SizeMismatch { what, got, want } => {
                write!(f, "{what} size mismatch: got {got}, want {want}")
            }
            Error::Invalid { what, detail } => write!(f, "invalid {what}: {detail}"),
            Error::Unsupported { what } => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Error::Invalid { what: "utf-8 string", detail: e.to_string() }
    }
}
