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
// See bosun.md "Upstream Contributions".

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
/// matching botocore's `SectionConfigProvider`. See bosun.md
/// §Upstream Contributions. Until then we work around by round-tripping
/// through the parser.
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
}
