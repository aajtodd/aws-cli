//! S3 CLI subcommand implementations.

pub mod cp;
pub mod ls;
pub mod mb;
pub mod presign;
pub mod rb;
pub mod rm;
pub mod website;

use crate::cli::S3Command;
use crate::context::AppContext;
use crate::{error, exit_code, termerrln};

/// Dispatch a parsed S3 command to its implementation.
pub async fn dispatch(command: S3Command, ctx: &AppContext) -> error::Result<i32> {
    match command {
        S3Command::Ls(args) => ls::run(args, ctx).await,
        S3Command::Cp(args) => cp::run(args, ctx).await,
        S3Command::Mb(args) => mb::run(args, ctx).await,
        S3Command::Rb(args) => rb::run(args, ctx).await,
        S3Command::Rm(args) => rm::run(args, ctx).await,
        S3Command::Presign(args) => presign::run(args, ctx).await,
        S3Command::Website(args) => website::run(args, ctx).await,
        S3Command::Mv(_) | S3Command::Sync(_) => {
            termerrln!(ctx.term, "command not yet implemented")?;
            Ok(exit_code::FAILURE)
        }
    }
}
