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

/// Guess the MIME type for a file based on its extension.
///
/// Returns `None` for unknown extensions (S3 will default to
/// `application/octet-stream`). Uses a compiled-in database via
/// `mime_guess2` — deterministic across platforms unlike Python's
/// `mimetypes` which reads system files.
pub(crate) fn guess_content_type(path: &Path) -> Option<String> {
    mime_guess2::from_path(path).first().map(|m| m.to_string())
}

/// Upload a single local file to S3.
#[tracing::instrument(skip(ctx), fields(%bucket, %key, source = %source.display()))]
pub(crate) async fn upload_single(
    ctx: &AppContext,
    source: &Path,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
) -> Result<(), CommandError> {
    let tm = build_tm(ctx);

    let stream = aws_sdk_s3_transfer_manager::io::InputStream::from_path(source).map_err(|e| {
        tracing::debug!(error = %e, source = %source.display(), "failed to open source file");
        CommandError::failure(format!("upload failed: {}", e)).with_source(e)
    })?;

    let mut upload = tm.upload().bucket(bucket).key(key).body(stream);

    if let Some(ct) = content_type {
        upload = upload.content_type(ct);
    }

    let handle = upload.initiate().map_err(|e| {
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

/// Recursively upload a local directory tree to an S3 prefix.
///
/// Each file is stored under `key_prefix` at its path relative to `source`.
/// Individual failures do not abort the operation; returns an error if any
/// file failed to transfer.
#[tracing::instrument(skip(ctx, args), fields(%bucket, %key_prefix, source = %source.display()))]
pub(crate) async fn upload_recursive(
    ctx: &AppContext,
    source: &Path,
    bucket: &str,
    key_prefix: &str,
    args: &crate::cli::TransferArgs,
) -> Result<(), CommandError> {
    use aws_sdk_s3_transfer_manager::types::FailedTransferPolicy;

    let tm = build_tm(ctx);
    let walker = crate::walk::build_fs_walker(args);

    // An empty key_prefix would key objects with a leading slash, since the
    // Transfer Manager joins `{prefix}{delimiter}{relative}`. Omit it so the
    // key is the bare relative path.
    let mut req = tm
        .upload_objects()
        .source(source)
        .bucket(bucket)
        .walker(walker)
        .failure_policy(FailedTransferPolicy::Continue);
    if !key_prefix.is_empty() {
        req = req.key_prefix(key_prefix);
    }

    let handle = req.initiate().map_err(|e| {
        tracing::debug!(error = %e, "failed to initiate recursive upload");
        CommandError::failure(format!("upload failed: {e}")).with_source(e)
    })?;

    let output = handle.join().await.map_err(|e| {
        tracing::debug!(error = %e, "recursive upload failed");
        CommandError::failure(format!("upload failed: {e}")).with_source(e)
    })?;

    let failed = output.failed_transfers();
    if !failed.is_empty() {
        tracing::debug!(count = failed.len(), "recursive upload had per-object failures");
        return Err(CommandError::failure(format!(
            "upload failed: {} of the requested object(s) failed to transfer",
            failed.len()
        )));
    }

    Ok(())
}

/// Recursively download an S3 prefix to a local directory.
///
/// Each object under `key_prefix` is written to `dest` at its path relative
/// to the prefix. The destination directory is created if it does not exist.
/// Individual failures do not abort the operation; returns an error if any
/// file failed to transfer.
#[tracing::instrument(skip(ctx), fields(%bucket, %key_prefix, dest = %dest.display()))]
pub(crate) async fn download_recursive(
    ctx: &AppContext,
    bucket: &str,
    key_prefix: &str,
    dest: &Path,
) -> Result<(), CommandError> {
    use aws_sdk_s3_transfer_manager::types::FailedTransferPolicy;

    let tm = build_tm(ctx);

    // download_objects requires an existing destination directory.
    std::fs::create_dir_all(dest).map_err(|e| {
        tracing::debug!(error = %e, dest = %dest.display(), "failed to create download destination directory");
        CommandError::failure(format!("download failed: {e}")).with_source(e)
    })?;

    // See upload_recursive: an empty key_prefix is omitted to avoid a
    // leading-slash key on the listing.
    let mut req = tm
        .download_objects()
        .bucket(bucket)
        .destination(dest)
        .failure_policy(FailedTransferPolicy::Continue);
    if !key_prefix.is_empty() {
        req = req.key_prefix(key_prefix);
    }

    let handle = req.initiate().map_err(|e| {
        tracing::debug!(error = %e, "failed to initiate recursive download");
        CommandError::failure(format!("download failed: {e}")).with_source(e)
    })?;

    let output = handle.join().await.map_err(|e| {
        tracing::debug!(error = %e, "recursive download failed");
        CommandError::failure(format!("download failed: {e}")).with_source(e)
    })?;

    let failed = output.failed_transfers();
    if !failed.is_empty() {
        tracing::debug!(count = failed.len(), "recursive download had per-object failures");
        return Err(CommandError::failure(format!(
            "download failed: {} of the requested object(s) failed to transfer",
            failed.len()
        )));
    }

    Ok(())
}
