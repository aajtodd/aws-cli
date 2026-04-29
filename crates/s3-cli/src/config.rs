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
}
