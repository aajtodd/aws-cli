use std::path::Path;

use crate::cli::CpArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::paths::format_local_path;
use crate::termoutln;
use crate::transfer;
use crate::uri::{S3Uri, TransferUri};
use crate::walk;

/// Resolve the content-type for an upload: explicit flag > guess > None.
pub(super) fn resolve_content_type(
    source: &Path,
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
    if args.transfer.dryrun {
        return dryrun(&args, ctx).await;
    }

    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            if args.recursive {
                return transfer::upload_recursive(
                    ctx,
                    src,
                    &dest.bucket,
                    &dest.key,
                    &args.transfer,
                )
                .await;
            }
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
            if args.recursive {
                return transfer::download_recursive(ctx, &src.bucket, &src.key, dest).await;
            }
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

/// Dryrun: enumerate what would be transferred without making API calls (for uploads)
/// or listing calls only (for downloads/copies).
async fn dryrun(args: &CpArgs, ctx: &AppContext) -> Result<(), CommandError> {
    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            if args.recursive {
                dryrun_upload_recursive(src, dest, args, ctx).await
            } else {
                let src_display = format_local_path(src);
                let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
                termoutln!(ctx.term, "(dryrun) upload: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::S3(src), TransferUri::Local(dest)) => {
            if args.recursive {
                dryrun_download_recursive(src, dest, args, ctx).await
            } else {
                let src_display = format!("s3://{}/{}", src.bucket, src.key);
                let dst_display = format_local_path(dest);
                termoutln!(ctx.term, "(dryrun) download: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::S3(src), TransferUri::S3(dest)) => {
            if args.recursive {
                dryrun_copy_recursive(src, dest, args, ctx).await
            } else {
                let src_display = format!("s3://{}/{}", src.bucket, src.key);
                let dst_display = format!("s3://{}/{}", dest.bucket, dest.key);
                termoutln!(ctx.term, "(dryrun) copy: {src_display} to {dst_display}")?;
                Ok(())
            }
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => Err(CommandError::param_validation(
            "usage: aws s3 cp <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type",
        )),
    }
}

/// Dryrun recursive upload: walk local dir, print each file.
async fn dryrun_upload_recursive(
    src: &Path,
    dest: &S3Uri,
    args: &CpArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_fs_walker(&args.transfer);
    let walk_ctx = walk::build_fs_walk_context(src);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let entry = result.map_err(|e| {
            CommandError::failure(format!("walk error: {}", e))
        })?;
        let rel = entry.relative_path();
        let key = format!(
            "{}{}",
            dest.key,
            rel.to_str().unwrap_or_default().replace('\\', "/")
        );
        let src_display = format_local_path(entry.path());
        let dst_display = format!("s3://{}/{}", dest.bucket, key);
        termoutln!(ctx.term, "(dryrun) upload: {src_display} to {dst_display}")?;
    }
    Ok(())
}

/// Dryrun recursive download: walk S3 prefix, print each object.
async fn dryrun_download_recursive(
    src: &S3Uri,
    dest: &Path,
    args: &CpArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_s3_walker(Some(&src.key), args.transfer.page_size);
    let s3_client = aws_sdk_s3::Client::from_conf(ctx.s3_config_builder().build());
    let walk_ctx = walk::build_s3_walk_context(s3_client, &src.bucket);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let obj = result.map_err(|e| {
            CommandError::failure(format!("list error: {}", e))
        })?;
        let key = obj.key().unwrap_or_default();
        let rel = key.strip_prefix(&src.key).unwrap_or(key);
        let local_path = dest.join(rel);
        let src_display = format!("s3://{}/{}", src.bucket, key);
        let dst_display = format_local_path(&local_path);
        termoutln!(ctx.term, "(dryrun) download: {src_display} to {dst_display}")?;
    }
    Ok(())
}

/// Dryrun recursive copy: walk source S3 prefix, print each object.
async fn dryrun_copy_recursive(
    src: &S3Uri,
    dest: &S3Uri,
    args: &CpArgs,
    ctx: &AppContext,
) -> Result<(), CommandError> {
    let walker = walk::build_s3_walker(Some(&src.key), args.transfer.page_size);
    let s3_client = aws_sdk_s3::Client::from_conf(ctx.s3_config_builder().build());
    let walk_ctx = walk::build_s3_walk_context(s3_client, &src.bucket);
    let mut walk = walker.walk(walk_ctx);

    while let Some(result) = walk.next().await {
        let obj = result.map_err(|e| {
            CommandError::failure(format!("list error: {}", e))
        })?;
        let key = obj.key().unwrap_or_default();
        let rel = key.strip_prefix(&src.key).unwrap_or(key);
        let dst_key = format!("{}{}", dest.key, rel);
        let src_display = format!("s3://{}/{}", src.bucket, key);
        let dst_display = format!("s3://{}/{}", dest.bucket, dst_key);
        termoutln!(ctx.term, "(dryrun) copy: {src_display} to {dst_display}")?;
    }
    Ok(())
}
