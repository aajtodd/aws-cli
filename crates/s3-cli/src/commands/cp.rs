use crate::cli::CpArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::paths::format_local_path;
use crate::termoutln;
use crate::transfer;
use crate::uri::TransferUri;

/// Resolve the content-type for an upload: explicit flag > guess > None.
pub(super) fn resolve_content_type(
    source: &std::path::Path,
    explicit: Option<&str>,
    no_guess: bool,
) -> Option<String> {
    if let Some(ct) = explicit {
        return Some(ct.to_string());
    }
    if no_guess {
        return None;
    }
    transfer::guess_content_type(source)
}

/// Run the `cp` command.
#[tracing::instrument(skip(ctx), fields(source = ?args.source, dest = ?args.dest, recursive = args.recursive))]
pub async fn run(args: CpArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    if args.recursive {
        return Err(CommandError::not_implemented("cp --recursive"));
    }
    if args.transfer.dryrun {
        return Err(CommandError::not_implemented("cp --dryrun"));
    }

    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            let src_display = format_local_path(src);
            let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
            let ct = resolve_content_type(
                src,
                args.transfer.content_type.as_deref(),
                args.transfer.no_guess_mime_type,
            );
            transfer::upload_single(ctx, src, &dest.bucket, &dest.key, ct.as_deref()).await?;
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
            Err(CommandError::not_implemented("cp S3 to S3"))
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => Err(CommandError::param_validation(
            "usage: aws s3 cp <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type",
        )),
    }
}
