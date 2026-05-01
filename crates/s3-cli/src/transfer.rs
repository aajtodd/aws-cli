use std::path::Path;

use aws_sdk_s3_transfer_manager::types::{ConcurrencyMode, PartSize};

use crate::context::AppContext;
use crate::error::CommandError;

/// Build a Transfer Manager client from the app context.
pub(crate) fn build_tm(ctx: &AppContext) -> aws_sdk_s3_transfer_manager::Client {
    let s3_builder = ctx.s3_config_builder();
    let s3_config = aws_sdk_s3_transfer_manager::config::S3ClientConfig::new(s3_builder);

    let part_size = match ctx.s3_config_keys.multipart_chunksize {
        Some(bytes) => PartSize::Target(bytes),
        None => PartSize::Auto,
    };
    let multipart_threshold = match ctx.s3_config_keys.multipart_threshold {
        Some(bytes) => PartSize::Target(bytes),
        None => PartSize::Auto,
    };
    // TODO: target_bandwidth is exposed on TM config but not yet implemented.
    // When TM implements TargetThroughput, wire ctx.s3_config_keys.target_bandwidth here.
    let concurrency = ConcurrencyMode::default();

    let config = aws_sdk_s3_transfer_manager::Config::builder()
        .s3_config(s3_config)
        .concurrency(concurrency)
        .part_size(part_size)
        .multipart_threshold(multipart_threshold)
        .build();
    aws_sdk_s3_transfer_manager::Client::new(config)
}

/// Upload a single local file to S3.
#[tracing::instrument(skip(ctx), fields(%bucket, %key, source = %source.display()))]
pub(crate) async fn upload_single(
    ctx: &AppContext,
    source: &Path,
    bucket: &str,
    key: &str,
) -> Result<(), CommandError> {
    let tm = build_tm(ctx);

    let stream = aws_sdk_s3_transfer_manager::io::InputStream::from_path(source).map_err(|e| {
        tracing::debug!(error = %e, source = %source.display(), "failed to open source file");
        CommandError::failure(format!("upload failed: {}", e)).with_source(e)
    })?;

    let handle = tm
        .upload()
        .bucket(bucket)
        .key(key)
        .body(stream)
        .initiate()
        .map_err(|e| {
            tracing::debug!(error = %e, "failed to initiate upload");
            CommandError::failure(format!("upload failed: {}", e)).with_source(e)
        })?;

    handle.join().await.map_err(|e| {
        tracing::debug!(error = %e, "upload failed during transfer");
        CommandError::failure(format!("upload failed: {}", e)).with_source(e)
    })?;

    Ok(())
}

/// Download a single S3 object to a local file.
#[tracing::instrument(skip(ctx), fields(%bucket, %key, dest = %dest.display()))]
pub(crate) async fn download_single(
    ctx: &AppContext,
    bucket: &str,
    key: &str,
    dest: &Path,
) -> Result<(), CommandError> {
    let tm = build_tm(ctx);

    let handle = tm
        .download()
        .bucket(bucket)
        .key(key)
        .write_to_path(dest)
        .await
        .map_err(|e| {
            tracing::debug!(error = %e, "failed to initiate download");
            CommandError::failure(format!("download failed: {}", e)).with_source(e)
        })?;

    handle.join().await.map_err(|e| {
        tracing::debug!(error = %e, "download failed during transfer");
        CommandError::failure(format!("download failed: {}", e)).with_source(e)
    })?;

    Ok(())
}
