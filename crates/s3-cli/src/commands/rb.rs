use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::cli::RbArgs;
use crate::context::AppContext;
use crate::error::Result;
use crate::exit_code;
use crate::uri::TransferUri;
use crate::{termerrln, termoutln};

/// Run the `rb` command.
#[tracing::instrument(skip(ctx), fields(path = ?args.path, force = args.force))]
pub async fn run(args: RbArgs, ctx: &AppContext) -> Result<i32> {
    let bucket = match &args.path {
        TransferUri::S3(uri) => {
            if !uri.key.is_empty() {
                termerrln!(
                    ctx.term,
                    "Please specify a valid bucket name only. E.g. s3://{}",
                    uri.bucket
                )?;
                return Ok(exit_code::PARAM_VALIDATION_ERROR);
            }
            &uri.bucket
        }
        TransferUri::Local(_) => {
            termerrln!(ctx.term, "<S3Uri>\nError: Invalid argument type")?;
            return Ok(exit_code::PARAM_VALIDATION_ERROR);
        }
    };

    if args.force {
        if let Err(msg) = force_delete_objects(ctx, bucket).await {
            termerrln!(ctx.term, "{msg}")?;
            return Ok(exit_code::GENERAL_ERROR);
        }
    }

    match ctx.client.delete_bucket().bucket(bucket).send().await {
        Ok(_) => {
            termoutln!(ctx.term, "remove_bucket: {bucket}")?;
            Ok(0)
        }
        Err(ref e) => {
            tracing::debug!(error = ?e, source = ?std::error::Error::source(e), "DeleteBucket failed");
            let code = e.code().unwrap_or("Unknown");
            let msg = e.message().unwrap_or("Unknown error");
            termerrln!(
                ctx.term,
                "remove_bucket failed: s3://{bucket} An error occurred ({code}) when calling the DeleteBucket operation: {msg}"
            )?;
            Ok(exit_code::FAILURE)
        }
    }
}

/// Delete all objects in a bucket (non-versioned).
async fn force_delete_objects(ctx: &AppContext, bucket: &str) -> std::result::Result<(), String> {
    let mut paginator = ctx
        .client
        .list_objects_v2()
        .bucket(bucket)
        .into_paginator()
        .send();

    while let Some(page) = paginator.try_next().await.map_err(|e| {
        tracing::debug!(error = ?e, source = ?std::error::Error::source(&e), "ListObjectsV2 failed during rb --force");
        "remove_bucket failed: Unable to delete all objects in the bucket, \
         bucket will not be deleted."
            .to_string()
    })? {
        for object in page.contents() {
            if let Some(key) = object.key() {
                ctx.client
                    .delete_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::debug!(error = ?e, source = ?std::error::Error::source(&e), %key, "DeleteObject failed during rb --force");
                        "remove_bucket failed: Unable to delete all objects in the bucket, \
                         bucket will not be deleted."
                            .to_string()
                    })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RbArgs;
    use crate::term::test_support::InMemoryTerminal;
    use crate::uri::{S3Uri, TransferUri};
    use aws_sdk_s3::operation::delete_bucket::DeleteBucketOutput;
    use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
    use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output;
    use aws_sdk_s3::types::Object;
    use aws_smithy_mocks::{mock, mock_client, RuleMode};
    use std::path::PathBuf;

    fn test_ctx_with_client(client: aws_sdk_s3::Client) -> (AppContext, InMemoryTerminal) {
        let term = InMemoryTerminal::new(24, 80);
        let ctx = AppContext {
            client,
            globals: crate::cli::GlobalArgs::default(),
            term: Box::new(term.clone()),
        };
        (ctx, term)
    }

    fn rb_args(bucket: &str, force: bool) -> RbArgs {
        RbArgs {
            path: TransferUri::S3(S3Uri {
                bucket: bucket.to_string(),
                key: String::new(),
            }),
            force,
        }
    }

    // -- Ported from test_rb_command.py --

    #[tokio::test]
    async fn remove_bucket_success() {
        // Ported from test_rb.
        let rule = mock!(aws_sdk_s3::Client::delete_bucket)
            .then_output(|| DeleteBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rb_args("bucket", false), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "remove_bucket: bucket");
    }

    #[tokio::test]
    async fn force_empty_bucket() {
        // Ported from test_rb_force_empty_bucket.
        // --force on empty bucket: ListObjectsV2 (empty) → DeleteBucket.
        let list = mock!(aws_sdk_s3::Client::list_objects_v2)
            .then_output(|| ListObjectsV2Output::builder().build());
        let delete = mock!(aws_sdk_s3::Client::delete_bucket)
            .then_output(|| DeleteBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list, delete]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rb_args("bucket", true), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "remove_bucket: bucket");
    }

    #[tokio::test]
    async fn force_non_empty_bucket() {
        // Ported from test_rb_force_non_empty_bucket.
        // --force: ListObjectsV2 (1 object) → DeleteObject → DeleteBucket.
        let list = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .contents(Object::builder().key("foo").size(100).build())
                .build()
        });
        let del_obj = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let del_bucket = mock!(aws_sdk_s3::Client::delete_bucket)
            .then_output(|| DeleteBucketOutput::builder().build());
        let client = mock_client!(
            aws_sdk_s3,
            RuleMode::Sequential,
            &[list, del_obj, del_bucket]
        );
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rb_args("bucket", true), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "remove_bucket: bucket");
    }

    #[tokio::test]
    async fn invalid_path_returns_252() {
        // Ported from test_nonzero_exit_if_uri_scheme_not_provided.
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let args = RbArgs {
            path: TransferUri::Local(PathBuf::from("bucket")),
            force: false,
        };
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 252);
        assert!(term.stderr_contents().contains("Invalid argument type"));
    }

    #[tokio::test]
    async fn key_provided_returns_252() {
        // Ported from test_nonzero_exit_if_key_provided.
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let args = RbArgs {
            path: TransferUri::S3(S3Uri {
                bucket: "bucket".to_string(),
                key: "key".to_string(),
            }),
            force: false,
        };
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 252);
        assert!(term
            .stderr_contents()
            .contains("Please specify a valid bucket name only"));
    }

    #[tokio::test]
    async fn key_with_force_returns_252() {
        // Also from test_nonzero_exit_if_key_provided (second case with --force).
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let args = RbArgs {
            path: TransferUri::S3(S3Uri {
                bucket: "bucket".to_string(),
                key: "key".to_string(),
            }),
            force: true,
        };
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 252);
        assert!(term
            .stderr_contents()
            .contains("Please specify a valid bucket name only"));
    }

    #[tokio::test]
    async fn delete_bucket_failure_returns_1() {
        // Ported from test_rb_failed_rc.
        let rule = mock!(aws_sdk_s3::Client::delete_bucket).then_error(|| {
            aws_sdk_s3::operation::delete_bucket::DeleteBucketError::generic(
                aws_smithy_types::error::ErrorMetadata::builder()
                    .code("BucketNotEmpty")
                    .message("The bucket you tried to delete is not empty")
                    .build(),
            )
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rb_args("bucket", false), &ctx).await.unwrap();

        assert_eq!(rc, 1);
        assert!(term.stderr_contents().contains("remove_bucket failed:"));
    }

    #[tokio::test]
    async fn force_with_failed_list_returns_255() {
        // Ported from test_rb_force_with_failed_rm.
        // --force but ListObjectsV2 fails → 255, bucket NOT deleted.
        let rule = mock!(aws_sdk_s3::Client::list_objects_v2).then_error(|| {
            aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error::generic(
                aws_smithy_types::error::ErrorMetadata::builder()
                    .code("AccessDenied")
                    .message("Access Denied")
                    .build(),
            )
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rb_args("bucket", true), &ctx).await.unwrap();

        assert_eq!(rc, 255);
        assert!(term.stderr_contents().contains("remove_bucket failed:"));
    }
}
