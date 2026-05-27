use crate::cli::MvArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::paths::format_local_path;
use crate::termoutln;
use crate::transfer;
use crate::uri::TransferUri;

/// Run the `mv` command.
#[tracing::instrument(skip(ctx), fields(source = ?args.source, dest = ?args.dest, recursive = args.recursive))]
pub async fn run(args: MvArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    if args.recursive {
        return Err(CommandError::not_implemented("mv --recursive"));
    }

    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            let src_display = format_local_path(src);
            let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
            let ct = super::cp::resolve_content_type(
                src,
                args.transfer.content_type.as_deref(),
                args.transfer.no_guess_mime_type,
            );
            transfer::upload_single(ctx, src, &dest.bucket, &dest.key, ct.as_deref()).await?;
            std::fs::remove_file(src).map_err(|e| {
                tracing::debug!(error = %e, source = %src.display(), "failed to delete source after upload");
                CommandError::failure(format!("failed to delete source file: {e}")).with_source(e)
            })?;
            termoutln!(ctx.term, "move: {src_display} to {dst_display}")?;
            Ok(())
        }
        (TransferUri::S3(src), TransferUri::Local(dest)) => {
            let src_display = format!("s3://{}/{}", src.bucket, src.key);
            let dst_display = format_local_path(dest);
            transfer::download_single(ctx, &src.bucket, &src.key, dest).await?;
            ctx.client
                .delete_object()
                .bucket(&src.bucket)
                .key(&src.key)
                .send()
                .await
                .map_err(|e| {
                    tracing::debug!(error = %e, "failed to delete source object after download");
                    CommandError::failure(format!("failed to delete source object: {e}"))
                        .with_source(e)
                })?;
            termoutln!(ctx.term, "move: {src_display} to {dst_display}")?;
            Ok(())
        }
        (TransferUri::S3(_), TransferUri::S3(_)) => {
            Err(CommandError::not_implemented("mv S3 to S3"))
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => Err(CommandError::param_validation(
            "usage: aws s3 mv <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type",
        )),
    }
}
