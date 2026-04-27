use aws_smithy_types::error::metadata::ProvideErrorMetadata;

use crate::cli::RmArgs;
use crate::context::AppContext;
use crate::error::{format_sdk_error, Error, Result};
use crate::uri::TransferUri;
use crate::{termerrln, termoutln};

/// Run the `rm` command.
pub async fn run(args: RmArgs, ctx: &AppContext) -> Result<i32> {
    let uri = match &args.path {
        TransferUri::S3(uri) => uri,
        TransferUri::Local(_) => {
            termerrln!(
                ctx.term,
                "\nusage: aws s3 rm <S3Uri>\nError: Invalid argument type"
            )?;
            return Ok(252);
        }
    };

    if args.recursive {
        delete_recursive(ctx, &uri.bucket, &uri.key, &args).await
    } else {
        delete_single(ctx, &uri.bucket, &uri.key, &args).await
    }
}

async fn delete_single(ctx: &AppContext, bucket: &str, key: &str, args: &RmArgs) -> Result<i32> {
    let path = format!("s3://{bucket}/{key}");

    if args.dryrun {
        termoutln!(ctx.term, "(dryrun) delete: {path}")?;
        return Ok(0);
    }

    let mut builder = ctx.client.delete_object().bucket(bucket).key(key);
    if let Some(ref payer) = args.request_payer {
        builder = builder.request_payer(payer.as_str().into());
    }

    match builder.send().await {
        Ok(_) => {
            if !args.quiet && !args.only_show_errors {
                termoutln!(ctx.term, "delete: {path}")?;
            }
            Ok(0)
        }
        Err(ref e) => {
            let code = e.code().unwrap_or("Unknown");
            let msg = e.message().unwrap_or("Unknown error");
            termerrln!(
                ctx.term,
                "delete failed: {path} An error occurred ({code}) when calling the DeleteObject operation: {msg}"
            )?;
            Ok(1)
        }
    }
}

async fn delete_recursive(
    ctx: &AppContext,
    bucket: &str,
    prefix: &str,
    args: &RmArgs,
) -> Result<i32> {
    let mut builder = ctx.client.list_objects_v2().bucket(bucket).prefix(prefix);
    if let Some(ref payer) = args.request_payer {
        builder = builder.request_payer(payer.as_str().into());
    }
    let mut paginator = builder.into_paginator();
    if let Some(page_size) = args.page_size {
        paginator = paginator.page_size(page_size);
    }

    let mut failures = 0u64;
    let mut pages = paginator.send();

    while let Some(page) = pages
        .try_next()
        .await
        .map_err(|ref e| Error::SdkService(format_sdk_error(e, "ListObjectsV2")))?
    {
        for object in page.contents() {
            if let Some(key) = object.key() {
                let path = format!("s3://{bucket}/{key}");

                if args.dryrun {
                    termoutln!(ctx.term, "(dryrun) delete: {path}")?;
                    continue;
                }

                let mut del = ctx.client.delete_object().bucket(bucket).key(key);
                if let Some(ref payer) = args.request_payer {
                    del = del.request_payer(payer.as_str().into());
                }

                match del.send().await {
                    Ok(_) => {
                        if !args.quiet && !args.only_show_errors {
                            termoutln!(ctx.term, "delete: {path}")?;
                        }
                    }
                    Err(ref e) => {
                        let code = e.code().unwrap_or("Unknown");
                        let msg = e.message().unwrap_or("Unknown error");
                        termerrln!(
                            ctx.term,
                            "delete failed: {path} An error occurred ({code}) when calling the DeleteObject operation: {msg}"
                        )?;
                        failures += 1;
                    }
                }
            }
        }
    }

    if failures > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RmArgs;
    use crate::term::test_support::InMemoryTerminal;
    use crate::uri::{S3Uri, TransferUri};
    use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
    use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output;
    use aws_sdk_s3::types::Object;
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

    fn rm_args(bucket: &str, key: &str) -> RmArgs {
        RmArgs {
            path: TransferUri::S3(S3Uri {
                bucket: bucket.to_string(),
                key: key.to_string(),
            }),
            dryrun: false,
            quiet: false,
            recursive: false,
            only_show_errors: false,
            page_size: None,
            request_payer: None,
        }
    }

    // -- Ported from test_rm_command.py --

    #[tokio::test]
    async fn single_delete() {
        // Ported from test_operations_used.
        let rule = mock!(aws_sdk_s3::Client::delete_object)
            .match_requests(|req| req.bucket() == Some("bucket") && req.key() == Some("key.txt"))
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(rm_args("bucket", "key.txt"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "delete: s3://bucket/key.txt");
    }

    #[tokio::test]
    async fn dryrun_delete() {
        // Ported from test_dryrun_delete.
        // No API calls should be made.
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "key.txt");
        args.dryrun = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(
            term.stdout_contents(),
            "(dryrun) delete: s3://bucket/key.txt"
        );
    }

    #[tokio::test]
    async fn delete_with_request_payer() {
        // Ported from test_delete_with_request_payer.
        let rule = mock!(aws_sdk_s3::Client::delete_object)
            .match_requests(|req| {
                req.bucket() == Some("mybucket")
                    && req.key() == Some("mykey")
                    && req.request_payer() == Some(&aws_sdk_s3::types::RequestPayer::Requester)
            })
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("mybucket", "mykey");
        args.request_payer = Some("requester".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "delete: s3://mybucket/mykey");
    }

    #[tokio::test]
    async fn recursive_delete_with_request_payer() {
        // Ported from test_recursive_delete_with_requests.
        let list = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| {
                req.request_payer() == Some(&aws_sdk_s3::types::RequestPayer::Requester)
            })
            .then_output(|| {
                ListObjectsV2Output::builder()
                    .contents(Object::builder().key("mykey").build())
                    .build()
            });
        let del = mock!(aws_sdk_s3::Client::delete_object)
            .match_requests(|req| {
                req.bucket() == Some("mybucket")
                    && req.key() == Some("mykey")
                    && req.request_payer() == Some(&aws_sdk_s3::types::RequestPayer::Requester)
            })
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list, del]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("mybucket", "");
        args.recursive = true;
        args.request_payer = Some("requester".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "delete: s3://mybucket/mykey");
    }

    // -- Additional tests --

    #[tokio::test]
    async fn recursive_dryrun() {
        // Recursive dryrun: lists objects, prints dryrun lines, no deletes.
        let list = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .contents(Object::builder().key("a.txt").build())
                .contents(Object::builder().key("b.txt").build())
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "");
        args.recursive = true;
        args.dryrun = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let output = term.stdout_contents();
        assert!(output.contains("(dryrun) delete: s3://bucket/a.txt"));
        assert!(output.contains("(dryrun) delete: s3://bucket/b.txt"));
    }

    #[tokio::test]
    async fn quiet_suppresses_output() {
        let rule = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "key.txt");
        args.quiet = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "");
    }

    #[tokio::test]
    async fn only_show_errors_suppresses_success() {
        let rule = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[rule]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "key.txt");
        args.only_show_errors = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        assert_eq!(term.stdout_contents(), "");
    }

    #[tokio::test]
    async fn recursive_delete_failure_returns_1() {
        // One object fails to delete → rc=1, but other deletes continue.
        let list = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .contents(Object::builder().key("good.txt").build())
                .contents(Object::builder().key("bad.txt").build())
                .build()
        });
        let del_ok = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let del_err = mock!(aws_sdk_s3::Client::delete_object).then_error(|| {
            aws_sdk_s3::operation::delete_object::DeleteObjectError::generic(
                aws_smithy_types::error::ErrorMetadata::builder()
                    .code("AccessDenied")
                    .message("Access Denied")
                    .build(),
            )
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list, del_ok, del_err]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "");
        args.recursive = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 1);
        assert!(term
            .stdout_contents()
            .contains("delete: s3://bucket/good.txt"));
        assert!(term
            .stderr_contents()
            .contains("delete failed: s3://bucket/bad.txt"));
    }

    #[tokio::test]
    async fn recursive_multi_page() {
        // 2 pages, 2 objects each = 4 deletes.
        let page1 = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .contents(Object::builder().key("a.txt").build())
                .contents(Object::builder().key("b.txt").build())
                .next_continuation_token("tok")
                .build()
        });
        let del1 = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let del2 = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let page2 = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .contents(Object::builder().key("c.txt").build())
                .contents(Object::builder().key("d.txt").build())
                .build()
        });
        let del3 = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let del4 = mock!(aws_sdk_s3::Client::delete_object)
            .then_output(|| DeleteObjectOutput::builder().build());
        let client = mock_client!(
            aws_sdk_s3,
            RuleMode::Sequential,
            &[page1, del1, del2, page2, del3, del4]
        );
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = rm_args("bucket", "");
        args.recursive = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let output = term.stdout_contents();
        assert_eq!(
            output.lines().count(),
            4,
            "expected 4 delete lines: {output}"
        );
        assert!(output.contains("a.txt"));
        assert!(output.contains("d.txt"));
    }

    #[tokio::test]
    async fn invalid_path_returns_252() {
        // rm /local/path → exit 252, matches Python: "usage: aws s3 rm <S3Uri>\nError: Invalid argument type"
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[]);
        let (ctx, term) = test_ctx_with_client(client);

        let args = RmArgs {
            path: TransferUri::Local(std::path::PathBuf::from("/local/path")),
            dryrun: false,
            quiet: false,
            recursive: false,
            only_show_errors: false,
            page_size: None,
            request_payer: None,
        };
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 252);
        let stderr = term.stderr_contents();
        assert!(stderr.contains("usage: aws s3 rm <S3Uri>"), "got: {stderr}");
        assert!(
            stderr.contains("Error: Invalid argument type"),
            "got: {stderr}"
        );
    }
}
