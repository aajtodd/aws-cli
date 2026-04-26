//! Shared application context for all S3 CLI subcommands.

use crate::cli::GlobalArgs;
use crate::term;

/// Shared context available to all S3 subcommands.
///
/// Constructed with a pre-configured S3 client and parsed global flags.
/// Subcommand implementations receive this by reference.
pub struct AppContext {
    /// AWS SDK S3 client, pre-configured with region, endpoint, credentials.
    pub client: aws_sdk_s3::Client,
    /// Global CLI flags (`--region`, `--debug`, `--no-sign-request`, etc.).
    pub globals: GlobalArgs,
    /// Terminal for output and terminal control.
    pub term: Box<dyn term::Terminal>,
}

impl AppContext {
    /// Create an AppContext with real stdout/stderr.
    pub fn new(client: aws_sdk_s3::Client, globals: GlobalArgs) -> Self {
        Self {
            client,
            globals,
            term: Box::new(term::StdTerminal::new()),
        }
    }
}
