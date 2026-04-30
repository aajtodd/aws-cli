//! SDK configuration builders driven by global CLI args.
//!
//! Translates parsed [`GlobalArgs`] into SDK-layer config inputs
//! (`SharedHttpClient`, `TimeoutConfig`). Callers apply the results to
//! an [`aws_config::ConfigLoader`].

use std::time::Duration;

use aws_smithy_http_client::{
    tls::{self, TlsContext, TrustStore},
    Builder,
};
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use aws_smithy_types::timeout::TimeoutConfig;

use crate::cli::GlobalArgs;

/// Build the HTTP client the SDK will use for all S3 requests.
///
/// Uses smithy-rs's rustls + aws-lc provider with native root certificates.
pub fn build_http_client(_globals: &GlobalArgs) -> SharedHttpClient {
    Builder::new()
        .tls_provider(tls::Provider::Rustls(
            tls::rustls_provider::CryptoMode::AwsLc,
        ))
        .build_https()
}

/// Build a `TimeoutConfig` reflecting any `--cli-read-timeout` and
/// `--cli-connect-timeout` values.
///
/// Always returns a config — when neither flag is set the result has
/// no overrides applied, equivalent to not calling
/// `.timeout_config(...)` at all.
///
/// Python semantics: `0` means "disabled / block forever." We honor
/// that by skipping the corresponding setter, matching
/// `awscli/customizations/globalargs.py:_resolve_timeout`.
pub fn build_timeout_config(globals: &GlobalArgs) -> TimeoutConfig {
    let mut builder = TimeoutConfig::builder();
    if let Some(secs) = globals.cli_connect_timeout.filter(|s| *s > 0) {
        builder = builder.connect_timeout(Duration::from_secs(secs));
    }
    if let Some(secs) = globals.cli_read_timeout.filter(|s| *s > 0) {
        builder = builder.read_timeout(Duration::from_secs(secs));
    }
    builder.build()
}

// TODO: re-enable `--ca-bundle` once TM supports `TlsContext` passthrough.

/// Error building a custom TLS context from `--ca-bundle`.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum CaBundleError {
    #[error("failed to read CA bundle '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("CA bundle '{path}' is invalid: {message}")]
    Parse { path: String, message: String },
    #[error("failed to construct TLS context: {0}")]
    TlsContext(Box<dyn std::error::Error + Send + Sync>),
}

/// Build a custom TLS context whose only trust root is the PEM at `path`.
#[allow(dead_code)]
pub fn build_ca_bundle_tls_context(path: &str) -> Result<TlsContext, CaBundleError> {
    // TODO: drop the two pre-parse passes below in favor of upstream
    // `TrustStore::validate()` / `try_with_pem_certificate()` once it exists.
    let pem = std::fs::read(path).map_err(|source| CaBundleError::Io {
        path: path.to_string(),
        source,
    })?;

    let certs: Vec<_> = rustls_pemfile::certs(&mut pem.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CaBundleError::Parse {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    if certs.is_empty() {
        return Err(CaBundleError::Parse {
            path: path.to_string(),
            message: "no CERTIFICATE blocks found".to_string(),
        });
    }

    let mut roots = rustls::RootCertStore::empty();
    let (valid, invalid) = roots.add_parsable_certificates(certs);
    if valid == 0 {
        return Err(CaBundleError::Parse {
            path: path.to_string(),
            message: format!(
                "contains {invalid} PEM block(s) but none parsed as valid X.509 certificates"
            ),
        });
    }
    if invalid > 0 {
        tracing::warn!(
            valid,
            invalid,
            path,
            "some certificates in CA bundle were unparseable; proceeding with the valid ones"
        );
    }

    let trust_store = TrustStore::empty().with_pem_certificate(pem);
    TlsContext::builder()
        .with_trust_store(trust_store)
        .build()
        .map_err(|e| CaBundleError::TlsContext(Box::new(e)))
}

/// Parse an `[s3]` sub-section from a profile as if it were a top-level profile.
///
/// `aws_config::profile::parser` validates but does not structurally expose
/// nested sub-sections like:
/// ```ignore
/// [profile myprofile]
/// s3 =
///   addressing_style = path
///   use_accelerate_endpoint = true
/// ```
/// Calling `profile.get("s3")` returns the raw multi-line string
/// `"\n  addressing_style = path\n  use_accelerate_endpoint = true"`.
///
/// This helper normalizes that string (strips leading whitespace per line)
/// and re-parses it through the SDK's own parser by wrapping it in a
/// synthetic `[default]` profile. Callers get structured key-value access
/// via the returned `HashMap`.
///
/// Returns `Ok(None)` if `raw_subsection` contains no `key = value` lines.
///
/// TODO: upstream this as structured sub-section access in `aws-config`,
/// matching botocore's `SectionConfigProvider`.
pub async fn parse_s3_subsection(
    raw_subsection: &str,
) -> Result<Option<std::collections::HashMap<String, String>>, SubsectionParseError> {
    use aws_config::profile::load;
    use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
    use aws_types::os_shim_internal::{Env, Fs};

    // Strip leading whitespace per line so the parser sees top-level keys,
    // not continuation lines.
    let normalized: String = raw_subsection
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if normalized.is_empty() {
        return Ok(None);
    }

    let synthetic = format!("[default]\n{normalized}\n");
    let files = EnvConfigFiles::builder()
        .include_default_config_file(false)
        .include_default_credentials_file(false)
        .with_contents(EnvConfigFileKind::Config, synthetic)
        .build();

    // Fs/Env are unused because we provided synthetic contents directly,
    // but the `load` signature requires them.
    let fs = Fs::from_map(std::collections::HashMap::<String, Vec<u8>>::new());
    let env = Env::from_slice(&[] as &[(&str, &str)]);

    let profile_set = load(&fs, &env, &files, None)
        .await
        .map_err(|e| SubsectionParseError(format!("{e}")))?;

    let profile = profile_set
        .get_profile("default")
        .ok_or_else(|| SubsectionParseError("synthetic profile vanished".to_string()))?;

    let mut out = std::collections::HashMap::new();
    for key in collect_property_keys(&normalized) {
        if let Some(value) = profile.get(&key) {
            out.insert(key, value.to_string());
        }
    }

    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

/// Extract property-name tokens from a normalized sub-section string.
/// `ProfileSet`/`Profile` doesn't expose an iterator of property names
/// from its public API, so we derive the key list from the input lines.
fn collect_property_keys(normalized: &str) -> Vec<String> {
    normalized
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                return None;
            }
            trimmed
                .split_once('=')
                .map(|(k, _)| k.trim().to_ascii_lowercase())
        })
        .collect()
}

/// Error parsing an `[s3]` sub-section.
#[derive(Debug, thiserror::Error)]
#[error("failed to parse [s3] sub-section: {0}")]
pub struct SubsectionParseError(String);

/// S3-specific configuration resolved from `[s3]` profile sub-section
/// and `AWS_S3_*` environment variables.
///
/// Priority (highest wins): env var > profile config > SDK default.
#[derive(Debug, Default)]
pub struct S3ConfigKeys {
    /// `addressing_style`: "path" or "virtual". Maps to `force_path_style(true)`.
    pub addressing_style: Option<String>,
    /// `use_accelerate_endpoint`: boolean. Maps to `accelerate(true)`.
    pub use_accelerate_endpoint: Option<bool>,
    /// `use_arn_region`: boolean. Env: `AWS_S3_USE_ARN_REGION`.
    pub use_arn_region: Option<bool>,
    /// `s3_disable_multiregion_access_points`: boolean.
    /// Env: `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS`.
    pub disable_multiregion_access_points: Option<bool>,
}

/// Valid values for `addressing_style` in `[s3]` config.
pub mod addressing_style {
    pub const PATH: &str = "path";
    pub const VIRTUAL: &str = "virtual";
    pub const AUTO: &str = "auto";
}

impl S3ConfigKeys {
    /// Apply these keys to an S3 config builder.
    pub fn apply(self, mut builder: aws_sdk_s3::config::Builder) -> aws_sdk_s3::config::Builder {
        if let Some(ref style) = self.addressing_style {
            if style == addressing_style::PATH {
                builder = builder.force_path_style(true);
            }
            // "virtual" and "auto" are the SDK default — no action needed.
            // Unknown values fall through (matches Python's behavior).
        }
        if let Some(accel) = self.use_accelerate_endpoint {
            builder = builder.accelerate(accel);
        }
        if let Some(arn) = self.use_arn_region {
            builder = builder.use_arn_region(arn);
        }
        if let Some(disable) = self.disable_multiregion_access_points {
            builder = builder.disable_multi_region_access_points(disable);
        }
        builder
    }
}

/// Load S3-specific config keys from the profile `[s3]` sub-section and
/// `AWS_S3_*` environment variables.
///
/// `profile_override` should be the `--profile` CLI flag value (if any).
/// When `None`, uses `AWS_PROFILE` or defaults to `"default"`.
pub async fn load_s3_config(profile_override: Option<&str>) -> S3ConfigKeys {
    let profile_keys = load_s3_profile_keys(profile_override).await;

    // Env vars take precedence over profile config.
    let use_arn_region = read_bool_env("AWS_S3_USE_ARN_REGION").or_else(|| {
        profile_keys
            .as_ref()
            .and_then(|m| parse_bool(m.get("use_arn_region")?))
    });

    let disable_multiregion_access_points =
        read_bool_env("AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS").or_else(|| {
            profile_keys
                .as_ref()
                .and_then(|m| parse_bool(m.get("s3_disable_multiregion_access_points")?))
        });

    let addressing_style = profile_keys
        .as_ref()
        .and_then(|m| m.get("addressing_style").cloned());

    let use_accelerate_endpoint = profile_keys
        .as_ref()
        .and_then(|m| parse_bool(m.get("use_accelerate_endpoint")?));

    S3ConfigKeys {
        addressing_style,
        use_accelerate_endpoint,
        use_arn_region,
        disable_multiregion_access_points,
    }
}

/// Load and parse the `[s3]` sub-section from the active profile.
async fn load_s3_profile_keys(
    profile_override: Option<&str>,
) -> Option<std::collections::HashMap<String, String>> {
    use aws_types::os_shim_internal::{Env, Fs};
    load_s3_profile_keys_from(profile_override, Fs::default(), Env::default()).await
}

/// Inner implementation accepting injectable Fs/Env for testability.
async fn load_s3_profile_keys_from(
    profile_override: Option<&str>,
    fs: aws_types::os_shim_internal::Fs,
    env: aws_types::os_shim_internal::Env,
) -> Option<std::collections::HashMap<String, String>> {
    use aws_config::profile::load;
    use aws_runtime::env_config::file::EnvConfigFiles;
    use std::borrow::Cow;

    let files = EnvConfigFiles::default();
    let override_cow = profile_override.map(|s| Cow::Owned(s.to_string()));

    let profile_set = match load(&fs, &env, &files, override_cow).await {
        Ok(ps) => ps,
        Err(e) => {
            tracing::debug!(error = %e, "failed to load profile; skipping [s3] config");
            return None;
        }
    };

    let profile_name = profile_override.unwrap_or(profile_set.selected_profile());
    let profile = profile_set.get_profile(profile_name)?;
    let raw_s3 = profile.get("s3")?;

    match parse_s3_subsection(raw_s3).await {
        Ok(keys) => keys,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse [s3] sub-section; ignoring");
            None
        }
    }
}

fn read_bool_env(key: &str) -> Option<bool> {
    parse_bool(&std::env::var(key).ok()?)
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::GlobalArgs;

    fn globals() -> GlobalArgs {
        GlobalArgs::default()
    }

    #[test]
    fn http_client_builds_default() {
        let g = globals();
        let _client = build_http_client(&g);
    }

    #[test]
    fn timeout_config_defaults_when_unset() {
        let g = globals();
        let tc = build_timeout_config(&g);
        let default_tc = TimeoutConfig::builder().build();
        assert_eq!(tc.connect_timeout(), default_tc.connect_timeout());
        assert_eq!(tc.read_timeout(), default_tc.read_timeout());
    }

    #[test]
    fn timeout_config_set_both() {
        let mut g = globals();
        g.cli_connect_timeout = Some(10);
        g.cli_read_timeout = Some(30);
        let tc = build_timeout_config(&g);
        assert_eq!(tc.connect_timeout(), Some(Duration::from_secs(10)));
        assert_eq!(tc.read_timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn timeout_config_set_connect_only() {
        let mut g = globals();
        g.cli_connect_timeout = Some(5);
        let tc = build_timeout_config(&g);
        assert_eq!(tc.connect_timeout(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn timeout_config_zero_means_disabled() {
        let mut g = globals();
        g.cli_connect_timeout = Some(0);
        g.cli_read_timeout = Some(0);
        let tc = build_timeout_config(&g);
        let default_tc = TimeoutConfig::builder().build();
        assert_eq!(tc.connect_timeout(), default_tc.connect_timeout());
        assert_eq!(tc.read_timeout(), default_tc.read_timeout());
    }

    #[test]
    fn timeout_config_mixed_zero_and_nonzero() {
        let mut g = globals();
        g.cli_connect_timeout = Some(0); // disabled
        g.cli_read_timeout = Some(60); // applied
        let tc = build_timeout_config(&g);
        assert_eq!(tc.read_timeout(), Some(Duration::from_secs(60)));
        let default_tc = TimeoutConfig::builder().build();
        assert_eq!(tc.connect_timeout(), default_tc.connect_timeout());
    }

    // -------------------------------------------------------------------------
    // --ca-bundle tests — ignored while the flag is rejected at arg-parse time.
    // When upstream TM exposes a TlsContext hook and we re-enable the flag,
    // drop the `#[ignore]` attributes.
    // -------------------------------------------------------------------------

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_missing_file_errors() {
        let err =
            build_ca_bundle_tls_context("/definitely/does/not/exist.pem").expect_err("io error");
        assert!(matches!(err, CaBundleError::Io { .. }), "got: {err:?}");
    }

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_invalid_pem_errors() {
        let tmp = std::env::temp_dir().join("s3cli-bogus-ca.pem");
        std::fs::write(
            &tmp,
            b"-----BEGIN CERTIFICATE-----\nnot valid base64 !!!\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let err =
            build_ca_bundle_tls_context(tmp.to_str().unwrap()).expect_err("parse error expected");
        assert!(matches!(err, CaBundleError::Parse { .. }), "got: {err:?}");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_empty_pem_errors() {
        let tmp = std::env::temp_dir().join("s3cli-empty-ca.pem");
        std::fs::write(&tmp, b"").unwrap();
        let err =
            build_ca_bundle_tls_context(tmp.to_str().unwrap()).expect_err("empty is rejected");
        assert!(matches!(err, CaBundleError::Parse { .. }), "got: {err:?}");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_non_pem_content_errors() {
        let tmp = std::env::temp_dir().join("s3cli-nonpem-ca.pem");
        std::fs::write(&tmp, b"this is not a certificate file\n").unwrap();
        let err =
            build_ca_bundle_tls_context(tmp.to_str().unwrap()).expect_err("non-PEM is rejected");
        assert!(matches!(err, CaBundleError::Parse { .. }), "got: {err:?}");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_valid_pem_but_not_x509_errors() {
        // Well-formed PEM (valid base64 inside CERTIFICATE markers) whose
        // decoded bytes are not a valid X.509 certificate — this used to
        // panic on a tokio worker thread inside smithy-rs's rustls provider.
        let tmp = std::env::temp_dir().join("s3cli-bad-x509.pem");
        std::fs::write(
            &tmp,
            b"-----BEGIN CERTIFICATE-----\n\
              MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAu1SU1LfVLPHCozMxH2Mo\n\
              4lgOEePzNm0tRgeLezV6ffAt0gunVTLw7onLRnrq0/IzW7yWR7QkrmBL7jTKEn5u\n\
              +qKhbwKfBstIs+bMY2Zkp18gnTxKLxoS2tFczGkPLPgizskuemMghRniWaoLcyeh\n\
              kd3qqGElvW/VDL5AaWTg0nLVkjRo9z+40RQzuVaE8AkAFmxZzow3x+VJYKdjykkJ\n\
              0iT9wCS0DRTXu269V264Vf/3jvredZiKRkgwlL9xNAwxXFg0x/XFw005UWVRIkdg\n\
              cKWTjpBP2dPwVZ4WWC+9aGVd+Gyn1o0CLelf4rEjGoXbAAEgAKBA8cy7pzH9gE9d\n\
              kQIDAQAB\n\
              -----END CERTIFICATE-----\n",
        )
        .unwrap();
        let err = build_ca_bundle_tls_context(tmp.to_str().unwrap())
            .expect_err("X.509-invalid PEM is rejected");
        assert!(matches!(err, CaBundleError::Parse { .. }), "got: {err:?}");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    #[ignore = "--ca-bundle rejected at arg-parse time; re-enable when upstream TM supports TlsContext passthrough"]
    fn ca_bundle_valid_pem_ok() {
        let path = if std::path::Path::new("/etc/ssl/cert.pem").exists() {
            "/etc/ssl/cert.pem"
        } else if std::path::Path::new("/etc/ssl/certs/ca-certificates.crt").exists() {
            "/etc/ssl/certs/ca-certificates.crt"
        } else {
            eprintln!("skipping ca_bundle_valid_pem_ok: no system bundle found");
            return;
        };
        let _ctx = build_ca_bundle_tls_context(path).expect("valid system bundle");
    }

    // --- [s3] sub-section re-parse ---

    #[tokio::test]
    async fn s3_subsection_basic_keys() {
        let raw = "\n  addressing_style = path\n  use_accelerate_endpoint = true\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("path")
        );
        assert_eq!(
            out.get("use_accelerate_endpoint").map(String::as_str),
            Some("true")
        );
    }

    #[tokio::test]
    async fn s3_subsection_no_indent_ok() {
        let raw = "addressing_style = virtual\nuse_arn_region = false\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("virtual")
        );
        assert_eq!(out.get("use_arn_region").map(String::as_str), Some("false"));
    }

    #[tokio::test]
    async fn s3_subsection_tabs_and_mixed_whitespace() {
        let raw = "\n\taddressing_style\t=\tpath\n    use_accelerate_endpoint =true\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("path")
        );
        assert_eq!(
            out.get("use_accelerate_endpoint").map(String::as_str),
            Some("true")
        );
    }

    #[tokio::test]
    async fn s3_subsection_empty_returns_none() {
        assert!(parse_s3_subsection("").await.unwrap().is_none());
        assert!(parse_s3_subsection("\n\n  \n").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn s3_subsection_comment_lines_ignored() {
        let raw = "# this is a comment\n  addressing_style = path\n  ; semicolon comment\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("path")
        );
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn s3_subsection_key_lowercased() {
        let raw = "  Addressing_Style = path\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert!(out.contains_key("addressing_style"));
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("path")
        );
    }

    #[tokio::test]
    async fn s3_subsection_value_preserves_case_and_spaces() {
        let raw = "  multipart_threshold = 64MB\n  multipart_chunksize = 16 MB\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("multipart_threshold").map(String::as_str),
            Some("64MB")
        );
        assert_eq!(
            out.get("multipart_chunksize").map(String::as_str),
            Some("16 MB")
        );
    }

    #[tokio::test]
    async fn s3_subsection_many_keys() {
        let raw = "\n\
            addressing_style = path\n\
            use_accelerate_endpoint = false\n\
            use_dualstack_endpoint = true\n\
            use_arn_region = true\n\
            s3_disable_multiregion_access_points = false\n\
            payload_signing_enabled = true\n\
            multipart_threshold = 64MB\n\
            multipart_chunksize = 16MB\n\
            max_concurrent_requests = 20\n\
            target_bandwidth = 1GB/s\n\
            preferred_transfer_client = auto\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(out.len(), 11);
        for k in [
            "addressing_style",
            "use_accelerate_endpoint",
            "use_dualstack_endpoint",
            "use_arn_region",
            "s3_disable_multiregion_access_points",
            "payload_signing_enabled",
            "multipart_threshold",
            "multipart_chunksize",
            "max_concurrent_requests",
            "target_bandwidth",
            "preferred_transfer_client",
        ] {
            assert!(out.contains_key(k), "missing {k}");
        }
    }

    #[tokio::test]
    async fn s3_subsection_duplicate_key_last_wins() {
        let raw = "  addressing_style = path\n  addressing_style = virtual\n";
        let out = parse_s3_subsection(raw).await.unwrap().unwrap();
        assert_eq!(
            out.get("addressing_style").map(String::as_str),
            Some("virtual")
        );
    }

    // --- S3ConfigKeys ---

    #[test]
    fn apply_path_style_builds_without_panic() {
        let keys = S3ConfigKeys {
            addressing_style: Some("path".to_string()),
            ..Default::default()
        };
        // Verifies the builder method is called without error.
        let _conf = keys.apply(aws_sdk_s3::config::Builder::new()).build();
    }

    #[test]
    fn apply_virtual_style_builds_without_panic() {
        let keys = S3ConfigKeys {
            addressing_style: Some("virtual".to_string()),
            ..Default::default()
        };
        let _conf = keys.apply(aws_sdk_s3::config::Builder::new()).build();
    }

    #[test]
    fn apply_all_keys_builds_without_panic() {
        let keys = S3ConfigKeys {
            addressing_style: Some("path".to_string()),
            use_accelerate_endpoint: Some(true),
            use_arn_region: Some(true),
            disable_multiregion_access_points: Some(true),
        };
        let _conf = keys.apply(aws_sdk_s3::config::Builder::new()).build();
    }

    #[test]
    fn apply_none_leaves_builder_unchanged() {
        let keys = S3ConfigKeys::default();
        let _conf = keys.apply(aws_sdk_s3::config::Builder::new()).build();
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("True"), Some(true));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("False"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool(""), None);
        assert_eq!(parse_bool("maybe"), None);
    }

    #[tokio::test]
    async fn load_s3_config_env_overrides_profile() {
        std::env::set_var("AWS_S3_USE_ARN_REGION", "true");
        let keys = load_s3_config(Some("nonexistent-profile-for-test")).await;
        std::env::remove_var("AWS_S3_USE_ARN_REGION");
        assert_eq!(keys.use_arn_region, Some(true));
    }

    #[tokio::test]
    async fn load_s3_config_env_disable_mrap() {
        std::env::set_var("AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS", "true");
        let keys = load_s3_config(Some("nonexistent-profile-for-test")).await;
        std::env::remove_var("AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS");
        assert_eq!(keys.disable_multiregion_access_points, Some(true));
    }

    #[tokio::test]
    async fn load_s3_config_missing_profile_returns_defaults() {
        let keys = load_s3_config(Some("nonexistent-profile-for-test")).await;
        assert!(keys.addressing_style.is_none());
        assert!(keys.use_accelerate_endpoint.is_none());
    }

    // --- Profile selection tests ---

    /// Helper: load S3 profile keys from a synthetic config file.
    async fn load_keys_from_config(
        config_content: &str,
        profile_override: Option<&str>,
        env_vars: &[(&str, &str)],
    ) -> Option<std::collections::HashMap<String, String>> {
        use aws_types::os_shim_internal::{Env, Fs};
        use std::collections::HashMap;

        let mut files = HashMap::new();
        files.insert(
            "~/.aws/config".to_string(),
            config_content.as_bytes().to_vec(),
        );
        let fs = Fs::from_map(files);
        let env = Env::from_slice(env_vars);
        load_s3_profile_keys_from(profile_override, fs, env).await
    }

    #[tokio::test]
    async fn profile_override_selects_correct_s3_section() {
        let config = "\
[default]
s3 =
  addressing_style = virtual

[profile custom]
s3 =
  addressing_style = path
  use_accelerate_endpoint = true
";
        // --profile custom → reads from [profile custom]
        let keys = load_keys_from_config(config, Some("custom"), &[])
            .await
            .unwrap();
        assert_eq!(
            keys.get("addressing_style").map(String::as_str),
            Some("path")
        );
        assert_eq!(
            keys.get("use_accelerate_endpoint").map(String::as_str),
            Some("true")
        );
    }

    #[tokio::test]
    async fn default_profile_used_when_no_override() {
        let config = "\
[default]
s3 =
  addressing_style = path

[profile other]
s3 =
  addressing_style = virtual
";
        // No override, no AWS_PROFILE → uses [default]
        let keys = load_keys_from_config(config, None, &[]).await.unwrap();
        assert_eq!(
            keys.get("addressing_style").map(String::as_str),
            Some("path")
        );
    }

    #[tokio::test]
    async fn aws_profile_env_selects_profile_when_no_override() {
        let config = "\
[default]
s3 =
  addressing_style = virtual

[profile from-env]
s3 =
  addressing_style = path
";
        // AWS_PROFILE=from-env, no --profile → uses [profile from-env]
        let keys = load_keys_from_config(config, None, &[("AWS_PROFILE", "from-env")])
            .await
            .unwrap();
        assert_eq!(
            keys.get("addressing_style").map(String::as_str),
            Some("path")
        );
    }

    #[tokio::test]
    async fn cli_override_takes_precedence_over_aws_profile_env() {
        let config = "\
[default]
s3 =
  addressing_style = virtual

[profile from-env]
s3 =
  addressing_style = path

[profile from-cli]
s3 =
  use_accelerate_endpoint = true
";
        // --profile from-cli wins over AWS_PROFILE=from-env
        let keys = load_keys_from_config(config, Some("from-cli"), &[("AWS_PROFILE", "from-env")])
            .await
            .unwrap();
        assert_eq!(
            keys.get("use_accelerate_endpoint").map(String::as_str),
            Some("true")
        );
        assert!(!keys.contains_key("addressing_style"));
    }

    #[tokio::test]
    async fn profile_without_s3_section_returns_none() {
        let config = "\
[default]
region = us-east-1

[profile no-s3]
region = eu-west-1
";
        let keys = load_keys_from_config(config, Some("no-s3"), &[]).await;
        assert!(keys.is_none());
    }
}
