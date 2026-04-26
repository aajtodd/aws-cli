//! S3 CLI subcommand implementations.

pub mod ls;

use crate::cli::S3Command;
use crate::context::AppContext;
use crate::error;
use crate::termerrln;

/// Dispatch a parsed S3 command to its implementation.
pub async fn dispatch(command: S3Command, ctx: &AppContext) -> error::Result<i32> {
    match command {
        S3Command::Ls(args) => ls::run(args, ctx).await,
        _ => {
            termerrln!(ctx.term, "command not yet implemented")?;
            Ok(1)
        }
    }
}
