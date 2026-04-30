//! Native Rust implementation of the `aws s3` CLI subcommand.
//!
//! This crate provides [`handle_s3_cmd`] as the library entry point. The
//! caller is responsible for parsing the CLI, building the SDK client with
//! the appropriate configuration (region, endpoint, credentials), and
//! constructing an [`AppContext`]. This keeps the library free of
//! parsing concerns and fully testable.

pub mod arn;
pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod error;
pub mod format;
pub mod paths;
pub mod redirect;
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
        Ok(()) => 0,
        Err(e) => {
            tracing::debug!(
                kind = ?e.kind,
                source = ?std::error::Error::source(&e),
                "command returned error"
            );
            if !e.message.is_empty() {
                let _ = termerrln!(ctx.term, "{}", e.message);
            }
            e.exit_code()
        }
    }
}
