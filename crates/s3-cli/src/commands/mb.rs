use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration, Tag};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::cli::MbArgs;
use crate::context::AppContext;
use crate::error::CommandError;
use crate::termoutln;
use crate::uri::TransferUri;

/// Run the `mb` command.
#[tracing::instrument(skip(ctx), fields(path = ?args.path))]
pub async fn run(args: MbArgs, ctx: &AppContext) -> std::result::Result<(), CommandError> {
    let bucket = match &args.path {
        TransferUri::S3(uri) => &uri.bucket,
        TransferUri::Local(_) => {
            return Err(CommandError::param_validation(
                "<S3Uri>\nError: Invalid argument type",
            ));
        }
    };

    if bucket.ends_with("--x-s3") {
        return Err(CommandError::param_validation(
            "Cannot use mb command with a directory bucket.",
        ));
    }

    let mut builder = ctx.client.create_bucket().bucket(bucket);

    if bucket.ends_with("-an") {
        builder = builder.bucket_namespace(aws_sdk_s3::types::BucketNamespace::AccountRegional);
    }

    let mut config = CreateBucketConfiguration::builder();
    let mut has_config = false;

    let region = ctx.client.config().region().map(|r| r.as_ref().to_string());
    if let Some(ref region) = region {
        if region != "us-east-1" {
            config = config.location_constraint(BucketLocationConstraint::from(region.as_str()));
            has_config = true;
        }
    }

    for pair in args.tags.chunks(2) {
        if pair.len() == 2 {
            config = config.tags(
                Tag::builder()
                    .key(&pair[0])
                    .value(&pair[1])
                    .build()
                    .expect("tag key and value are set"),
            );
            has_config = true;
        }
    }

    if has_config {
        builder = builder.create_bucket_configuration(config.build());
    }

    match builder.send().await {
        Ok(_) => {
            termoutln!(ctx.term, "make_bucket: {bucket}")?;
            Ok(())
        }
        Err(e) => {
            tracing::debug!(error = ?e, source = ?std::error::Error::source(&e), "CreateBucket failed");
            let code = e.code().unwrap_or("Unknown");
            let msg = e.message().unwrap_or("Unknown error");
            let message = format!(
                "make_bucket failed: s3://{bucket} An error occurred ({code}) when calling the CreateBucket operation: {msg}"
            );
            Err(CommandError::failure(message).with_source(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::MbArgs;
    use crate::term::test_support::InMemoryTerminal;
    use crate::uri::{S3Uri, TransferUri};
    use aws_sdk_s3::operation::create_bucket::CreateBucketOutput;
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

    fn mb_args(bucket: &str) -> MbArgs {
        MbArgs {
            path: TransferUri::S3(S3Uri {
                bucket: bucket.to_string(),
                key: String::new(),
            }),
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn make_bucket_success() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(mb_args("mybucket"), &ctx).await;

        assert!(rc.is_ok(), "run failed: {:?}", rc.err());
        assert_eq!(term.stdout_contents(), "make_bucket: mybucket");
    }

    #[tokio::test]
    async fn adds_location_constraint() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                req.create_bucket_configuration()
                    .and_then(|c| c.location_constraint())
                    .map(|l| l.as_str() == "us-west-2")
                    .unwrap_or(false)
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-west-2"))
        });
        let (ctx, term) = test_ctx_with_client(client);

        run(mb_args("bucket"), &ctx).await.unwrap();
        assert_eq!(term.stdout_contents(), "make_bucket: bucket");
    }

    #[tokio::test]
    async fn no_location_constraint_for_us_east_1() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| req.create_bucket_configuration().is_none())
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-east-1"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        run(mb_args("bucket"), &ctx).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_path_returns_252() {
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, _term) = test_ctx_with_client(client);

        let args = MbArgs {
            path: TransferUri::Local(PathBuf::from("bucket")),
            tags: vec![],
        };
        let err = run(args, &ctx).await.expect_err("expected error");
        assert_eq!(err.kind, crate::error::CommandErrorKind::ParamValidation);
        assert_eq!(err.exit_code(), 252);
        assert!(err.message.contains("Invalid argument type"));
    }

    #[tokio::test]
    async fn rejects_s3_express_directory_bucket() {
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, _term) = test_ctx_with_client(client);

        let err = run(mb_args("bucket--usw2-az1--x-s3"), &ctx)
            .await
            .expect_err("expected error");
        assert_eq!(err.kind, crate::error::CommandErrorKind::ParamValidation);
        assert_eq!(err.exit_code(), 252);
        assert!(err
            .message
            .contains("Cannot use mb command with a directory bucket."));
    }

    #[tokio::test]
    async fn single_tag() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                let tags = req
                    .create_bucket_configuration()
                    .map(|c| c.tags())
                    .unwrap_or_default();
                tags.len() == 1 && tags[0].key() == "Key1" && tags[0].value() == "Value1"
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-west-2"))
        });
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = mb_args("bucket");
        args.tags = vec!["Key1".into(), "Value1".into()];
        run(args, &ctx).await.unwrap();
        assert_eq!(term.stdout_contents(), "make_bucket: bucket");
    }

    #[tokio::test]
    async fn multiple_tags() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                let tags = req
                    .create_bucket_configuration()
                    .map(|c| c.tags())
                    .unwrap_or_default();
                tags.len() == 2 && tags[0].key() == "Key1" && tags[1].key() == "Key2"
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-west-2"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        let mut args = mb_args("bucket");
        args.tags = vec![
            "Key1".into(),
            "Value1".into(),
            "Key2".into(),
            "Value2".into(),
        ];
        run(args, &ctx).await.unwrap();
    }

    #[tokio::test]
    async fn account_regional_namespace_bucket() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                req.bucket_namespace() == Some(&aws_sdk_s3::types::BucketNamespace::AccountRegional)
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-west-2"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        run(
            mb_args("amzn-s3-demo-bucket-111122223333-us-west-2-an"),
            &ctx,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn account_regional_namespace_us_east_1() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                req.bucket_namespace() == Some(&aws_sdk_s3::types::BucketNamespace::AccountRegional)
                    && req.create_bucket_configuration().is_none()
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-east-1"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        run(mb_args("my-bucket-111122223333-us-east-1-an"), &ctx)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn regular_bucket_no_namespace() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| req.bucket_namespace().is_none())
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-east-1"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        run(mb_args("my-regular-bucket"), &ctx).await.unwrap();
    }

    #[tokio::test]
    async fn short_an_bucket() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                req.bucket_namespace() == Some(&aws_sdk_s3::types::BucketNamespace::AccountRegional)
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-east-1"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        run(mb_args("xyz-an"), &ctx).await.unwrap();
    }

    #[tokio::test]
    async fn tags_us_east_1_no_location_constraint() {
        let rule = mock!(aws_sdk_s3::Client::create_bucket)
            .match_requests(|req| {
                let config = req.create_bucket_configuration();
                let has_tags = config.map(|c| !c.tags().is_empty()).unwrap_or(false);
                let no_location = config.and_then(|c| c.location_constraint()).is_none();
                has_tags && no_location
            })
            .then_output(|| CreateBucketOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule], |conf| {
            conf.region(aws_sdk_s3::config::Region::new("us-east-1"))
        });
        let (ctx, _) = test_ctx_with_client(client);

        let mut args = mb_args("bucket");
        args.tags = vec!["Key1".into(), "Value1".into()];
        run(args, &ctx).await.unwrap();
    }
}
