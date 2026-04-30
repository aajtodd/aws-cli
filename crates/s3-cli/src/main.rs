//! Standalone binary entry point for the native S3 CLI.
//!
//! Handles argument parsing, SDK configuration, and SIGPIPE. The actual
//! S3 command logic lives in the library crate ([`s3_cli::handle_s3_cmd`]).

use std::process::ExitCode;

use clap::Parser;
use s3_cli::cli::{Cli, GlobalArgs, Service};
use s3_cli::context::AppContext;

fn main() -> ExitCode {
    reset_sigpipe();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            e.print().ok();
            let code = if e.use_stderr() {
                s3_cli::exit_code::PARAM_VALIDATION_ERROR
            } else {
                0
            };
            return ExitCode::from(code as u8);
        }
    };

    // TODO: remove when aws-smithy-http-client exposes a TLS verification
    // toggle (e.g. TrustStore::with_certificate_verification(bool)).
    if cli.globals.no_verify_ssl {
        eprintln!(
            "\n--no-verify-ssl is not currently supported. TLS certificate \
             verification cannot be disabled in this build. See \
             crates/docs/compat.md for status."
        );
        return ExitCode::from(s3_cli::exit_code::PARAM_VALIDATION_ERROR as u8);
    }

    // TODO: remove when TM exposes `S3ClientConfig::with_tls_context(...)`
    // or equivalent so --ca-bundle applies uniformly to cp/sync.
    if cli.globals.ca_bundle.is_some() {
        eprintln!(
            "\n--ca-bundle is not currently supported. See \
             crates/docs/compat.md for status."
        );
        return ExitCode::from(s3_cli::exit_code::PARAM_VALIDATION_ERROR as u8);
    }

    // Install the tracing subscriber before the runtime starts so SDK
    // config-time logs are captured.
    install_tracing(&cli.globals);

    let Service::S3 { command } = cli.service;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(async {
        let ctx = build_context(&cli.globals).await;
        let code = s3_cli::handle_s3_cmd(command, &ctx).await;
        ExitCode::from(code as u8)
    })
}

/// Install a `tracing_subscriber::fmt` logger writing to stderr when
/// `--debug` is set. Silent when absent.
///
/// `RUST_LOG` takes precedence if set. Default filter enables DEBUG for
/// the Rust equivalents of Python's `botocore`/`awscli`/`s3transfer`/
/// `urllib3` loggers.
fn install_tracing(globals: &GlobalArgs) {
    if !globals.debug {
        return;
    }
    use tracing_subscriber::{fmt, EnvFilter};

    // Target mapping (Python logger → Rust target prefix):
    //   botocore  → aws_config, aws_runtime, aws_sdk_*, aws_smithy_*
    //   awscli    → s3_cli
    //   s3transfer → aws_sdk_s3_transfer_manager (crate name) plus
    //                `aws_s3_transfer_manager::*` (custom targets in
    //                telemetry.rs)
    //   urllib3   → hyper, h2
    // Signing paths (`aws_sigv4`, `aws_runtime::auth`) are elevated to
    // TRACE so canonical request + string-to-sign appear — these match
    // Python's `botocore.auth` DEBUG output.
    const DEFAULT_FILTER: &str = "\
        off,\
        s3_cli=debug,\
        aws_config=debug,\
        aws_runtime=debug,\
        aws_runtime::auth=trace,\
        aws_sdk_s3=debug,\
        aws_sdk_sts=debug,\
        aws_sigv4=trace,\
        aws_smithy_runtime=debug,\
        aws_smithy_runtime_api=debug,\
        aws_smithy_http_client=debug,\
        aws_sdk_s3_transfer_manager=debug,\
        aws_s3_transfer_manager=debug,\
        hyper=debug,\
        h2=debug\
    ";

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true)
        .init();
}

/// Build an [`AppContext`] by translating global flags into SDK
/// configuration and constructing the S3 client.
async fn build_context(globals: &GlobalArgs) -> AppContext {
    // Pin BehaviorVersion rather than `latest()` to lock the compat
    // surface. Bump deliberately.
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::v2026_01_12());

    // Resolve effective profile: --profile > AWS_PROFILE > AWS_DEFAULT_PROFILE.
    // Rust SDK only reads AWS_PROFILE; we handle AWS_DEFAULT_PROFILE ourselves.
    let effective_profile = resolve_profile_name(globals.profile.as_deref());
    if let Some(ref profile) = effective_profile {
        config_loader = config_loader.profile_name(profile);
    }

    if let Some(ref region) = globals.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }
    if let Some(ref endpoint) = globals.endpoint_url {
        config_loader = config_loader.endpoint_url(endpoint);
    }

    if globals.no_sign_request {
        config_loader = config_loader.no_credentials();
    }

    let http_client = s3_cli::config::build_http_client(globals);
    config_loader = config_loader
        .http_client(http_client)
        .timeout_config(s3_cli::config::build_timeout_config(globals));

    let sdk_config = config_loader.load().await;

    let s3_keys = s3_cli::config::load_s3_config(effective_profile.as_deref()).await;
    let s3_config_builder = s3_keys.apply(aws_sdk_s3::config::Builder::from(&sdk_config));
    let client = aws_sdk_s3::Client::from_conf(s3_config_builder.build());
    AppContext::new(client, sdk_config, globals.clone())
}

/// Resolve the effective profile name.
///
/// Priority (matches Python's `configprovider.py`):
/// `--profile` flag > `AWS_PROFILE` env > `AWS_DEFAULT_PROFILE` env > None (SDK default).
///
/// Returns `None` when no override is needed (SDK will use "default").
fn resolve_profile_name(cli_flag: Option<&str>) -> Option<String> {
    s3_cli::config::resolve_profile_name(cli_flag)
}

/// Reset SIGPIPE to default behavior so piping to `head`, `less`, etc.
/// terminates cleanly instead of panicking.
///
/// On Windows, broken pipe is handled differently (ERROR_BROKEN_PIPE from
/// WriteFile). Rust's I/O returns `ErrorKind::BrokenPipe` which we handle
/// in the write paths. A full Windows solution would use SetConsoleCtrlHandler
/// or similar — deferred until we have Windows CI.
fn reset_sigpipe() {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
