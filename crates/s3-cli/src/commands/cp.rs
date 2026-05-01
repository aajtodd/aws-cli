use crate::cli::CpArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::paths::format_local_path;
use crate::termoutln;
use crate::transfer;
use crate::uri::TransferUri;

/// Run the `cp` command.
#[tracing::instrument(skip(ctx), fields(source = ?args.source, dest = ?args.dest, recursive = args.recursive))]
pub async fn run(args: CpArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    if args.recursive {
        unimplemented!("cp --recursive not yet implemented");
    }

    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            let src_display = format_local_path(src);
            let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
            transfer::upload_single(ctx, src, &dest.bucket, &dest.key).await?;
            termoutln!(ctx.term, "upload: {src_display} to {dst_display}")?;
            Ok(())
        }
        (TransferUri::S3(src), TransferUri::Local(dest)) => {
            let src_display = format!("s3://{}/{}", src.bucket, src.key);
            let dst_display = format_local_path(dest);
            transfer::download_single(ctx, &src.bucket, &src.key, dest).await?;
            termoutln!(ctx.term, "download: {src_display} to {dst_display}")?;
            Ok(())
        }
        (TransferUri::S3(_), TransferUri::S3(_)) => {
            unimplemented!("S3 to S3 cp not yet implemented");
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => Err(CommandError::param_validation(
            "usage: aws s3 cp <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type",
        )),
    }
}
