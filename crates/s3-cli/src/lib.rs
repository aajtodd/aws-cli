//! Native Rust implementation of the `aws s3` CLI subcommand.
//!
//! This crate provides [`handle_s3_cmd`] as the library entry point. The
//! caller is responsible for parsing the CLI, building the SDK client with
//! the appropriate configuration (region, endpoint, credentials), and
//! constructing an [`AppContext`]. This keeps the library free of
//! parsing concerns and fully testable.

pub mod cli;
pub mod commands;
pub mod context;
pub mod error;
pub mod format;
pub mod term;
pub mod uri;

pub use cli::S3Command;
pub use context::AppContext;

/// Exit code constants matching `awscli/constants.py`.
pub mod exit_code {
    /// One or more S3 transfer tasks failed.
    pub const FAILURE: i32 = 1;
    /// One or more S3 transfer tasks had warnings (e.g. glacier).
    pub const WARNING: i32 = 2;
    /// Argument or parameter validation error.
    pub const PARAM_VALIDATION_ERROR: i32 = 252;
    /// Configuration error (bad profile, missing config).
    pub const CONFIGURATION_ERROR: i32 = 253;
    /// Service/client error (NoSuchBucket, AccessDenied, etc.).
    pub const CLIENT_ERROR: i32 = 254;
    /// General/unexpected error.
    pub const GENERAL_ERROR: i32 = 255;
}

/// Execute a parsed S3 subcommand.
///
/// The caller owns parsing and SDK configuration. This function only
/// dispatches to the appropriate subcommand implementation.
///
/// Returns the process exit code matching the Python CLI's conventions.
pub async fn handle_s3_cmd(command: S3Command, ctx: &AppContext) -> i32 {
    match commands::dispatch(command, ctx).await {
        Ok(code) => code,
        Err(e) => {
            let _ = termerrln!(ctx.term, "{e}");
            match e {
                error::Error::SdkService(_) => exit_code::CLIENT_ERROR,
                error::Error::InvalidUri(_) => exit_code::PARAM_VALIDATION_ERROR,
                error::Error::Io(_) => exit_code::GENERAL_ERROR,
            }
        }
    }
}
