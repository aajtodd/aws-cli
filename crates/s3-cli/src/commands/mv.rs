use std::path::Path;

use crate::cli::MvArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::paths::format_local_path;
use crate::termoutln;
use crate::transfer;
use crate::uri::{S3Uri, TransferUri};
use crate::walk;

/// Run the `mv` command.
#[tracing::instrument(skip(ctx), fields(source = ?args.source, dest = ?args.dest, recursive = args.recursive))]
pub async fn run(args: MvArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    if args.transfer.dryrun {
        return dryrun(&args, ctx).await;
    }

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

/// Dryrun: print what would be moved without transferring or deleting.
async fn dryrun(args: &MvArgs, ctx: &AppContext) -> Result<(), CommandError> {
    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            if args.recursive {
                dryrun_recursive_local_to_s3(src, dest, args, ctx).await
            } else {
                let src_display = format_local_path(src);
                let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
                termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::S3(src), TransferUri::Local(dest)) => {
            if args.recursive {
                dryrun_recursive_s3_to_local(src, dest, args, ctx).await
            } else {
                let src_display = format!("s3://{}/{}", src.bucket, src.key);
                let dst_display = format_local_path(dest);
                termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::S3(src), TransferUri::S3(dest)) => {
            if args.recursive {
                dryrun_recursive_s3_to_s3(src, dest, args, ctx).await
            } else {
                let src_display = format!("s3://{}/{}", src.bucket, src.key);
                let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
                termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => Err(CommandError::param_validation(
            "usage: aws s3 mv <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type",
        )),
    }
}

async fn dryrun_recursive_local_to_s3(
    src: &Path,
    dest: &S3Uri,
    args: &MvArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_fs_walker(&args.transfer);
    let walk_ctx = walk::build_fs_walk_context(src);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let entry = result.map_err(|e| CommandError::failure(format!("walk error: {}", e)))?;
        let rel = entry.relative_path();
        let key = format!(
            "{}{}",
            dest.key,
            rel.to_str().unwrap_or_default().replace('\\', "/")
        );
        let src_display = format_local_path(entry.path());
        let dst_display = format!("s3://{}/{}", dest.bucket, key);
        termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
    }
    Ok(())
}

async fn dryrun_recursive_s3_to_local(
    src: &S3Uri,
    dest: &Path,
    args: &MvArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_s3_walker(Some(&src.key), args.transfer.page_size);
    let s3_client = aws_sdk_s3::Client::from_conf(ctx.s3_config_builder().build());
    let walk_ctx = walk::build_s3_walk_context(s3_client, &src.bucket);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let obj = result.map_err(|e| CommandError::failure(format!("list error: {}", e)))?;
        let key = obj.key().unwrap_or_default();
        let rel = key.strip_prefix(&src.key).unwrap_or(key);
        let local_path = dest.join(rel);
        let src_display = format!("s3://{}/{}", src.bucket, key);
        let dst_display = format_local_path(&local_path);
        termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
    }
    Ok(())
}

async fn dryrun_recursive_s3_to_s3(
    src: &S3Uri,
    dest: &S3Uri,
    args: &MvArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_s3_walker(Some(&src.key), args.transfer.page_size);
    let s3_client = aws_sdk_s3::Client::from_conf(ctx.s3_config_builder().build());
    let walk_ctx = walk::build_s3_walk_context(s3_client, &src.bucket);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let obj = result.map_err(|e| CommandError::failure(format!("list error: {}", e)))?;
        let key = obj.key().unwrap_or_default();
        let rel = key.strip_prefix(&src.key).unwrap_or(key);
        let dst_key = format!("{}{}", dest.key, rel);
        let src_display = format!("s3://{}/{}", src.bucket, key);
        let dst_display = format!("s3://{}/{}", dest.bucket, dst_key);
        termoutln!(ctx.term, "(dryrun) move: {src_display} to {dst_display}")?;
    }
    Ok(())
}
