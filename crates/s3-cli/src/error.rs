//! Error types for the S3 CLI.

use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use std::fmt;

use crate::exit_code;

/// Errors that can occur during internal S3 CLI operations before they get
/// classified into a [`CommandError`] for return to the dispatcher.
///
/// Command implementations may use this for internal plumbing
/// (e.g. propagating I/O errors through `?`) but the final error they
/// return to the dispatcher is always a [`CommandError`].
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

/// Convenience alias used for internal command plumbing.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// CommandError — the error type returned by command implementations.
// ---------------------------------------------------------------------------

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Classification of command errors.
///
/// The kind drives default exit-code mapping (see [`default_exit_code`]),
/// but carries no exit-code information itself. To change exit-code policy,
/// update the mapping function in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandErrorKind {
    /// Transfer task failure. Used for SDK errors reported with a
    /// command-specific prefix (e.g. "make_bucket failed: ..."), and for
    /// partial recursive failures where per-item errors have been printed
    /// inline.
    Failure,

    /// Transfer task warning (e.g. glacier scenarios Python reports as warnings).
    Warning,

    /// Parameter validation error — invalid or malformed arguments.
    ParamValidation,

    /// Configuration error — invalid config file, missing region, etc.
    Configuration,

    /// Service-level client error. Used when the message follows the bare
    /// botocore template without a command-specific prefix.
    Client,

    /// General error — covers cases not classified above.
    General,
}

/// Errors returned by command implementations.
///
/// Carries a kind classifying the error (for exit-code mapping and tracing),
/// a user-facing stderr message, and an optional source chain. The kind
/// determines the default exit code; a specific command may override it via
/// [`CommandError::with_exit_code`] if a rare case calls for it.
///
/// Partial-failure cases (where per-item errors have already been printed
/// inline to stderr) use [`CommandError::partial_failure`] — an empty
/// message signals the dispatcher to skip stderr output.
#[derive(Debug)]
pub struct CommandError {
    pub kind: CommandErrorKind,
    pub message: String,
    pub source: Option<BoxError>,
    /// Explicit exit-code override. If `None`, uses the default for `kind`.
    exit_code_override: Option<i32>,
}

/// Single place where [`CommandErrorKind`] maps to a process exit code.
///
/// Any change to exit-code policy happens here. Commands never reference
/// exit codes directly; they construct a [`CommandError`] with a kind and
/// the dispatcher calls [`CommandError::exit_code`].
fn default_exit_code(kind: CommandErrorKind) -> i32 {
    match kind {
        CommandErrorKind::Failure => exit_code::FAILURE,
        CommandErrorKind::Warning => exit_code::WARNING,
        CommandErrorKind::ParamValidation => exit_code::PARAM_VALIDATION_ERROR,
        CommandErrorKind::Configuration => exit_code::CONFIGURATION_ERROR,
        CommandErrorKind::Client => exit_code::CLIENT_ERROR,
        CommandErrorKind::General => exit_code::GENERAL_ERROR,
    }
}

impl CommandError {
    /// Construct an error with the given kind and stderr message.
    pub fn new(kind: CommandErrorKind, message: impl Into<String>) -> Self {
        CommandError {
            kind,
            message: message.into(),
            source: None,
            exit_code_override: None,
        }
    }

    /// Attach an error source for tracing / debugging.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Box::new(source));
        self
    }

    /// Override the default exit code. Rarely needed.
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code_override = Some(code);
        self
    }

    /// Process exit code. Uses the explicit override if set, otherwise the
    /// default for [`Self::kind`].
    pub fn exit_code(&self) -> i32 {
        self.exit_code_override
            .unwrap_or_else(|| default_exit_code(self.kind))
    }

    // Convenience constructors keep call sites terse.

    /// Construct a [`CommandErrorKind::Failure`] error.
    pub fn failure(message: impl Into<String>) -> Self {
        Self::new(CommandErrorKind::Failure, message)
    }

    /// Construct a [`CommandErrorKind::ParamValidation`] error.
    pub fn param_validation(message: impl Into<String>) -> Self {
        Self::new(CommandErrorKind::ParamValidation, message)
    }

    /// Construct a [`CommandErrorKind::Configuration`] error.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(CommandErrorKind::Configuration, message)
    }

    /// Construct a [`CommandErrorKind::Client`] error.
    pub fn client(message: impl Into<String>) -> Self {
        Self::new(CommandErrorKind::Client, message)
    }

    /// Construct a [`CommandErrorKind::General`] error.
    pub fn general(message: impl Into<String>) -> Self {
        Self::new(CommandErrorKind::General, message)
    }

    /// Marker for partial-failure in recursive operations. Per-item failure
    /// messages have already been printed inline to stderr; this just flags
    /// the non-zero exit without duplicating output.
    pub fn partial_failure() -> Self {
        Self::new(CommandErrorKind::Failure, "")
    }

    /// Feature not yet implemented. Prints a clear message to stderr and
    /// exits with PARAM_VALIDATION_ERROR (252) — same code Python uses for
    /// unsupported argument combinations.
    pub fn not_implemented(feature: &str) -> Self {
        Self::new(
            CommandErrorKind::ParamValidation,
            format!("{feature} is not yet supported in this build."),
        )
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|s| s as &(dyn std::error::Error + 'static))
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

// Convenience conversions from internal Error to CommandError.
// These match the prior lib.rs dispatcher mapping.
impl From<Error> for CommandError {
    fn from(e: Error) -> Self {
        match &e {
            Error::SdkService(msg) => CommandError::client(msg.clone()),
            Error::InvalidUri(uri) => {
                CommandError::param_validation(format!("Invalid S3 URI: {uri}"))
            }
            Error::Io(io) => CommandError::general(io.to_string()),
        }
    }
}

/// I/O errors from terminal writes and filesystem operations become
/// `CommandError::General`. These are typically EPIPE (broken pipe) during
/// output, which Python CLI treats as a generic error.
impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::general(e.to_string()).with_source(e)
    }
}
