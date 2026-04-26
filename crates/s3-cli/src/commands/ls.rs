//! Implementation of `aws s3 ls`.

use crate::cli::LsArgs;
use crate::context::AppContext;
use crate::error::{format_sdk_error, Error, Result};
use crate::format::{format_datetime_local, format_size, human_readable_size};
use crate::termoutln;
use crate::uri::parse_s3_uri;

/// Run the `ls` command.
pub async fn run(args: LsArgs, ctx: &AppContext) -> Result<i32> {
    let uri = parse_s3_uri(&args.s3uri).ok_or_else(|| Error::InvalidUri(args.s3uri.clone()))?;

    let mut state = LsState::new(args.human_readable);

    if uri.bucket.is_empty() {
        list_buckets(ctx, &args, &mut state).await?;
    } else if args.recursive {
        list_objects_recursive(ctx, &uri.bucket, &uri.key, &args, &mut state).await?;
    } else {
        list_objects(ctx, &uri.bucket, &uri.key, &args, &mut state).await?;
    }

    if args.summarize {
        print_summary(ctx, &state)?;
    }

    if !uri.key.is_empty() && state.empty_result && state.at_first_page {
        return Ok(1);
    }

    Ok(0)
}

struct LsState {
    total_objects: u64,
    size_accumulator: u64,
    empty_result: bool,
    at_first_page: bool,
    human_readable: bool,
}

impl LsState {
    fn new(human_readable: bool) -> Self {
        Self {
            total_objects: 0,
            size_accumulator: 0,
            empty_result: false,
            at_first_page: true,
            human_readable,
        }
    }
}

async fn list_buckets(ctx: &AppContext, args: &LsArgs, state: &mut LsState) -> Result<()> {
    let mut builder = ctx.client.list_buckets();
    if let Some(ref prefix) = args.bucket_name_prefix {
        builder = builder.prefix(prefix);
    }
    if let Some(ref region) = args.bucket_region {
        builder = builder.bucket_region(region);
    }
    let mut paginator = builder.into_paginator();
    if let Some(page_size) = args.page_size {
        paginator = paginator.page_size(page_size);
    }

    let mut pages = paginator.send();
    while let Some(page) = pages
        .try_next()
        .await
        .map_err(|ref e| Error::SdkService(format_sdk_error(e, "ListBuckets")))?
    {
        for bucket in page.buckets() {
            let date_str = format_datetime_local(bucket.creation_date());
            let name = bucket.name().unwrap_or("");
            termoutln!(ctx.term, "{date_str} {name}")?;
        }
        state.at_first_page = false;
    }
    Ok(())
}

async fn list_objects(
    ctx: &AppContext,
    bucket: &str,
    prefix: &str,
    args: &LsArgs,
    state: &mut LsState,
) -> Result<()> {
    let mut builder = ctx
        .client
        .list_objects_v2()
        .bucket(bucket)
        .prefix(prefix)
        .delimiter("/");
    if let Some(ref payer) = args.request_payer {
        builder = builder.request_payer(payer.as_str().into());
    }
    let mut paginator = builder.into_paginator();
    if let Some(page_size) = args.page_size {
        paginator = paginator.page_size(page_size);
    }

    let mut pages = paginator.send();
    while let Some(page) = pages
        .try_next()
        .await
        .map_err(|ref e| Error::SdkService(format_sdk_error(e, "ListObjectsV2")))?
    {
        display_page(ctx, page.common_prefixes(), page.contents(), true, state)?;
    }
    Ok(())
}

async fn list_objects_recursive(
    ctx: &AppContext,
    bucket: &str,
    prefix: &str,
    args: &LsArgs,
    state: &mut LsState,
) -> Result<()> {
    let mut builder = ctx.client.list_objects_v2().bucket(bucket).prefix(prefix);
    if let Some(ref payer) = args.request_payer {
        builder = builder.request_payer(payer.as_str().into());
    }
    let mut paginator = builder.into_paginator();
    if let Some(page_size) = args.page_size {
        paginator = paginator.page_size(page_size);
    }

    let mut pages = paginator.send();
    while let Some(page) = pages
        .try_next()
        .await
        .map_err(|ref e| Error::SdkService(format_sdk_error(e, "ListObjectsV2")))?
    {
        display_page(ctx, page.common_prefixes(), page.contents(), false, state)?;
    }
    Ok(())
}

fn display_page(
    ctx: &AppContext,
    common_prefixes: &[aws_sdk_s3::types::CommonPrefix],
    contents: &[aws_sdk_s3::types::Object],
    use_basename: bool,
    state: &mut LsState,
) -> Result<()> {
    if common_prefixes.is_empty() && contents.is_empty() {
        state.empty_result = true;
        return Ok(());
    }

    for cp in common_prefixes {
        if let Some(prefix) = cp.prefix() {
            let components: Vec<&str> = prefix.split('/').collect();
            let dir_name = if components.len() >= 2 {
                components[components.len() - 2]
            } else {
                prefix
            };
            termoutln!(ctx.term, "{:>30} {dir_name}/", "PRE")?;
        }
    }

    for object in contents {
        let date_str = format_datetime_local(object.last_modified());
        let size = object.size().unwrap_or(0);
        state.size_accumulator += size as u64;
        state.total_objects += 1;
        let size_str = format_size(size, state.human_readable);

        let name = if use_basename {
            object.key().unwrap_or("").rsplit('/').next().unwrap_or("")
        } else {
            object.key().unwrap_or("")
        };

        termoutln!(ctx.term, "{date_str} {size_str} {name}")?;
    }

    state.at_first_page = false;
    Ok(())
}

fn print_summary(ctx: &AppContext, state: &LsState) -> Result<()> {
    let size_str = if state.human_readable {
        human_readable_size(state.size_accumulator)
    } else {
        state.size_accumulator.to_string()
    };
    termoutln!(ctx.term)?;
    termoutln!(ctx.term, "{:>15}{}", "Total Objects: ", state.total_objects)?;
    termoutln!(ctx.term, "{:>15}{}", "Total Size: ", size_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::test_support::InMemoryTerminal;
    use aws_sdk_s3::operation::list_buckets::ListBucketsOutput;
    use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output;
    use aws_sdk_s3::types::{Bucket, CommonPrefix, Object};
    use aws_smithy_mocks::{mock, mock_client, RuleMode};

    fn test_ctx_with_client(client: aws_sdk_s3::Client) -> (AppContext, InMemoryTerminal) {
        let term = InMemoryTerminal::new(24, 80);
        let term_handle = term.clone();
        let ctx = AppContext {
            client,
            globals: crate::cli::GlobalArgs::default(),
            term: Box::new(term),
        };
        (ctx, term_handle)
    }

    fn test_ctx() -> (AppContext, InMemoryTerminal) {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .build();
        test_ctx_with_client(aws_sdk_s3::Client::from_conf(config))
    }

    fn ls_args(s3uri: &str) -> LsArgs {
        LsArgs {
            s3uri: s3uri.to_string(),
            recursive: false,
            human_readable: false,
            summarize: false,
            page_size: None,
            request_payer: None,
            bucket_name_prefix: None,
            bucket_region: None,
        }
    }

    // -----------------------------------------------------------------------
    // run() with smithy-mocks — ported from test_ls_command.py
    // These test the full pipeline: args → SDK calls → output
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn run_list_objects_non_recursive() {
        // Ported from test_operations_used_in_recursive_list (non-recursive path).
        // Verifies: Bucket, Prefix="", Delimiter="/", output format.
        let time = aws_smithy_types::DateTime::from_secs(1389304549);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| {
                req.bucket() == Some("bucket")
                    && req.prefix() == Some("")
                    && req.delimiter() == Some("/")
            })
            .then_output(move || {
                ListObjectsV2Output::builder()
                    .contents(
                        Object::builder()
                            .key("bar.txt")
                            .size(100)
                            .last_modified(time)
                            .build(),
                    )
                    .build()
            });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let expected_date = format_datetime_local(Some(&time));
        assert_eq!(
            term.stdout_contents(),
            format!("{expected_date}        100 bar.txt")
        );
    }

    #[tokio::test]
    async fn run_list_objects_recursive_no_delimiter() {
        // Ported from test_operations_used_in_recursive_list.
        // Verifies: no Delimiter set, Prefix="", full key in output.
        let time = aws_smithy_types::DateTime::from_secs(1389304549);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| {
                req.bucket() == Some("bucket")
                    && req.prefix() == Some("")
                    && req.delimiter().is_none()
            })
            .then_output(move || {
                ListObjectsV2Output::builder()
                    .contents(
                        Object::builder()
                            .key("foo/bar.txt")
                            .size(100)
                            .last_modified(time)
                            .build(),
                    )
                    .build()
            });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.recursive = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let expected_date = format_datetime_local(Some(&time));
        assert_eq!(
            term.stdout_contents(),
            format!("{expected_date}        100 foo/bar.txt")
        );
    }

    #[tokio::test]
    async fn run_list_objects_with_prefix() {
        // ls s3://bucket/photos/ → Prefix="photos/", Delimiter="/"
        let time = aws_smithy_types::DateTime::from_secs(1389304549);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| {
                req.bucket() == Some("bucket")
                    && req.prefix() == Some("photos/")
                    && req.delimiter() == Some("/")
            })
            .then_output(move || {
                ListObjectsV2Output::builder()
                    .contents(
                        Object::builder()
                            .key("photos/cat.jpg")
                            .size(2048)
                            .last_modified(time)
                            .build(),
                    )
                    .build()
            });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/photos/"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let expected_date = format_datetime_local(Some(&time));
        assert_eq!(
            term.stdout_contents(),
            format!("{expected_date}       2048 cat.jpg")
        );
    }

    #[tokio::test]
    async fn run_list_objects_page_size() {
        // Ported from test_operations_use_page_size.
        // Verifies: --page-size translates to max_keys.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| req.max_keys() == Some(8))
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.page_size = Some(8);
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_objects_request_payer() {
        // Ported from test_requester_pays.
        // Verifies: --request-payer requester passes RequestPayer.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| {
                req.request_payer() == Some(&aws_sdk_s3::types::RequestPayer::Requester)
            })
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.request_payer = Some("requester".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_buckets() {
        // Ported from test_ls_command_with_no_args.
        // ls with no path → ListBuckets.
        let time = aws_smithy_types::DateTime::from_secs(1389304549);
        let list_buckets = mock!(aws_sdk_s3::Client::list_buckets).then_output(move || {
            ListBucketsOutput::builder()
                .buckets(
                    Bucket::builder()
                        .name("my-bucket")
                        .creation_date(time)
                        .build(),
                )
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_buckets]);
        let (ctx, term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let expected_date = format_datetime_local(Some(&time));
        assert_eq!(term.stdout_contents(), format!("{expected_date} my-bucket"));
    }

    #[tokio::test]
    async fn run_list_buckets_page_size() {
        // Ported from test_list_buckets_use_page_size.
        // Verifies: --page-size translates to max_buckets.
        let list_buckets = mock!(aws_sdk_s3::Client::list_buckets)
            .match_requests(|req| req.max_buckets() == Some(8))
            .then_output(|| ListBucketsOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_buckets]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://");
        args.page_size = Some(8);
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_buckets_with_prefix_filter() {
        // Ported from test_list_buckets_uses_bucket_name_prefix.
        let list_buckets = mock!(aws_sdk_s3::Client::list_buckets)
            .match_requests(|req| req.prefix() == Some("my-"))
            .then_output(|| ListBucketsOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_buckets]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://");
        args.bucket_name_prefix = Some("my-".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_buckets_with_region_filter() {
        // Ported from test_list_buckets_uses_bucket_region.
        let list_buckets = mock!(aws_sdk_s3::Client::list_buckets)
            .match_requests(|req| req.bucket_region() == Some("us-west-1"))
            .then_output(|| ListBucketsOutput::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_buckets]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://");
        args.bucket_region = Some("us-west-1".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_no_match_returns_1() {
        // Ported from test_fail_rc_no_objects_nor_prefixes.
        // ls s3://bucket/nonexistent with empty response → rc=1.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/nonexistent"), &ctx).await.unwrap();

        assert_eq!(rc, 1);
    }

    #[tokio::test]
    async fn run_empty_bucket_returns_0() {
        // Ported from test_success_rc_empty_bucket_no_key_given.
        // ls s3://bucket (no key) with empty response → rc=0.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_summarize() {
        // Ported from test_summarize.
        let time = aws_smithy_types::DateTime::from_secs(0);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(move || {
            ListObjectsV2Output::builder()
                .contents(
                    Object::builder()
                        .key("a")
                        .size(100)
                        .last_modified(time)
                        .build(),
                )
                .contents(
                    Object::builder()
                        .key("b")
                        .size(200)
                        .last_modified(time)
                        .build(),
                )
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.summarize = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let output = term.stdout_contents();
        assert!(output.contains("Total Objects: 2"), "got: {output}");
        assert!(output.contains("Total Size: 300"), "got: {output}");
    }

    #[tokio::test]
    async fn run_summarize_human_readable() {
        // Ported from test_summarize_with_human_readable.
        let time = aws_smithy_types::DateTime::from_secs(0);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(move || {
            ListObjectsV2Output::builder()
                .contents(
                    Object::builder()
                        .key("a")
                        .size(1024 * 1024)
                        .last_modified(time)
                        .build(),
                )
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.summarize = true;
        args.human_readable = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
        let output = term.stdout_contents();
        assert!(output.contains("Total Objects: 1"), "got: {output}");
        assert!(output.contains("Total Size: 1.0 MiB"), "got: {output}");
    }

    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn run_list_objects_recursive_page_size() {
        // Ported from test_operations_use_page_size_recursive.
        // Verifies: --page-size + --recursive sets max_keys, no delimiter.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| req.max_keys() == Some(8) && req.delimiter().is_none())
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://bucket/");
        args.page_size = Some(8);
        args.recursive = true;
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_mixed_prefixes_and_objects_returns_0() {
        // Ported from test_success_rc_has_prefixes_and_objects.
        // Both CommonPrefixes and Contents present → rc=0.
        let time = aws_smithy_types::DateTime::from_secs(0);
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(move || {
            ListObjectsV2Output::builder()
                .common_prefixes(CommonPrefix::builder().prefix("dir/").build())
                .contents(
                    Object::builder()
                        .key("file.txt")
                        .size(10)
                        .last_modified(time)
                        .build(),
                )
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/prefix"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_only_prefixes_returns_0() {
        // Ported from test_success_rc_has_only_prefixes.
        // Only CommonPrefixes, no Contents → rc=0.
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(|| {
            ListObjectsV2Output::builder()
                .common_prefixes(CommonPrefix::builder().prefix("subdir/").build())
                .build()
        });
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/prefix"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_pagination_with_empty_second_page_returns_0() {
        // Ported from test_success_rc_with_pagination.
        // Page 1 has results, page 2 is empty → rc=0 (not 1).
        let time = aws_smithy_types::DateTime::from_secs(1389304549);
        let page1 = mock!(aws_sdk_s3::Client::list_objects_v2).then_output(move || {
            ListObjectsV2Output::builder()
                .common_prefixes(CommonPrefix::builder().prefix("foo/").build())
                .contents(
                    Object::builder()
                        .key("foo/bar.txt")
                        .size(100)
                        .last_modified(time)
                        .build(),
                )
                .next_continuation_token("token")
                .build()
        });
        let page2 = mock!(aws_sdk_s3::Client::list_objects_v2)
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[page1, page2]);
        let (ctx, _term) = test_ctx_with_client(client);

        let rc = run(ls_args("s3://bucket/foo"), &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_objects_ignores_bucket_name_prefix() {
        // Ported from test_list_objects_ignores_bucket_name_prefix.
        // --bucket-name-prefix is ignored when listing objects (not buckets).
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| req.bucket() == Some("mybucket"))
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://mybucket/");
        args.bucket_name_prefix = Some("ignored".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }

    #[tokio::test]
    async fn run_list_objects_ignores_bucket_region() {
        // Ported from test_list_objects_ignores_bucket_region.
        // --bucket-region is ignored when listing objects (not buckets).
        let list_objects = mock!(aws_sdk_s3::Client::list_objects_v2)
            .match_requests(|req| req.bucket() == Some("mybucket"))
            .then_output(|| ListObjectsV2Output::builder().build());
        let client = mock_client!(aws_sdk_s3, RuleMode::Sequential, &[list_objects]);
        let (ctx, _term) = test_ctx_with_client(client);

        let mut args = ls_args("s3://mybucket/");
        args.bucket_region = Some("us-west-1".to_string());
        let rc = run(args, &ctx).await.unwrap();

        assert_eq!(rc, 0);
    }
    // display_page — unit tests for formatting
    // -----------------------------------------------------------------------

    #[test]
    fn display_page_full_line() {
        let (ctx, term) = test_ctx();
        let dt = aws_smithy_types::DateTime::from_secs(1389304549);
        let expected_date = format_datetime_local(Some(&dt));
        let objects = vec![Object::builder()
            .key("foo/bar.txt")
            .size(100)
            .last_modified(dt)
            .build()];
        let mut state = LsState::new(false);

        display_page(&ctx, &[], &objects, false, &mut state).unwrap();

        assert_eq!(
            term.stdout_contents(),
            format!("{expected_date}        100 foo/bar.txt")
        );
        assert_eq!(state.total_objects, 1);
        assert_eq!(state.size_accumulator, 100);
    }

    #[test]
    fn display_page_object_with_basename() {
        let (ctx, term) = test_ctx();
        let dt = aws_smithy_types::DateTime::from_secs(1389304549);
        let expected_date = format_datetime_local(Some(&dt));
        let objects = vec![Object::builder()
            .key("foo/bar.txt")
            .size(100)
            .last_modified(dt)
            .build()];
        let mut state = LsState::new(false);

        display_page(&ctx, &[], &objects, true, &mut state).unwrap();

        assert_eq!(
            term.stdout_contents(),
            format!("{expected_date}        100 bar.txt")
        );
    }

    #[test]
    fn display_page_common_prefix() {
        let (ctx, term) = test_ctx();
        let prefixes = vec![CommonPrefix::builder().prefix("photos/").build()];
        let mut state = LsState::new(false);

        display_page(&ctx, &prefixes, &[], true, &mut state).unwrap();

        assert_eq!(
            term.stdout_contents(),
            "                           PRE photos/"
        );
    }

    #[test]
    fn display_page_empty_sets_empty_result() {
        let (ctx, term) = test_ctx();
        let mut state = LsState::new(false);

        display_page(&ctx, &[], &[], true, &mut state).unwrap();

        assert!(state.empty_result);
        assert_eq!(term.stdout_contents(), "");
    }

    #[test]
    fn display_page_accumulates_size() {
        let (ctx, _term) = test_ctx();
        let dt = aws_smithy_types::DateTime::from_secs(0);
        let objects = vec![
            Object::builder()
                .key("a")
                .size(100)
                .last_modified(dt)
                .build(),
            Object::builder()
                .key("b")
                .size(200)
                .last_modified(dt)
                .build(),
        ];
        let mut state = LsState::new(false);

        display_page(&ctx, &[], &objects, true, &mut state).unwrap();

        assert_eq!(state.total_objects, 2);
        assert_eq!(state.size_accumulator, 300);
    }
}
