//! Error types for the S3 CLI.

use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use std::fmt;

/// Errors that can occur during S3 CLI operations.
#[derive(Debug)]
pub enum Error {
    /// An error from an S3 service operation, formatted to match the Python CLI.
    SdkService(String),

    /// Invalid S3 URI.
    InvalidUri(String),

    /// I/O error.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SdkService(msg) => write!(f, "{msg}"),
            Error::InvalidUri(uri) => write!(f, "Invalid S3 URI: {uri}"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Format an SDK error to match the Python CLI's error format:
///
/// `An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}`
///
/// This matches `botocore/exceptions.py` `ClientError.MSG_TEMPLATE`.
pub fn format_sdk_error(
    err: &(impl ProvideErrorMetadata + std::error::Error),
    operation: &str,
) -> String {
    let code = err.code().unwrap_or("Unknown");
    let message = err.message().unwrap_or("Unknown");
    format!("An error occurred ({code}) when calling the {operation} operation: {message}")
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;
