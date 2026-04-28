use aws_sdk_s3::types::{ErrorDocument, IndexDocument, WebsiteConfiguration};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::cli::WebsiteArgs;
use crate::context::AppContext;
use crate::error::Result;
use crate::exit_code;
use crate::termerrln;

/// Run the `website` command.
#[tracing::instrument(skip(ctx), fields(path = %args.path))]
pub async fn run(args: WebsiteArgs, ctx: &AppContext) -> Result<i32> {
    // Python's _get_bucket_name strips s3:// prefix and trailing slash.
    let bucket = args.path.strip_prefix("s3://").unwrap_or(&args.path);
    let bucket = bucket.strip_suffix('/').unwrap_or(bucket);

    if bucket.is_empty() {
        termerrln!(ctx.term, "<S3Uri>\nError: Invalid argument type")?;
        return Ok(exit_code::PARAM_VALIDATION_ERROR);
    }

    let mut config = WebsiteConfiguration::builder();
    if let Some(ref suffix) = args.index_document {
        config = config.index_document(
            IndexDocument::builder()
                .suffix(suffix)
                .build()
                .expect("suffix set"),
        );
    }
    if let Some(ref key) = args.error_document {
        config = config.error_document(ErrorDocument::builder().key(key).build().expect("key set"));
    }

    match ctx
        .client
        .put_bucket_website()
        .bucket(bucket)
        .website_configuration(config.build())
        .send()
        .await
    {
        Ok(_) => Ok(0),
        Err(ref e) => {
            tracing::debug!(error = ?e, source = ?std::error::Error::source(e), "PutBucketWebsite failed");
            let code = e.code().unwrap_or("Unknown");
            let msg = e.message().unwrap_or("Unknown error");
            termerrln!(
                ctx.term,
                "An error occurred ({code}) when calling the PutBucketWebsite operation: {msg}"
            )?;
            Ok(exit_code::FAILURE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::WebsiteArgs;
    use crate::term::test_support::InMemoryTerminal;
    use aws_sdk_s3::operation::put_bucket_website::PutBucketWebsiteOutput;
    use aws_smithy_mocks::{mock, mock_client, RuleMode};

    fn test_ctx_with_client(client: aws_sdk_s3::Client) -> (AppContext, InMemoryTerminal) {
        let term = InMemoryTerminal::new(24, 80);
        let ctx = AppContext {
            client,
            globals: crate::cli::GlobalArgs::default(),
            term: Box::new(term.clone()),
        };
        (ctx, term)
    }

    fn website_args(path: &str) -> WebsiteArgs {
        WebsiteArgs {
            path: path.to_string(),
            index_document: None,
            error_document: None,
        }
    }

    // -- Ported from test_website_command.py --

    #[tokio::test]
    async fn index_document() {
        // Ported from test_index_document.
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| {
                req.bucket() == Some("mybucket")
                    && req
                        .website_configuration()
                        .and_then(|w| w.index_document())
                        .map(|i| i.suffix() == "index.html")
                        .unwrap_or(false)
                    && req
                        .website_configuration()
                        .and_then(|w| w.error_document())
                        .is_none()
            })
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = website_args("s3://mybucket");
        args.index_document = Some("index.html".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        // Silent on success (matches Python).
        assert_eq!(term.stdout_contents(), "");
    }

    #[tokio::test]
    async fn error_document() {
        // Ported from test_error_document.
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| {
                req.bucket() == Some("mybucket")
                    && req
                        .website_configuration()
                        .and_then(|w| w.error_document())
                        .map(|e| e.key() == "mykey")
                        .unwrap_or(false)
                    && req
                        .website_configuration()
                        .and_then(|w| w.index_document())
                        .is_none()
            })
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, _) = test_ctx_with_client(client);

        let mut args = website_args("s3://mybucket");
        args.error_document = Some("mykey".to_string());
        assert_eq!(run(args, &ctx).await.unwrap(), 0);
    }

    // -- Additional coverage --

    #[tokio::test]
    async fn both_documents() {
        // Python supports both flags together — not tested by Python CLI.
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| {
                let w = req.website_configuration().unwrap();
                w.index_document().map(|i| i.suffix()) == Some("idx.html")
                    && w.error_document().map(|e| e.key()) == Some("err.html")
            })
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, _) = test_ctx_with_client(client);

        let mut args = website_args("s3://mybucket");
        args.index_document = Some("idx.html".to_string());
        args.error_document = Some("err.html".to_string());
        assert_eq!(run(args, &ctx).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn strips_trailing_slash() {
        // _get_bucket_name strips trailing slash.
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| req.bucket() == Some("mybucket"))
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, _) = test_ctx_with_client(client);

        assert_eq!(run(website_args("s3://mybucket/"), &ctx).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn accepts_bucket_without_s3_prefix() {
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| req.bucket() == Some("mybucket"))
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, _) = test_ctx_with_client(client);

        assert_eq!(run(website_args("mybucket"), &ctx).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn empty_configuration_sends_empty_config() {
        // Neither --index-document nor --error-document — sends empty config.
        let rule = mock!(aws_sdk_s3::Client::put_bucket_website)
            .match_requests(|req| {
                let w = req.website_configuration().unwrap();
                w.index_document().is_none() && w.error_document().is_none()
            })
            .then_output(|| PutBucketWebsiteOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, _) = test_ctx_with_client(client);

        assert_eq!(run(website_args("s3://mybucket"), &ctx).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn empty_path_returns_252() {
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(website_args("s3://"), &ctx).await.unwrap();

        assert_eq!(rc, 252);
        assert!(term.stderr_contents().contains("Invalid argument type"));
    }
}
