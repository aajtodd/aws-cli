//! Error types for the compat testing framework.

use std::fmt;

/// A boxed error that is `Send` and `Sync`.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Errors returned by the compat testing framework.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    source: Option<BoxError>,
}

/// General categories of compat framework errors.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// CLI binary not found at the specified path.
    BinaryNotFound,
    /// CLI process exceeded the timeout.
    Timeout,
    /// Failed to seed S3 state.
    Seed,
    /// Failed to create a local file.
    LocalFile,
    /// Failed to write AWS config.
    Config,
    /// Invalid value in a spec file.
    InvalidSpec,
    /// Mock server error.
    Mock,
    /// S3 SDK operation error.
    Sdk,
    /// Feature requires the mock backend.
    MockOnly,
    /// I/O error.
    Io,
    /// Test harness error (init, lease, release, shutdown).
    Harness,
}

impl Error {
    /// Create an error from a kind and a source error.
    pub fn new<E: Into<BoxError>>(kind: ErrorKind, err: E) -> Self {
        Self {
            kind,
            source: Some(err.into()),
        }
    }

    /// Create an error from a kind alone (no source error).
    pub fn from_kind(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    /// Returns the error kind.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::BinaryNotFound => write!(f, "CLI binary not found"),
            ErrorKind::Timeout => write!(f, "command timed out"),
            ErrorKind::Seed => write!(f, "failed to seed S3 state"),
            ErrorKind::LocalFile => write!(f, "failed to create local file"),
            ErrorKind::Config => write!(f, "failed to write config"),
            ErrorKind::InvalidSpec => write!(f, "invalid spec value"),
            ErrorKind::Mock => write!(f, "mock server error"),
            ErrorKind::Sdk => write!(f, "S3 SDK error"),
            ErrorKind::MockOnly => write!(f, "feature requires mock backend"),
            ErrorKind::Io => write!(f, "I/O error"),
            ErrorKind::Harness => write!(f, "test harness error"),
        }?;
        if let Some(source) = &self.source {
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, err)
    }
}

impl From<s3_mock_server::Error> for Error {
    fn from(err: s3_mock_server::Error) -> Self {
        Self::new(ErrorKind::Mock, err)
    }
}

/// Helper to create a `.map_err()` closure that wraps an error with a kind.
pub(crate) fn from_kind<E>(kind: ErrorKind) -> impl FnOnce(E) -> Error
where
    E: Into<BoxError>,
{
    |err| Error::new(kind, err)
}
