//! Shared application context for all S3 CLI subcommands.

use aws_config::SdkConfig;

use crate::cli::GlobalArgs;
use crate::config::S3ConfigKeys;
use crate::term;

/// Shared context available to all S3 subcommands.
///
/// Constructed with a pre-configured S3 client, the `SdkConfig` it was
/// built from, and parsed global flags. Subcommand implementations
/// receive this by reference.
pub struct AppContext {
    /// AWS SDK S3 client, pre-configured with region, endpoint, credentials.
    pub client: aws_sdk_s3::Client,
    /// The `SdkConfig` the `client` was built from.
    pub sdk_config: SdkConfig,
    /// Global CLI flags (`--region`, `--debug`, `--no-sign-request`, etc.).
    pub globals: GlobalArgs,
    /// S3-specific config keys from profile + env vars.
    pub s3_config_keys: S3ConfigKeys,
    /// Terminal for output and terminal control.
    pub term: Box<dyn term::Terminal>,
}

impl AppContext {
    /// Create an AppContext with real stdout/stderr.
    pub fn new(
        client: aws_sdk_s3::Client,
        sdk_config: SdkConfig,
        globals: GlobalArgs,
        s3_config_keys: S3ConfigKeys,
    ) -> Self {
        Self {
            client,
            sdk_config,
            globals,
            s3_config_keys,
            term: Box::new(term::StdTerminal::new()),
        }
    }

    /// Build an `aws_sdk_s3::config::Builder` with S3-specific config keys
    /// and the redirect interceptor applied. This is the single path for
    /// constructing S3 client config — used by both the direct client
    /// (ls/mb/rb/rm/presign/website) and the Transfer Manager.
    pub fn s3_config_builder(&self) -> aws_sdk_s3::config::Builder {
        self.s3_config_keys
            .clone()
            .apply(aws_sdk_s3::config::Builder::from(&self.sdk_config))
            .interceptor(crate::redirect::RegionRedirectInterceptor::new())
            .retry_classifier(crate::redirect::region_redirect_classifier())
    }
}

/// Test-only helpers.
#[cfg(test)]
pub mod test_util {
    use super::AppContext;
    use aws_config::{BehaviorVersion, SdkConfig};

    use crate::cli::GlobalArgs;
    use crate::term::Terminal;

    /// A minimal `SdkConfig` usable wherever tests need one. Real tests
    /// exercise S3 via mock clients, so this only needs to satisfy the
    /// type requirement — region is set to match the mocked client.
    pub fn test_sdk_config() -> SdkConfig {
        SdkConfig::builder()
            .behavior_version(BehaviorVersion::v2026_01_12())
            .region(aws_config::Region::new("us-east-1"))
            .build()
    }

    /// Construct an `AppContext` for tests using a caller-provided
    /// terminal and S3 client. Handles `sdk_config` + `globals` defaults.
    pub fn app_context<T: Terminal + 'static>(client: aws_sdk_s3::Client, term: T) -> AppContext {
        AppContext {
            client,
            sdk_config: test_sdk_config(),
            globals: GlobalArgs::default(),
            s3_config_keys: crate::config::S3ConfigKeys::default(),
            term: Box::new(term),
        }
    }
}
