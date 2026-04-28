use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::cli::PresignArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::termoutln;

/// Run the `presign` command.
#[tracing::instrument(skip(ctx), fields(path = %args.path, expires_in = args.expires_in))]
pub async fn run(args: PresignArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    // Python CLI accepts both "s3://bucket/key" and "bucket/key" forms.
    let path = args.path.strip_prefix("s3://").unwrap_or(&args.path);
    let (bucket, key) = match path.split_once('/') {
        Some((b, k)) if !b.is_empty() && !k.is_empty() => (b, k),
        _ => {
            return Err(CommandError::param_validation(
                "<S3Uri>\nError: Invalid argument type",
            ));
        }
    };

    let config = match PresigningConfig::expires_in(Duration::from_secs(args.expires_in)) {
        Ok(c) => c,
        Err(e) => {
            return Err(CommandError::param_validation(format!(
                "Invalid --expires-in: {e}"
            )));
        }
    };

    match ctx
        .client
        .get_object()
        .bucket(bucket)
        .key(key)
        .presigned(config)
        .await
    {
        Ok(req) => {
            termoutln!(ctx.term, "{}", req.uri())?;
            Ok(())
        }
        Err(e) => {
            tracing::debug!(error = ?e, source = ?std::error::Error::source(&e), "GetObject presign failed");
            let code = e.code().unwrap_or("Unknown");
            let msg = e.message().unwrap_or("Unknown error");
            let message =
                format!("An error occurred ({code}) when calling the GetObject operation: {msg}");
            Err(CommandError::failure(message).with_source(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::PresignArgs;
    use crate::term::test_support::InMemoryTerminal;
    use aws_credential_types::Credentials;
    use aws_smithy_async::time::StaticTimeSource;

    fn test_ctx_fixed_time() -> (AppContext, InMemoryTerminal) {
        // Fixed timestamp 2016-08-18T14:33:03Z = 1471530783 (use Python test's time).
        // Python uses FROZEN_TIMESTAMP=1471305652 but that's the time.time() value;
        // the actual signing time comes from datetime.datetime.utcnow() which the
        // test mocks to 2016-08-18 14:33:03 UTC. We use the same UTC time here.
        let time_source = StaticTimeSource::from_secs(1471530783);
        let config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(Credentials::for_tests())
            .time_source(time_source)
            .build();
        let client = aws_sdk_s3::Client::from_conf(config);
        // Wide terminal so presigned URLs don't wrap.
        let term = InMemoryTerminal::new(24, 2000);
        let ctx = AppContext {
            client,
            globals: crate::cli::GlobalArgs::default(),
            term: Box::new(term.clone()),
        };
        (ctx, term)
    }

    fn presign_args(path: &str, expires_in: u64) -> PresignArgs {
        PresignArgs {
            path: path.to_string(),
            expires_in,
        }
    }

    #[tokio::test]
    async fn generates_url() {
        // Ported from test_generates_a_url.
        let (ctx, term) = test_ctx_fixed_time();
        run(presign_args("s3://bucket/key", 3600), &ctx)
            .await
            .unwrap();

        let url = term.stdout_contents();
        assert!(
            url.starts_with("https://bucket.s3.us-east-1.amazonaws.com/key?"),
            "unexpected host/path: {url}"
        );
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Expires=3600"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("X-Amz-Date="));
        assert!(url.contains("X-Amz-Credential="));
        assert!(url.contains("X-Amz-Signature="));
    }

    #[tokio::test]
    async fn non_dns_compatible_bucket_falls_back_to_path_style() {
        // Ported from test_handles_non_dns_compatible_buckets.
        let (ctx, term) = test_ctx_fixed_time();
        run(presign_args("s3://bucket.dots/key", 3600), &ctx)
            .await
            .unwrap();

        let url = term.stdout_contents();
        // Bucket with dots cannot be virtual-hosted; SDK uses path-style.
        assert!(
            url.starts_with("https://s3.us-east-1.amazonaws.com/bucket.dots/key?"),
            "expected path-style: {url}"
        );
    }

    #[tokio::test]
    async fn custom_expires_in() {
        // Ported from test_handles_expires_in.
        let (ctx, term) = test_ctx_fixed_time();
        run(presign_args("s3://bucket/key", 1000), &ctx)
            .await
            .unwrap();

        assert!(term.stdout_contents().contains("X-Amz-Expires=1000"));
    }

    #[tokio::test]
    async fn s3_prefix_not_required() {
        // Ported from test_s3_prefix_not_needed.
        let (ctx, term) = test_ctx_fixed_time();
        run(presign_args("bucket/key", 3600), &ctx).await.unwrap();

        assert!(term
            .stdout_contents()
            .starts_with("https://bucket.s3.us-east-1.amazonaws.com/key?"));
    }

    #[tokio::test]
    async fn invalid_path_returns_252() {
        let (ctx, _term) = test_ctx_fixed_time();
        let err = run(presign_args("just-a-bucket", 3600), &ctx)
            .await
            .expect_err("expected error");

        assert_eq!(err.kind, crate::error::CommandErrorKind::ParamValidation);
        assert_eq!(err.exit_code(), 252);
        assert!(err.message.contains("Invalid argument type"));
    }

    #[tokio::test]
    async fn expires_in_too_long_returns_252() {
        // PresigningConfig rejects expires_in > ONE_WEEK (604800 seconds).
        let (ctx, _term) = test_ctx_fixed_time();
        let err = run(presign_args("s3://bucket/key", 604801), &ctx)
            .await
            .expect_err("expected error");

        assert_eq!(err.kind, crate::error::CommandErrorKind::ParamValidation);
        assert_eq!(err.exit_code(), 252);
        assert!(err.message.contains("Invalid --expires-in"));
    }
}
