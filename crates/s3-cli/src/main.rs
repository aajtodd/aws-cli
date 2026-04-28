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

/// Build an [`AppContext`] from parsed global flags.
///
/// Applies `--region`, `--endpoint-url`, `--profile`, `--no-verify-ssl`,
/// etc. to the SDK configuration before constructing the S3 client.
async fn build_context(globals: &GlobalArgs) -> AppContext {
    // TODO - need to pin behavior version
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    if let Some(ref region) = globals.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }
    if let Some(ref endpoint) = globals.endpoint_url {
        config_loader = config_loader.endpoint_url(endpoint);
    }
    if let Some(ref profile) = globals.profile {
        config_loader = config_loader.profile_name(profile);
    }

    let config = config_loader.load().await;

    // FIXME(compat): aws-config does not parse S3-specific config keys.
    // Python's botocore reads these from `~/.aws/config` `[s3]` section and
    // `AWS_S3_*` env vars; we need to read them ourselves and apply to the
    // S3 Config builder. Affects:
    //   - addressing_style (path/virtual/auto) → force_path_style
    //   - use_arn_region → use_arn_region
    //   - us_east_1_regional_endpoint → (endpoint resolution)
    //   - use_accelerate_endpoint → accelerate
    //   - use_dualstack_endpoint → use_dualstack_endpoint
    //   - signature_version → (sigv4/sigv4a selection)
    //   - payload_signing_enabled
    //   - s3_disable_multiregion_access_points
    // See docs/compat.md for the full list and Python test coverage.
    let client = aws_sdk_s3::Client::new(&config);
    AppContext::new(client, globals.clone())
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
