//! CLI argument definitions for the `aws s3` command surface.
//!
//! All clap definitions live here so the full command surface is visible
//! in one place. Command implementations live in [`crate::commands`].
//!
//! The top-level parser models the full `aws [globals] s3 <subcommand>`
//! invocation. Only the `s3` service is implemented.

use crate::uri::TransferUri;
use clap::{Parser, Subcommand};

// ---------------------------------------------------------------------------
// Top-level: aws [globals] s3 <subcommand>
// ---------------------------------------------------------------------------

/// AWS CLI — native S3 implementation.
#[derive(Debug, Parser)]
#[command(name = "aws", disable_help_subcommand = true)]
pub struct Cli {
    #[command(flatten)]
    pub globals: GlobalArgs,

    #[command(subcommand)]
    pub service: Service,
}

/// Service-level dispatch. Only `s3` is implemented.
#[derive(Debug, Subcommand)]
pub enum Service {
    /// Amazon S3 high-level commands.
    S3 {
        #[command(subcommand)]
        command: S3Command,
    },
}

// ---------------------------------------------------------------------------
// Global args — mirrors awscli/data/cli.json
// ---------------------------------------------------------------------------

/// Global flags parsed before the service subcommand.
///
/// These mirror the options defined in `awscli/data/cli.json`.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct GlobalArgs {
    /// Turn on debug logging.
    #[arg(long, global = true)]
    pub debug: bool,

    /// Override command's default URL with the given URL.
    #[arg(long, global = true)]
    pub endpoint_url: Option<String>,

    /// By default, the AWS CLI uses SSL when communicating with AWS services.
    /// This option disables SSL certificate verification.
    #[arg(long, global = true)]
    pub no_verify_ssl: bool,

    /// Disable automatic pagination.
    #[arg(long, global = true)]
    pub no_paginate: bool,

    /// The formatting style for command output.
    #[arg(long, global = true)]
    pub output: Option<String>,

    /// A JMESPath query to use in filtering the response data.
    #[arg(long, global = true)]
    pub query: Option<String>,

    /// Use a specific profile from your credential file.
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// The region to use. Overrides config/env settings.
    #[arg(long, global = true)]
    pub region: Option<String>,

    /// Turn on/off color output.
    #[arg(long, global = true, default_value = "auto")]
    pub color: String,

    /// Do not sign requests. Credentials will not be loaded.
    #[arg(long, global = true)]
    pub no_sign_request: bool,

    /// The CA certificate bundle to use when verifying SSL certificates.
    #[arg(long, global = true)]
    pub ca_bundle: Option<String>,

    /// The maximum socket read time in seconds.
    #[arg(long, global = true)]
    pub cli_read_timeout: Option<u64>,

    /// The maximum socket connect time in seconds.
    #[arg(long, global = true)]
    pub cli_connect_timeout: Option<u64>,

    /// The formatting style to be used for binary blobs.
    #[arg(long, global = true)]
    pub cli_binary_format: Option<String>,

    /// Disable cli pager for output.
    #[arg(long, global = true)]
    pub no_cli_pager: bool,

    /// Automatically prompt for CLI input parameters.
    #[arg(long, global = true)]
    pub cli_auto_prompt: bool,

    /// Disable automatically prompt for CLI input parameters.
    #[arg(long, global = true)]
    pub no_cli_auto_prompt: bool,

    /// The formatting style for error output.
    #[arg(long, global = true)]
    pub cli_error_format: Option<String>,
}

// ---------------------------------------------------------------------------
// S3 subcommands
// ---------------------------------------------------------------------------

/// All S3 subcommands.
#[derive(Debug, Subcommand)]
pub enum S3Command {
    /// List S3 objects and common prefixes under a prefix or all S3 buckets.
    Ls(LsArgs),
    /// Copy files and objects to and from S3.
    Cp(CpArgs),
    /// Move files and objects to and from S3.
    Mv(MvArgs),
    /// Delete S3 objects.
    Rm(RmArgs),
    /// Sync directories and S3 prefixes.
    Sync(SyncArgs),
    /// Create an S3 bucket.
    Mb(MbArgs),
    /// Remove an S3 bucket.
    Rb(RbArgs),
    /// Generate a pre-signed URL for an S3 object.
    Presign(PresignArgs),
    /// Set or remove the website configuration for a bucket.
    Website(WebsiteArgs),
}

// ---------------------------------------------------------------------------
// ls
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct LsArgs {
    /// S3 URI to list. Omit to list all buckets.
    #[arg(default_value = "s3://")]
    pub s3uri: String,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long)]
    pub human_readable: bool,

    #[arg(long)]
    pub summarize: bool,

    #[arg(long)]
    pub page_size: Option<i32>,

    #[arg(
        long,
        value_name = "requester",
        num_args = 0..=1,
        default_missing_value = "requester",
        value_parser = ["requester"],
    )]
    pub request_payer: Option<String>,

    #[arg(long)]
    pub bucket_name_prefix: Option<String>,

    #[arg(long)]
    pub bucket_region: Option<String>,
}

// ---------------------------------------------------------------------------
// cp / mv / sync shared args
// ---------------------------------------------------------------------------

/// Arguments shared by cp, mv, and sync.
#[derive(Debug, clap::Args)]
pub struct TransferArgs {
    #[arg(long)]
    pub dryrun: bool,

    #[arg(long)]
    pub quiet: bool,

    #[arg(long)]
    pub only_show_errors: bool,

    #[arg(long)]
    pub no_progress: bool,

    #[arg(long)]
    pub follow_symlinks: Option<bool>,

    #[arg(long)]
    pub no_follow_symlinks: bool,

    #[arg(long)]
    pub no_guess_mime_type: bool,

    #[arg(long)]
    pub content_type: Option<String>,

    #[arg(long)]
    pub cache_control: Option<String>,

    #[arg(long)]
    pub content_disposition: Option<String>,

    #[arg(long)]
    pub content_encoding: Option<String>,

    #[arg(long)]
    pub content_language: Option<String>,

    #[arg(long)]
    pub expires: Option<String>,

    #[arg(long)]
    pub metadata: Option<String>,

    #[arg(long)]
    pub metadata_directive: Option<String>,

    #[arg(long)]
    pub storage_class: Option<String>,

    #[arg(long)]
    pub acl: Option<String>,

    #[arg(long)]
    pub grants: Vec<String>,

    #[arg(long)]
    pub website_redirect: Option<String>,

    #[arg(long)]
    pub sse: Option<String>,

    #[arg(long)]
    pub sse_c: Option<String>,

    #[arg(long)]
    pub sse_c_key: Option<String>,

    #[arg(long)]
    pub sse_kms_key_id: Option<String>,

    #[arg(long)]
    pub sse_c_copy_source: Option<String>,

    #[arg(long)]
    pub sse_c_copy_source_key: Option<String>,

    #[arg(
        long,
        value_name = "requester",
        num_args = 0..=1,
        default_missing_value = "requester",
        value_parser = ["requester"],
    )]
    pub request_payer: Option<String>,

    #[arg(long)]
    pub ignore_glacier_warnings: bool,

    #[arg(long)]
    pub force_glacier_transfer: bool,

    #[arg(long)]
    pub copy_props: Option<String>,

    #[arg(long)]
    pub checksum_mode: Option<String>,

    #[arg(long)]
    pub checksum_algorithm: Option<String>,

    #[arg(long)]
    pub no_overwrite: bool,

    #[arg(long)]
    pub page_size: Option<i32>,
}

/// Include/exclude filter arguments.
#[derive(Debug, Default, clap::Args)]
pub struct FilterArgs {
    #[arg(long)]
    pub include: Vec<String>,

    #[arg(long)]
    pub exclude: Vec<String>,
}

// ---------------------------------------------------------------------------
// cp
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct CpArgs {
    pub source: TransferUri,
    pub dest: TransferUri,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long)]
    pub expected_size: Option<String>,

    #[command(flatten)]
    pub transfer: TransferArgs,

    #[command(flatten)]
    pub filters: FilterArgs,
}

// ---------------------------------------------------------------------------
// mv
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct MvArgs {
    pub source: TransferUri,
    pub dest: TransferUri,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long)]
    pub expected_size: Option<String>,

    #[arg(long)]
    pub validate_same_s3_paths: bool,

    #[command(flatten)]
    pub transfer: TransferArgs,

    #[command(flatten)]
    pub filters: FilterArgs,
}

// ---------------------------------------------------------------------------
// rm
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct RmArgs {
    pub path: TransferUri,

    #[arg(long)]
    pub dryrun: bool,

    #[arg(long)]
    pub quiet: bool,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long)]
    pub only_show_errors: bool,

    #[arg(long)]
    pub page_size: Option<i32>,

    #[arg(
        long,
        value_name = "requester",
        num_args = 0..=1,
        default_missing_value = "requester",
        value_parser = ["requester"],
    )]
    pub request_payer: Option<String>,
}

// ---------------------------------------------------------------------------
// sync
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct SyncArgs {
    pub source: TransferUri,
    pub dest: TransferUri,

    #[arg(long)]
    pub size_only: bool,

    #[arg(long)]
    pub exact_timestamps: bool,

    #[arg(long)]
    pub delete: bool,

    #[command(flatten)]
    pub transfer: TransferArgs,

    #[command(flatten)]
    pub filters: FilterArgs,
}

// ---------------------------------------------------------------------------
// mb
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct MbArgs {
    pub path: TransferUri,

    /// Tags to add to the bucket: --tags Key Value (repeatable).
    #[arg(long, num_args = 2, action = clap::ArgAction::Append, value_names = ["KEY", "VALUE"])]
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// rb
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct RbArgs {
    pub path: TransferUri,

    #[arg(long)]
    pub force: bool,
}

// ---------------------------------------------------------------------------
// presign
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct PresignArgs {
    pub path: String,

    #[arg(long, default_value = "3600")]
    pub expires_in: u64,
}

// ---------------------------------------------------------------------------
// website
// ---------------------------------------------------------------------------

#[derive(Debug, clap::Args)]
pub struct WebsiteArgs {
    pub path: String,

    #[arg(long)]
    pub index_document: Option<String>,

    #[arg(long)]
    pub error_document: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn parse(args: &[&str]) -> Cli {
        let mut full = vec!["aws"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap()
    }

    fn parse_err(args: &[&str]) -> clap::Error {
        let mut full = vec!["aws"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap_err()
    }

    // -----------------------------------------------------------------------
    // Global args
    // -----------------------------------------------------------------------

    #[test]
    fn globals_default_values() {
        let cli = parse(&["s3", "ls"]);
        assert!(!cli.globals.debug);
        assert!(cli.globals.endpoint_url.is_none());
        assert!(!cli.globals.no_verify_ssl);
        assert!(!cli.globals.no_paginate);
        assert!(cli.globals.output.is_none());
        assert!(cli.globals.query.is_none());
        assert!(cli.globals.profile.is_none());
        assert!(cli.globals.region.is_none());
        assert_eq!(cli.globals.color, "auto");
        assert!(!cli.globals.no_sign_request);
        assert!(cli.globals.ca_bundle.is_none());
        assert!(cli.globals.cli_read_timeout.is_none());
        assert!(cli.globals.cli_connect_timeout.is_none());
        assert!(cli.globals.cli_binary_format.is_none());
        assert!(!cli.globals.no_cli_pager);
        assert!(!cli.globals.cli_auto_prompt);
        assert!(!cli.globals.no_cli_auto_prompt);
        assert!(cli.globals.cli_error_format.is_none());
    }

    #[test]
    fn globals_before_service() {
        let cli = parse(&[
            "--region",
            "us-west-2",
            "--endpoint-url",
            "http://localhost:4566",
            "--profile",
            "dev",
            "--debug",
            "--no-verify-ssl",
            "--no-sign-request",
            "s3",
            "ls",
        ]);
        assert_eq!(cli.globals.region.as_deref(), Some("us-west-2"));
        assert_eq!(
            cli.globals.endpoint_url.as_deref(),
            Some("http://localhost:4566")
        );
        assert_eq!(cli.globals.profile.as_deref(), Some("dev"));
        assert!(cli.globals.debug);
        assert!(cli.globals.no_verify_ssl);
        assert!(cli.globals.no_sign_request);
    }

    #[test]
    fn globals_after_subcommand() {
        // global = true allows flags after the subcommand too
        let cli = parse(&["s3", "ls", "--region", "eu-west-1", "--debug"]);
        assert_eq!(cli.globals.region.as_deref(), Some("eu-west-1"));
        assert!(cli.globals.debug);
    }

    #[test]
    fn globals_timeout_values() {
        let cli = parse(&[
            "--cli-read-timeout",
            "30",
            "--cli-connect-timeout",
            "10",
            "s3",
            "ls",
        ]);
        assert_eq!(cli.globals.cli_read_timeout, Some(30));
        assert_eq!(cli.globals.cli_connect_timeout, Some(10));
    }

    #[test]
    fn globals_ca_bundle() {
        let cli = parse(&["--ca-bundle", "/path/to/cert.pem", "s3", "ls"]);
        assert_eq!(cli.globals.ca_bundle.as_deref(), Some("/path/to/cert.pem"));
    }

    // -----------------------------------------------------------------------
    // Validate against cli.json — every option in cli.json must have a
    // corresponding field in GlobalArgs.
    // -----------------------------------------------------------------------

    #[test]
    fn global_args_cover_cli_json() {
        // All options from awscli/data/cli.json (authoritative source).
        // If the Python CLI adds a new global option, this test must be
        // updated — which is the point.
        let cli_json_options: HashSet<&str> = [
            "debug",
            "endpoint-url",
            "no-verify-ssl",
            "no-paginate",
            "output",
            "query",
            "profile",
            "region",
            "version",
            "color",
            "no-sign-request",
            "ca-bundle",
            "cli-read-timeout",
            "cli-connect-timeout",
            "cli-binary-format",
            "no-cli-pager",
            "cli-auto-prompt",
            "no-cli-auto-prompt",
            "cli-error-format",
        ]
        .into_iter()
        .collect();

        // --version is handled by clap automatically, not as a field.
        let skip = HashSet::from(["version"]);

        // Verify every cli.json option (except skipped) is accepted by our parser.
        for opt in &cli_json_options {
            if skip.contains(opt) {
                continue;
            }
            let flag = format!("--{opt}");
            // Try boolean form, string value form, and integer value form
            // to handle flags, string options, and typed (int) options.
            let args_bool = vec!["aws", &flag, "s3", "ls"];
            let args_str = vec!["aws", &flag, "dummy", "s3", "ls"];
            let args_int = vec!["aws", &flag, "42", "s3", "ls"];

            let accepted = Cli::try_parse_from(&args_bool).is_ok()
                || Cli::try_parse_from(&args_str).is_ok()
                || Cli::try_parse_from(&args_int).is_ok();
            assert!(
                accepted,
                "cli.json option '{opt}' is not accepted by our CLI parser"
            );
        }
    }

    // -----------------------------------------------------------------------
    // ls
    // -----------------------------------------------------------------------

    #[test]
    fn ls_no_args_defaults_to_all_buckets() {
        let cli = parse(&["s3", "ls"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert_eq!(args.s3uri, "s3://");
        assert!(!args.recursive);
        assert!(!args.human_readable);
        assert!(!args.summarize);
        assert!(args.page_size.is_none());
    }

    #[test]
    fn ls_with_uri() {
        let cli = parse(&["s3", "ls", "s3://my-bucket/prefix/"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert_eq!(args.s3uri, "s3://my-bucket/prefix/");
    }

    #[test]
    fn ls_all_flags() {
        let cli = parse(&[
            "s3",
            "ls",
            "s3://bucket",
            "--recursive",
            "--human-readable",
            "--summarize",
            "--page-size",
            "100",
            "--request-payer",
            "requester",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert!(args.recursive);
        assert!(args.human_readable);
        assert!(args.summarize);
        assert_eq!(args.page_size, Some(100));
        assert_eq!(args.request_payer.as_deref(), Some("requester"));
    }

    #[test]
    fn ls_bucket_name_prefix_and_region() {
        let cli = parse(&[
            "s3",
            "ls",
            "--bucket-name-prefix",
            "my-",
            "--bucket-region",
            "us-west-2",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert_eq!(args.bucket_name_prefix.as_deref(), Some("my-"));
        assert_eq!(args.bucket_region.as_deref(), Some("us-west-2"));
    }

    // -----------------------------------------------------------------------
    // cp
    // -----------------------------------------------------------------------

    #[test]
    fn cp_basic_upload() {
        let cli = parse(&["s3", "cp", "./file.txt", "s3://bucket/key"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert!(matches!(args.source, TransferUri::Local(_)));
        assert!(matches!(args.dest, TransferUri::S3(_)));
        assert!(!args.recursive);
    }

    #[test]
    fn cp_basic_download() {
        let cli = parse(&["s3", "cp", "s3://bucket/key", "./file.txt"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert!(matches!(args.source, TransferUri::S3(_)));
        assert!(matches!(args.dest, TransferUri::Local(_)));
    }

    #[test]
    fn cp_s3_to_s3() {
        let cli = parse(&["s3", "cp", "s3://src/key", "s3://dst/key"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert!(matches!(args.source, TransferUri::S3(_)));
        assert!(matches!(args.dest, TransferUri::S3(_)));
    }

    #[test]
    fn cp_transfer_args() {
        let cli = parse(&[
            "s3",
            "cp",
            "s3://b/k",
            "./f",
            "--dryrun",
            "--quiet",
            "--only-show-errors",
            "--no-progress",
            "--storage-class",
            "GLACIER",
            "--sse",
            "AES256",
            "--acl",
            "public-read",
            "--content-type",
            "text/plain",
            "--no-guess-mime-type",
            "--no-overwrite",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert!(args.transfer.dryrun);
        assert!(args.transfer.quiet);
        assert!(args.transfer.only_show_errors);
        assert!(args.transfer.no_progress);
        assert_eq!(args.transfer.storage_class.as_deref(), Some("GLACIER"));
        assert_eq!(args.transfer.sse.as_deref(), Some("AES256"));
        assert_eq!(args.transfer.acl.as_deref(), Some("public-read"));
        assert_eq!(args.transfer.content_type.as_deref(), Some("text/plain"));
        assert!(args.transfer.no_guess_mime_type);
        assert!(args.transfer.no_overwrite);
    }

    #[test]
    fn cp_filter_args() {
        let cli = parse(&[
            "s3",
            "cp",
            "s3://b/",
            "./d",
            "--recursive",
            "--exclude",
            "*.log",
            "--include",
            "important.log",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert!(args.recursive);
        assert_eq!(args.filters.exclude, vec!["*.log"]);
        assert_eq!(args.filters.include, vec!["important.log"]);
    }

    #[test]
    fn cp_requires_two_paths() {
        let err = parse_err(&["s3", "cp", "s3://bucket"]);
        assert!(err.to_string().contains("required"));
    }

    // -----------------------------------------------------------------------
    // mv
    // -----------------------------------------------------------------------

    #[test]
    fn mv_has_validate_same_s3_paths() {
        let cli = parse(&[
            "s3",
            "mv",
            "s3://a/k",
            "s3://b/k",
            "--validate-same-s3-paths",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Mv(args) = command else {
            panic!("expected Mv");
        };
        assert!(args.validate_same_s3_paths);
    }

    // -----------------------------------------------------------------------
    // rm
    // -----------------------------------------------------------------------

    #[test]
    fn rm_basic() {
        let cli = parse(&["s3", "rm", "s3://bucket/key"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Rm(args) = command else {
            panic!("expected Rm");
        };
        assert!(matches!(args.path, TransferUri::S3(_)));
        assert!(!args.recursive);
        assert!(!args.dryrun);
    }

    #[test]
    fn rm_recursive() {
        let cli = parse(&["s3", "rm", "s3://bucket/prefix/", "--recursive"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Rm(args) = command else {
            panic!("expected Rm");
        };
        assert!(args.recursive);
    }

    // -----------------------------------------------------------------------
    // sync
    // -----------------------------------------------------------------------

    #[test]
    fn sync_with_delete_and_size_only() {
        let cli = parse(&[
            "s3",
            "sync",
            "./local",
            "s3://bucket/prefix",
            "--delete",
            "--size-only",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Sync(args) = command else {
            panic!("expected Sync");
        };
        assert!(args.delete);
        assert!(args.size_only);
        assert!(!args.exact_timestamps);
    }

    // -----------------------------------------------------------------------
    // mb / rb
    // -----------------------------------------------------------------------

    #[test]
    fn mb_basic() {
        let cli = parse(&["s3", "mb", "s3://new-bucket"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Mb(args) = command else {
            panic!("expected Mb");
        };
        assert!(matches!(args.path, TransferUri::S3(_)));
        assert!(args.tags.is_empty());
    }

    #[test]
    fn mb_with_tags() {
        let cli = parse(&[
            "s3",
            "mb",
            "s3://bucket",
            "--tags",
            "Key1",
            "Value1",
            "--tags",
            "Key2",
            "Value2",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Mb(args) = command else {
            panic!("expected Mb");
        };
        assert_eq!(args.tags, vec!["Key1", "Value1", "Key2", "Value2"]);
    }

    #[test]
    fn rb_basic() {
        let cli = parse(&["s3", "rb", "s3://bucket"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Rb(args) = command else {
            panic!("expected Rb");
        };
        assert!(!args.force);
    }

    #[test]
    fn rb_with_force() {
        let cli = parse(&["s3", "rb", "s3://bucket", "--force"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Rb(args) = command else {
            panic!("expected Rb");
        };
        assert!(args.force);
    }

    // -----------------------------------------------------------------------
    // presign
    // -----------------------------------------------------------------------

    #[test]
    fn presign_default_expiry() {
        let cli = parse(&["s3", "presign", "s3://bucket/key"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Presign(args) = command else {
            panic!("expected Presign");
        };
        assert_eq!(args.expires_in, 3600);
    }

    #[test]
    fn presign_custom_expiry() {
        let cli = parse(&["s3", "presign", "s3://bucket/key", "--expires-in", "900"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Presign(args) = command else {
            panic!("expected Presign");
        };
        assert_eq!(args.expires_in, 900);
    }

    // -----------------------------------------------------------------------
    // website
    // -----------------------------------------------------------------------

    #[test]
    fn website_with_documents() {
        let cli = parse(&[
            "s3",
            "website",
            "s3://bucket",
            "--index-document",
            "index.html",
            "--error-document",
            "error.html",
        ]);
        let Service::S3 { command } = cli.service;
        let S3Command::Website(args) = command else {
            panic!("expected Website");
        };
        assert_eq!(args.index_document.as_deref(), Some("index.html"));
        assert_eq!(args.error_document.as_deref(), Some("error.html"));
    }

    // -----------------------------------------------------------------------
    // Error cases — ported from test_ls_command.py and test_subcommands.py
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_subcommand_is_error() {
        let err = parse_err(&["s3", "bogus"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn unknown_flag_is_error() {
        // Ported from test_errors_out_with_extra_arguments
        let err = parse_err(&["s3", "ls", "--extra-argument-foo"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn unknown_flag_mentioned_in_error() {
        // Ported from test_errors_out_with_extra_arguments:
        // stderr should contain the unknown flag name
        let err = parse_err(&["s3", "ls", "--extra-argument-foo"]);
        let msg = err.to_string();
        assert!(
            msg.contains("extra-argument-foo"),
            "error should mention the flag: {msg}"
        );
    }

    #[test]
    fn no_service_is_error() {
        let err = parse_err(&[]);
        assert!(err.use_stderr());
    }

    #[test]
    fn no_subcommand_is_error() {
        let err = parse_err(&["s3"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn cp_missing_dest_is_error() {
        let err = parse_err(&["s3", "cp", "s3://bucket/key"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn mv_missing_dest_is_error() {
        let err = parse_err(&["s3", "mv", "s3://bucket/key"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn sync_missing_dest_is_error() {
        let err = parse_err(&["s3", "sync", "s3://bucket/"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn rm_missing_path_is_error() {
        let err = parse_err(&["s3", "rm"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn mb_missing_path_is_error() {
        let err = parse_err(&["s3", "mb"]);
        assert!(err.use_stderr());
    }

    #[test]
    fn presign_missing_path_is_error() {
        let err = parse_err(&["s3", "presign"]);
        assert!(err.use_stderr());
    }

    // --- --request-payer optional value (Python nargs='?' const='requester') ---

    #[test]
    fn ls_request_payer_bare_flag_defaults_to_requester() {
        let cli = parse(&["s3", "ls", "s3://bucket", "--request-payer"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert_eq!(args.request_payer.as_deref(), Some("requester"));
    }

    #[test]
    fn ls_request_payer_explicit_value() {
        let cli = parse(&["s3", "ls", "s3://bucket", "--request-payer", "requester"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert_eq!(args.request_payer.as_deref(), Some("requester"));
    }

    #[test]
    fn ls_request_payer_absent_is_none() {
        let cli = parse(&["s3", "ls", "s3://bucket"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Ls(args) = command else {
            panic!("expected Ls");
        };
        assert!(args.request_payer.is_none());
    }

    #[test]
    fn cp_request_payer_bare_flag_defaults_to_requester() {
        let cli = parse(&["s3", "cp", "./src", "s3://bucket/dst", "--request-payer"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Cp(args) = command else {
            panic!("expected Cp");
        };
        assert_eq!(args.transfer.request_payer.as_deref(), Some("requester"));
    }

    #[test]
    fn rm_request_payer_bare_flag_defaults_to_requester() {
        let cli = parse(&["s3", "rm", "s3://bucket/key", "--request-payer"]);
        let Service::S3 { command } = cli.service;
        let S3Command::Rm(args) = command else {
            panic!("expected Rm");
        };
        assert_eq!(args.request_payer.as_deref(), Some("requester"));
    }

    #[test]
    fn ls_request_payer_rejects_invalid_value() {
        // Matches Python's `choices=['requester']` — any other value
        // (including a misplaced positional like './src') is rejected
        // at parse time with "invalid choice"/"invalid value".
        let err = parse_err(&["s3", "ls", "s3://b", "--request-payer", "bogus"]);
        assert!(err.use_stderr());
    }
}
