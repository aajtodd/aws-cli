//! Test harness for managing backend resources and constructing test environments.
//!
//! The harness creates backends up front, leases them to tests (constructing
//! complete `TestEnv` instances), and automatically recycles backends when
//! `TestEnv` is dropped.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backend::TestBackend;
use crate::error::{Error, ErrorKind};
use crate::runner::{
    CredentialSource, Credentials, PlaceholderMap, collect_bucket_placeholders,
    generate_bucket_name, reverse_placeholders,
};
use crate::spec::TestSpec;

const BUCKET_PREFIX: &str = "s3-compat-";

/// Clean up stale buckets from previous test runs.
async fn cleanup_stale_buckets(backend: &TestBackend) -> Result<(), Error> {
    tracing::info!("cleaning up stale s3-compat-* buckets");
    let buckets = backend
        .client()
        .list_buckets()
        .send()
        .await
        .map_err(crate::error::from_kind(ErrorKind::Sdk))?;

    for bucket in buckets.buckets() {
        if let Some(name) = bucket.name()
            && name.starts_with(BUCKET_PREFIX)
        {
            tracing::debug!(%name, "deleting stale bucket");
            if let Err(e) = backend.empty_and_delete_bucket(name).await {
                tracing::warn!(%name, error = %e, "failed to clean stale bucket");
            }
        }
    }
    Ok(())
}

/// Configuration for the test harness.
pub struct HarnessConfig {
    /// Number of concurrent backends.
    pub concurrency: usize,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self { concurrency: 4 }
    }
}

struct TestHarnessInner {
    dirty: Mutex<VecDeque<TestBackend>>,
    notify: tokio::sync::Notify,
}

impl TestHarnessInner {
    fn return_backend(&self, backend: TestBackend) {
        self.dirty.lock().unwrap().push_back(backend);
        self.notify.notify_one();
    }
}

/// Manages shared test infrastructure and constructs test environments.
///
/// Backends are created during [`TestHarness::init`] and leased to tests via
/// [`TestHarness::lease`]. When a [`TestEnv`] is dropped, its backend is
/// automatically returned to the harness for reuse.
pub struct TestHarness {
    inner: Arc<TestHarnessInner>,
    cli_binary: PathBuf,
    default_credentials: Credentials,
    is_mock: bool,
    /// Unique ID for this test run. Used in prod bucket names to avoid collisions.
    run_id: String,
}

impl TestHarness {
    /// Create and initialize the harness.
    pub async fn init(
        config: HarnessConfig,
        cli_binary: PathBuf,
        credential_source: CredentialSource,
    ) -> Result<Self, Error> {
        Self::init_with_target(config, cli_binary, credential_source, true).await
    }

    /// Create and initialize the harness with explicit mock/prod selection.
    pub async fn init_with_target(
        config: HarnessConfig,
        cli_binary: PathBuf,
        credential_source: CredentialSource,
        is_mock: bool,
    ) -> Result<Self, Error> {
        let default_credentials = credential_source.resolve().await?;
        let mut backends = VecDeque::with_capacity(config.concurrency);

        let run_id = if is_mock {
            "mock".to_string()
        } else {
            format!(
                "{:08x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u32
            )
        };

        for _ in 0..config.concurrency {
            let backend = if is_mock {
                TestBackend::mock().await?
            } else {
                TestBackend::prod("us-east-1").await?
            };
            backends.push_back(backend);
        }

        // For prod: clean up stale buckets from previous runs
        if !is_mock && let Some(backend) = backends.front() {
            cleanup_stale_buckets(backend).await?;
        }

        Ok(Self {
            inner: Arc::new(TestHarnessInner {
                dirty: Mutex::new(backends),
                notify: tokio::sync::Notify::new(),
            }),
            cli_binary,
            default_credentials,
            is_mock,
            run_id,
        })
    }

    /// Lease a complete test environment for the given spec.
    ///
    /// Blocks if all backends are in use. The backend is reset before use.
    /// When the returned `TestEnv` is dropped, the backend is automatically
    /// returned to the harness.
    pub async fn lease(&self, spec: &TestSpec) -> Result<TestEnv, Error> {
        let backend = loop {
            let notified = self.inner.notify.notified();
            if let Some(b) = self.inner.dirty.lock().unwrap().pop_front() {
                break b;
            }
            notified.await;
        };

        backend.reset().await?;

        // Collect all bucket placeholders from the spec
        let mut placeholders = PlaceholderMap::new();
        for name in collect_bucket_placeholders(spec) {
            placeholders.insert(
                name.clone(),
                generate_bucket_name(&spec.test.name, &name, &self.run_id),
            );
        }
        // Always ensure {bucket} exists as a default
        if !placeholders.contains_key("{bucket}") {
            placeholders.insert(
                "{bucket}".into(),
                generate_bucket_name(&spec.test.name, "{bucket}", &self.run_id),
            );
        }

        let working_dir = tempfile::tempdir()
            .map_err(|e| Error::new(ErrorKind::Harness, e))?
            .keep();

        let reverse_placeholders = reverse_placeholders(&placeholders);

        Ok(TestEnv {
            backend: Some(backend),
            harness: Arc::clone(&self.inner),
            placeholders,
            reverse_placeholders,
            working_dir,
            cli_binary: self.cli_binary.clone(),
            credentials: self.default_credentials.clone(),
        })
    }

    /// Shut down all backends.
    ///
    /// All `TestEnv` instances must be dropped before calling this.
    pub async fn shutdown(self) -> Result<(), Error> {
        let backends: Vec<_> = self.inner.dirty.lock().unwrap().drain(..).collect();
        for backend in backends {
            backend.shutdown().await?;
        }
        Ok(())
    }

    /// Whether the harness is using mock backends.
    pub fn is_mock(&self) -> bool {
        self.is_mock
    }
}

/// A test environment leased from the harness. Owns all resources needed to run a spec.
///
/// When dropped, the backend is automatically returned to the harness for reuse.
/// The working directory is cleaned up on drop.
pub struct TestEnv {
    backend: Option<TestBackend>,
    harness: Arc<TestHarnessInner>,
    /// Resolved placeholder map for this test.
    pub placeholders: PlaceholderMap,
    /// Reverse map for output normalization.
    pub reverse_placeholders: PlaceholderMap,
    /// Working directory (temp dir for this test).
    pub working_dir: PathBuf,
    /// CLI binary path.
    pub cli_binary: PathBuf,
    /// Resolved credentials for CLI execution.
    pub credentials: Credentials,
}

impl TestEnv {
    /// Access the test backend.
    pub fn backend(&self) -> &TestBackend {
        self.backend.as_ref().expect("backend already returned")
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if std::env::var_os("COMPAT_KEEP_TEMPDIR").is_some() {
            tracing::debug!(path = %self.working_dir.display(), "keeping temp dir");
        } else {
            let _ = std::fs::remove_dir_all(&self.working_dir);
        }
        if let Some(backend) = self.backend.take() {
            self.harness.return_backend(backend);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::parse_spec;

    fn test_spec(name: &str) -> TestSpec {
        let toml = format!(
            r#"
[test]
name = "{name}"
description = "test"

[command]
args = ["hello"]

[expected]
exit_code = 0
"#
        );
        parse_spec(&toml).unwrap()
    }

    #[tokio::test]
    async fn test_harness_init_and_shutdown() {
        let harness = TestHarness::init(
            HarnessConfig::default(),
            PathBuf::from("/usr/bin/echo"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_lease_and_release() {
        let harness = TestHarness::init(
            HarnessConfig::default(),
            PathBuf::from("/usr/bin/echo"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();

        let spec = test_spec("basic_ls");
        let env = harness.lease(&spec).await.unwrap();
        assert!(env.placeholders.contains_key("{bucket}"));
        assert!(env.working_dir.exists());
        assert_eq!(env.cli_binary, PathBuf::from("/usr/bin/echo"));

        drop(env);
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_unique_placeholders() {
        let harness = TestHarness::init(
            HarnessConfig::default(),
            PathBuf::from("/usr/bin/echo"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();

        let spec_a = test_spec("spec_a");
        let spec_b = test_spec("spec_b");
        let env1 = harness.lease(&spec_a).await.unwrap();
        let env2 = harness.lease(&spec_b).await.unwrap();

        assert_ne!(
            env1.placeholders.get("{bucket}"),
            env2.placeholders.get("{bucket}"),
        );

        drop(env1);
        drop(env2);
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_release_resets_state() {
        let harness = TestHarness::init(
            HarnessConfig { concurrency: 1 },
            PathBuf::from("/usr/bin/echo"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();

        let spec = test_spec("reset_test");
        let env = harness.lease(&spec).await.unwrap();
        env.backend().create_bucket("test-bucket").await.unwrap();
        env.backend()
            .put_object("test-bucket", "key.txt", b"data".to_vec(), None, None, None)
            .await
            .unwrap();
        drop(env);

        let spec2 = test_spec("after_reset");
        let env2 = harness.lease(&spec2).await.unwrap();
        env2.backend().create_bucket("test-bucket").await.unwrap();
        let objects = env2
            .backend()
            .list_objects("test-bucket", None)
            .await
            .unwrap();
        assert!(objects.is_empty(), "state should be reset between leases");

        drop(env2);
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_multi_bucket_placeholders() {
        let harness = TestHarness::init(
            HarnessConfig::default(),
            PathBuf::from("/usr/bin/echo"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();

        let spec = parse_spec(
            r#"
[test]
name = "multi_bucket"
description = "test"

[[setup.buckets]]
name = "{source}"

[[setup.buckets]]
name = "{dest}"

[command]
args = ["hello"]

[expected]
exit_code = 0
"#,
        )
        .unwrap();

        let env = harness.lease(&spec).await.unwrap();
        assert!(env.placeholders.contains_key("{source}"));
        assert!(env.placeholders.contains_key("{dest}"));
        assert!(env.placeholders.contains_key("{bucket}"));
        // All three should have different resolved names
        let source = env.placeholders.get("{source}").unwrap();
        let dest = env.placeholders.get("{dest}").unwrap();
        let bucket = env.placeholders.get("{bucket}").unwrap();
        assert_ne!(source, dest);
        assert_ne!(source, bucket);
        drop(env);
        harness.shutdown().await.unwrap();
    }
}
