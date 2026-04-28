use std::path::Path;

use aws_sdk_s3_transfer_manager::types::{ConcurrencyMode, PartSize};

use crate::cli::CpArgs;
use crate::context::AppContext;
use crate::error::Result;
use crate::exit_code;
use crate::paths::format_local_path;
use crate::uri::TransferUri;
use crate::{termerrln, termoutln};

/// Run the `cp` command.
#[tracing::instrument(skip(ctx), fields(source = ?args.source, dest = ?args.dest, recursive = args.recursive))]
pub async fn run(args: CpArgs, ctx: &AppContext) -> Result<i32> {
    if args.recursive {
        unimplemented!("cp --recursive not yet implemented");
    }

    match (&args.source, &args.dest) {
        (TransferUri::Local(src), TransferUri::S3(dest)) => {
            let src_display = format_local_path(src);
            let dst_display = format_s3(&dest.bucket, &dest.key);
            upload_single(ctx, src, &dest.bucket, &dest.key, src_display, dst_display).await
        }
        (TransferUri::S3(src), TransferUri::Local(dest)) => {
            let src_display = format_s3(&src.bucket, &src.key);
            let dst_display = format_local_path(dest);
            download_single(ctx, &src.bucket, &src.key, dest, src_display, dst_display).await
        }
        (TransferUri::S3(_), TransferUri::S3(_)) => {
            unimplemented!("S3 to S3 cp not yet implemented");
        }
        (TransferUri::Local(_), TransferUri::Local(_)) => {
            termerrln!(
                ctx.term,
                "usage: aws s3 cp <LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>\nError: Invalid argument type"
            )?;
            Ok(exit_code::PARAM_VALIDATION_ERROR)
        }
    }
}

#[tracing::instrument(skip(ctx), fields(%bucket, %key, source = %source.display()))]
async fn upload_single(
    ctx: &AppContext,
    source: &Path,
    bucket: &str,
    key: &str,
    src_display: String,
    dst_display: String,
) -> Result<i32> {
    let tm = build_tm(ctx);

    let stream = match aws_sdk_s3_transfer_manager::io::InputStream::from_path(source) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, source = %source.display(), "failed to open source file for upload");
            termerrln!(
                ctx.term,
                "upload failed: {src_display} to {dst_display} {e}"
            )?;
            return Ok(exit_code::FAILURE);
        }
    };

    let handle = match tm.upload().bucket(bucket).key(key).body(stream).initiate() {
        Ok(h) => h,
        Err(e) => {
            tracing::debug!(error = %e, "failed to initiate upload");
            termerrln!(
                ctx.term,
                "upload failed: {src_display} to {dst_display} {e}"
            )?;
            return Ok(exit_code::FAILURE);
        }
    };

    match handle.join().await {
        Ok(_) => {
            tracing::debug!("upload completed successfully");
            termoutln!(ctx.term, "upload: {src_display} to {dst_display}")?;
            Ok(0)
        }
        Err(e) => {
            tracing::debug!(error = %e, "upload failed during transfer");
            termerrln!(
                ctx.term,
                "upload failed: {src_display} to {dst_display} {e}"
            )?;
            Ok(exit_code::FAILURE)
        }
    }
}

#[tracing::instrument(skip(ctx), fields(%bucket, %key, dest = %dest.display()))]
async fn download_single(
    ctx: &AppContext,
    bucket: &str,
    key: &str,
    dest: &Path,
    src_display: String,
    dst_display: String,
) -> Result<i32> {
    let tm = build_tm(ctx);

    let handle = match tm
        .download()
        .bucket(bucket)
        .key(key)
        .write_to_path(dest)
        .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::debug!(error = %e, "failed to initiate download");
            termerrln!(
                ctx.term,
                "download failed: {src_display} to {dst_display} {e}"
            )?;
            return Ok(exit_code::FAILURE);
        }
    };

    match handle.join().await {
        Ok(_) => {
            tracing::debug!("download completed successfully");
            termoutln!(ctx.term, "download: {src_display} to {dst_display}")?;
            Ok(0)
        }
        Err(e) => {
            tracing::debug!(error = %e, "download failed during transfer");
            termerrln!(
                ctx.term,
                "download failed: {src_display} to {dst_display} {e}"
            )?;
            Ok(exit_code::FAILURE)
        }
    }
}

fn build_tm(ctx: &AppContext) -> aws_sdk_s3_transfer_manager::Client {
    let config = aws_sdk_s3_transfer_manager::Config::builder()
        .client(ctx.client.clone())
        .concurrency(ConcurrencyMode::default())
        .part_size(PartSize::Auto)
        .build();
    aws_sdk_s3_transfer_manager::Client::new(config)
}

fn format_s3(bucket: &str, key: &str) -> String {
    if key.is_empty() {
        format!("s3://{bucket}")
    } else {
        format!("s3://{bucket}/{key}")
    }
}
