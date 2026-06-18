//! Core types and logic for the spec runner.
//!
//! Provides test outcome tracking, placeholder resolution for bucket names,
//! platform/target filtering, and setup functions that translate spec types
//! into actual state (seeding mock objects, creating local files, writing
//! AWS config, building CLI environment maps).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::assertions::{AssertionResult, assert_exit_code, assert_output};
use crate::backend::TestBackend;
use crate::error::{Error, ErrorKind};
use crate::executor::CliExecutor;
use crate::harness::TestEnv;
use crate::spec::{
    ConfigSetup, Deviation, Expected, LocalFile, OutputAssertion, S3Object, TestSpec, TestTarget,
};

/// Resolved credentials for CLI execution.
///
/// The runner resolves credentials once at startup (via `CredentialSource`),
/// then passes them to every test as static values.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// How the runner obtains credentials.
#[derive(Debug, Clone)]
pub enum CredentialSource {
    /// Hardcoded credentials (default for mock backend).
    Static(Credentials),
    /// Resolve via the default SDK credential chain at runner startup.
    /// The resolved credentials are passed to tests as static values.
    Resolve,
}

impl CredentialSource {
    /// Default credential source for mock backend.
    ///
    /// Uses the same credentials the mock server expects for SigV4 validation.
    pub fn mock_default() -> Self {
        CredentialSource::Static(Credentials {
            access_key_id: "mock-akid".into(),
            secret_access_key: "mock-secret".into(),
            session_token: None,
        })
    }

    /// Resolve credentials from this source.
    ///
    /// For `Static`, returns the credentials directly.
    /// For `Resolve`, uses the default SDK credential chain.
    pub async fn resolve(&self) -> Result<Credentials, Error> {
        match self {
            CredentialSource::Static(creds) => Ok(creds.clone()),
            CredentialSource::Resolve => {
                use aws_credential_types::provider::ProvideCredentials;
                let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .load()
                    .await;
                let provider = config.credentials_provider().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidSpec, "no credentials provider found")
                })?;
                let creds = provider.provide_credentials().await.map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidSpec,
                        format!("credential resolution failed: {e}"),
                    )
                })?;
                Ok(Credentials {
                    access_key_id: creds.access_key_id().to_string(),
                    secret_access_key: creds.secret_access_key().to_string(),
                    session_token: creds.session_token().map(|s| s.to_string()),
                })
            }
        }
    }
}

/// Controls what happens with CLI output after execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Assert output against expectations.
    Test,
    /// Print output to stdout. No assertions.
    Probe,
    /// Write golden files from output. No assertions.
    Capture,
}

impl RunMode {
    /// Parse from the `COMPAT_MODE` environment variable.
    pub fn from_env() -> Self {
        match std::env::var("COMPAT_MODE").as_deref() {
            Ok("probe") => RunMode::Probe,
            Ok("capture") => RunMode::Capture,
            _ => RunMode::Test,
        }
    }
}

/// Result of running a single spec.
#[derive(Debug)]
pub enum TestOutcome {
    /// All assertions passed.
    Pass,
    /// One or more assertions failed.
    Fail { message: String },
    /// Test passed but behavior intentionally deviates from baseline.
    Deviation { behavior: String, rationale: String },
    /// Test was skipped (platform mismatch, target mismatch, etc.).
    Skipped { reason: String },
}

/// Summary of a test run.
#[derive(Debug, Default)]
pub struct RunReport {
    pub passed: usize,
    pub failed: usize,
    pub deviations: usize,
    pub skipped: usize,
}

impl RunReport {
    /// Record a single test outcome into the report.
    pub fn record(&mut self, outcome: &TestOutcome) {
        match outcome {
            TestOutcome::Pass => self.passed += 1,
            TestOutcome::Fail { .. } => self.failed += 1,
            TestOutcome::Deviation { .. } => self.deviations += 1,
            TestOutcome::Skipped { .. } => self.skipped += 1,
        }
    }

    /// Total number of tests recorded.
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.deviations + self.skipped
    }
}

/// Maps placeholder patterns (e.g. `"{bucket}"`) to resolved values.
pub type PlaceholderMap = HashMap<String, String>;

/// Replace all placeholder occurrences in a string.
pub fn resolve_string(s: &str, placeholders: &PlaceholderMap) -> String {
    let mut result = s.to_string();
    for (placeholder, value) in placeholders {
        result = result.replace(placeholder, value);
    }
    result
}

/// Replace placeholders in a list of strings.
pub fn resolve_strings(strings: &[String], placeholders: &PlaceholderMap) -> Vec<String> {
    strings
        .iter()
        .map(|s| resolve_string(s, placeholders))
        .collect()
}

/// Build the reverse map for output normalization.
///
/// Maps resolved values back to their placeholder patterns so CLI output
/// can be normalized before assertion comparison.
pub fn reverse_placeholders(placeholders: &PlaceholderMap) -> PlaceholderMap {
    placeholders
        .iter()
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect()
}

/// Normalize output by replacing resolved values with their placeholders.
///
/// Longer resolved values are replaced first to prevent partial matches
/// when one value is a prefix of another.
pub fn normalize_output(output: &str, reverse_map: &PlaceholderMap) -> String {
    let mut result = output.to_string();
    let mut entries: Vec<_> = reverse_map.iter().collect();
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (resolved, placeholder) in entries {
        result = result.replace(resolved, placeholder);
    }
    result
}

/// Generate a deterministic bucket name from a spec ID, placeholder, and run ID.
///
/// Bucket names must be 3-63 characters, lowercase, alphanumeric + hyphens only.
/// For mock, `run_id` is "mock" (deterministic). For prod, it's a unique
/// hex string per run to avoid collisions and enable stale cleanup.
///
/// If the name would exceed 63 chars, we truncate the spec portion and append
/// a short hash to maintain uniqueness.
pub fn generate_bucket_name(spec_id: &str, placeholder: &str, run_id: &str) -> String {
    let clean_placeholder = placeholder.trim_matches('{').trim_matches('}');
    let clean_spec = spec_id
        .replace("::", "-")
        .replace(['/', '_'], "-")
        .to_lowercase();

    let name = format!("s3-compat-{run_id}-{clean_placeholder}-{clean_spec}");
    if name.len() <= 63 {
        return name;
    }

    // Truncate spec portion and add 8-char hash suffix for uniqueness
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    clean_spec.hash(&mut hasher);
    let hash = format!("{:08x}", hasher.finish() as u32);

    let prefix = format!("s3-compat-{run_id}-{clean_placeholder}-");
    let max_spec_len = 63 - prefix.len() - 9; // 9 = "-" + 8 hash chars
    let truncated = &clean_spec[..max_spec_len];
    format!("{prefix}{truncated}-{hash}")
}

/// Current platform as a static string.
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// Check if the current platform matches the spec's platform requirements.
///
/// Returns `None` if the spec should run, `Some(reason)` if it should be skipped.
pub fn check_platform(platforms: &[String]) -> Option<String> {
    if platforms.is_empty() {
        return None;
    }
    let current = current_platform();
    if platforms.iter().any(|p| p == current) {
        None
    } else {
        Some(format!("requires {platforms:?}, running on {current}"))
    }
}

/// Check if the spec's target matches the current backend.
///
/// Returns `None` if the spec should run, `Some(reason)` if it should be skipped.
pub fn check_target(target: &TestTarget, is_mock: bool) -> Option<String> {
    match (target, is_mock) {
        (TestTarget::Both, _) => None,
        (TestTarget::MockOnly, true) => None,
        (TestTarget::MockOnly, false) => Some("spec is mock_only, running against prod".into()),
        (TestTarget::ProdOnly, false) => None,
        (TestTarget::ProdOnly, true) => Some("spec is prod_only, running against mock".into()),
    }
}

// ---------------------------------------------------------------------------
// Expectation resolution
// ---------------------------------------------------------------------------

/// Resolve the final expected values from base + platform overrides + deviation.
///
/// Platform overrides are applied first (OS-specific baseline behavior),
/// then deviation overrides on top (intentional differences from baseline).
/// Fields not overridden fall through from the base.
pub fn resolve_expected(base: &Expected, deviation: Option<&Deviation>) -> Expected {
    let platform = current_platform();

    // Start with base values
    let mut exit_code = base.exit_code;
    let mut stdout = base.stdout.clone();
    let mut stderr = base.stderr.clone();

    // Apply platform overrides
    if let Some(platform_override) = base.platform.get(platform) {
        if let Some(ec) = platform_override.exit_code {
            exit_code = ec;
        }
        if platform_override.stdout.is_some() {
            stdout = platform_override.stdout.clone();
        }
        if platform_override.stderr.is_some() {
            stderr = platform_override.stderr.clone();
        }
    }

    // Apply deviation overrides
    if let Some(dev_expected) = deviation.and_then(|d| d.expected.as_ref()) {
        if let Some(ec) = dev_expected.exit_code {
            exit_code = ec;
        }
        if dev_expected.stdout.is_some() {
            stdout = dev_expected.stdout.clone();
        }
        if dev_expected.stderr.is_some() {
            stderr = dev_expected.stderr.clone();
        }
    }

    Expected {
        exit_code,
        verify_integrity: base.verify_integrity,
        stdout,
        stderr,
        objects: base.objects.clone(),
        files: base.files.clone(),
        user_agent: base.user_agent.clone(),
        platform: HashMap::new(), // already resolved
    }
}

// ---------------------------------------------------------------------------
// Spec execution
// ---------------------------------------------------------------------------

/// Run a single test spec: setup → execute → normalize → assert.
///
/// The caller provides resolved expectations (after platform/deviation merging).
/// This function does not interpret deviations — it returns `Pass` or `Fail`.
/// The caller is responsible for wrapping the outcome as `Deviation` if appropriate.
pub async fn run_spec(
    spec: &TestSpec,
    env: &TestEnv,
    expected: &Expected,
    spec_path: &Path,
    mode: RunMode,
) -> Result<TestOutcome, Error> {
    let is_mock = env.backend().mock_control().is_some();
    let _span = tracing::info_span!("spec", name = %spec.test.name).entered();
    tracing::info!(
        mode = ?mode,
        backend = if is_mock { "mock" } else { "prod" },
        "running spec"
    );
    for (placeholder, resolved) in &env.placeholders {
        tracing::debug!(%placeholder, %resolved, "placeholder");
    }

    // 1. Platform/target filtering
    if let Some(reason) = check_platform(&spec.test.platform) {
        tracing::info!(reason, "skipped");
        return Ok(TestOutcome::Skipped { reason });
    }
    if let Some(reason) = check_target(&spec.test.target, is_mock) {
        tracing::info!(reason, "skipped");
        return Ok(TestOutcome::Skipped { reason });
    }

    // 2. Setup
    if let Some(setup) = &spec.setup {
        tracing::debug!(
            buckets = setup.buckets.len(),
            objects = setup.objects.len(),
            files = setup.files.len(),
            "seeding state"
        );
        create_buckets(env.backend(), setup, &env.placeholders).await?;
        seed_objects(env.backend(), &setup.objects, &env.placeholders).await?;
        create_local_files(&setup.files, &env.working_dir, &env.placeholders)?;
    }
    let config = spec.setup.as_ref().and_then(|s| s.config.as_ref());
    let config_path = write_aws_config(config, &env.working_dir, env.backend().endpoint_url())?;
    if tracing::enabled!(tracing::Level::TRACE)
        && let Ok(contents) = std::fs::read_to_string(&config_path)
    {
        tracing::trace!(path = %config_path.display(), contents, "aws config file");
    }

    // 3. Build env + execute CLI
    let region = config.map(|c| c.region.as_str()).unwrap_or("us-east-1");
    let config_env = config.map(|c| &c.env).cloned().unwrap_or_default();
    let cli_env = build_env(env, &config_path, region, &spec.command.env, &config_env);
    for (k, v) in &cli_env {
        tracing::trace!(key = %k, value = %v, "cli env var");
    }

    let executor = CliExecutor::new(&env.cli_binary, Duration::from_secs(30));
    let args = resolve_strings(&spec.command.args, &env.placeholders);
    tracing::debug!(
        cli = %env.cli_binary.display(),
        ?args,
        endpoint = ?env.backend().endpoint_url(),
        working_dir = %env.working_dir.display(),
        "executing"
    );
    let output = executor
        .execute(
            &args,
            &cli_env,
            &env.working_dir,
            spec.command.stdin.as_deref(),
            spec.command.timeout.map(Duration::from_secs),
        )
        .await?;

    tracing::debug!(
        exit_code = output.exit_code,
        stdout_len = output.stdout.len(),
        stderr_len = output.stderr.len(),
        duration_ms = output.duration.as_millis() as u64,
        "execution complete"
    );
    if !output.stdout.is_empty() {
        tracing::trace!(raw_stdout = ?output.stdout, "cli raw stdout (pre-normalization)");
    }
    if !output.stderr.is_empty() {
        tracing::trace!(raw_stderr = ?output.stderr, "cli raw stderr (pre-normalization)");
    }

    // 4. Normalize output
    let stdout = normalize_output(&output.stdout, &env.reverse_placeholders);
    let stderr = normalize_output(&output.stderr, &env.reverse_placeholders);
    if !stdout.is_empty() {
        tracing::trace!(stdout = ?stdout, "normalized stdout (what assertions compare against)");
    }
    if !stderr.is_empty() {
        tracing::trace!(stderr = ?stderr, "normalized stderr (what assertions compare against)");
    }

    // 5. Handle mode
    match mode {
        RunMode::Probe => {
            use crate::assertions::terminal_lines;
            use std::fmt::Write;
            let mut buf = String::new();
            let write_stream = |buf: &mut String, label: &str, raw: &str, normalized: &str| {
                let lines = terminal_lines(normalized);
                let _ = writeln!(buf, "{label} terminal-lines ({}):", lines.len());
                if lines.is_empty() {
                    let _ = writeln!(buf, "  (none)");
                } else {
                    for (i, line) in lines.iter().enumerate() {
                        let _ = writeln!(buf, "  [{}] {line:?}", i + 1);
                    }
                }
                let _ = writeln!(buf, "{label} raw:        {raw:?}");
                let _ = writeln!(buf, "{label} normalized: {normalized:?}");
            };
            let _ = writeln!(buf, "--- probe: {} ---", spec.test.name);
            let _ = writeln!(buf, "exit_code: {}", output.exit_code);
            write_stream(&mut buf, "stdout", &output.stdout, &stdout);
            write_stream(&mut buf, "stderr", &output.stderr, &stderr);
            probe_object_state_into(env, &mut buf).await;
            // Single write to keep this spec's probe output contiguous when
            // multiple specs run in parallel.
            print!("{buf}");
            return Ok(TestOutcome::Pass);
        }
        RunMode::Capture => {
            use crate::golden::{golden_path, write_golden};
            if !stdout.is_empty() {
                write_golden(&golden_path(spec_path, "stdout"), &stdout)?;
            }
            if !stderr.is_empty() {
                write_golden(&golden_path(spec_path, "stderr"), &stderr)?;
            }
            println!("CAPTURED: {}", spec_path.display());
            return Ok(TestOutcome::Pass);
        }
        RunMode::Test => {} // fall through to assertions
    }

    // 6. Assert (test mode only)
    tracing::debug!("asserting expectations");
    let mut failures = Vec::new();

    // Check golden files exist before asserting (if either stream uses golden mode)
    let uses_golden = matches!(expected.stdout.as_ref(), Some(OutputAssertion::Golden))
        || matches!(expected.stderr.as_ref(), Some(OutputAssertion::Golden));
    if uses_golden {
        use crate::golden::check_golden_files_exist;
        check_golden_files_exist(spec_path)?;
    }

    tracing::debug!(
        expected = expected.exit_code,
        actual = output.exit_code,
        "asserting exit_code"
    );
    if let AssertionResult::Fail { message } =
        assert_exit_code(output.exit_code, expected.exit_code)
    {
        failures.push(format!("exit_code: {message}"));
    }

    if let Some(assertion) = &expected.stdout {
        tracing::debug!(expected = ?assertion, actual = ?stdout, "asserting stdout");
        let result = match assertion {
            OutputAssertion::Golden => {
                use crate::golden::{assert_golden, golden_path};
                let path = golden_path(spec_path, "stdout");
                tracing::debug!(golden_file = %path.display(), "comparing against golden file");
                assert_golden(&stdout, &path, !is_mock)?
            }
            other => assert_output(&stdout, other),
        };
        if let AssertionResult::Fail { message } = &result {
            tracing::debug!(message, "stdout assertion FAILED");
            failures.push(format!("stdout: {message}"));
        } else {
            tracing::debug!("stdout assertion passed");
        }
    }

    if let Some(assertion) = &expected.stderr {
        tracing::debug!(expected = ?assertion, actual = ?stderr, "asserting stderr");
        let result = match assertion {
            OutputAssertion::Golden => {
                use crate::golden::{assert_golden, golden_path};
                let path = golden_path(spec_path, "stderr");
                tracing::debug!(golden_file = %path.display(), "comparing against golden file");
                assert_golden(&stderr, &path, !is_mock)?
            }
            other => assert_output(&stderr, other),
        };
        if let AssertionResult::Fail { message } = &result {
            tracing::debug!(message, "stderr assertion FAILED");
            failures.push(format!("stderr: {message}"));
        } else {
            tracing::debug!("stderr assertion passed");
        }
    }

    // Object assertions
    if !expected.objects.is_empty() {
        tracing::debug!(count = expected.objects.len(), "asserting objects");
        for exp_obj in &expected.objects {
            let bucket = crate::runner::resolve_string(&exp_obj.bucket, &env.placeholders);
            let key = crate::runner::resolve_string(&exp_obj.key, &env.placeholders);
            tracing::debug!(%bucket, %key, "checking object");

            let fetched = env
                .backend()
                .fetch_object_for_assertion(&bucket, &key)
                .await?;

            let (head, body) = match fetched {
                Some((h, b)) => (Some(h), Some(b)),
                None => (None, None),
            };

            let results =
                crate::assertions::object::assert_object(exp_obj, head.as_ref(), body.as_deref());
            for result in results {
                if let crate::assertions::AssertionResult::Fail { message } = result {
                    failures.push(message);
                }
            }

            // Integrity check: if verify_integrity is on and the object exists,
            // find the expected content from setup and verify the fetched body matches.
            if expected.verify_integrity
                && exp_obj.exists
                && let Some(actual_body) = &body
            {
                let expected_content =
                    find_expected_content(spec, &exp_obj.key, &env.placeholders, &env.working_dir);
                if let Some(expected_bytes) = expected_content {
                    let label = format!("{}/{}", exp_obj.bucket, exp_obj.key);
                    let result = crate::assertions::object::verify_integrity(
                        &expected_bytes,
                        actual_body,
                        &label,
                    );
                    if let crate::assertions::AssertionResult::Fail { message } = result {
                        failures.push(message);
                    }
                }
            }
        }
    }

    // File assertions
    if !expected.files.is_empty() {
        tracing::debug!(count = expected.files.len(), "asserting files");
        for exp_file in &expected.files {
            tracing::debug!(path = %exp_file.path, "checking file");
            let results = crate::assertions::file::assert_file(exp_file, &env.working_dir);
            for result in results {
                if let crate::assertions::AssertionResult::Fail { message } = result {
                    failures.push(message);
                }
            }

            // Integrity check for downloaded files
            if expected.verify_integrity && exp_file.exists {
                let file_path = env.working_dir.join(&exp_file.path);
                if let Ok(actual_bytes) = std::fs::read(&file_path) {
                    let expected_content =
                        find_expected_content_for_file(spec, &exp_file.path, &env.placeholders);
                    if let Some(expected_bytes) = expected_content {
                        let result = crate::assertions::object::verify_integrity(
                            &expected_bytes,
                            &actual_bytes,
                            &exp_file.path,
                        );
                        if let crate::assertions::AssertionResult::Fail { message } = result {
                            failures.push(message);
                        }
                    }
                }
            }
        }
    }

    // TODO: expected.user_agent assertions (mock-only, inspect request log)

    if failures.is_empty() {
        tracing::info!("all assertions passed");
        Ok(TestOutcome::Pass)
    } else {
        tracing::info!(count = failures.len(), "assertions failed");
        Ok(TestOutcome::Fail {
            message: failures.join("\n"),
        })
    }
}

// ---------------------------------------------------------------------------
// Setup phase: translate spec types into actual state
// ---------------------------------------------------------------------------

/// Collect all unique bucket placeholder names from a spec.
///
/// Sources: `setup.buckets[].name`, `setup.objects[].bucket`.
pub fn collect_bucket_placeholders(spec: &TestSpec) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some(setup) = &spec.setup {
        for b in &setup.buckets {
            if !names.contains(&b.name) {
                names.push(b.name.clone());
            }
        }
        for obj in &setup.objects {
            if !names.contains(&obj.bucket) {
                names.push(obj.bucket.clone());
            }
        }
    }
    names
}

/// Create buckets from the spec's setup.
///
/// If `setup.buckets` is non-empty, creates those. Otherwise, collects
/// unique buckets from `setup.objects` and creates them implicitly.
pub async fn create_buckets(
    backend: &TestBackend,
    setup: &crate::spec::SetupState,
    placeholders: &PlaceholderMap,
) -> Result<(), Error> {
    let bucket_names: Vec<String> = if !setup.buckets.is_empty() {
        setup
            .buckets
            .iter()
            .map(|b| resolve_string(&b.name, placeholders))
            .collect()
    } else {
        let mut names = Vec::new();
        for obj in &setup.objects {
            let resolved = resolve_string(&obj.bucket, placeholders);
            if !names.contains(&resolved) {
                names.push(resolved);
            }
        }
        names
    };

    for bucket in &bucket_names {
        tracing::debug!(%bucket, "creating bucket");
        backend
            .create_bucket(bucket)
            .await
            .map_err(|e| Error::new(ErrorKind::Seed, format!("create bucket {bucket}: {e}")))?;
    }
    Ok(())
}

/// Seed S3 objects from the spec's `setup.objects` into the backend.
///
/// TODO: respect `upload_method` field. When `upload_method = "multipart"`,
/// seed via CreateMultipartUpload + UploadPart + CompleteMultipartUpload
/// instead of PutObject. This produces different ETags (e.g. "abc-3") and
/// exercises different CLI code paths for download/checksum verification.
pub async fn seed_objects(
    backend: &TestBackend,
    objects: &[S3Object],
    placeholders: &PlaceholderMap,
) -> Result<(), Error> {
    for obj in objects {
        let bucket = resolve_string(&obj.bucket, placeholders);
        let key = resolve_string(&obj.key, placeholders);

        let ctx = format!("{bucket}/{key}");
        let size = obj.size.or(obj.content.as_ref().map(|c| c.len() as u64));
        tracing::debug!(%bucket, %key, ?size, "seeding object");

        let body = if let Some(content) = &obj.content {
            content.as_bytes().to_vec()
        } else if let Some(size) = obj.size {
            generate_deterministic_content(&key, size)
        } else {
            Vec::new()
        };

        let last_modified = if let Some(s) = &obj.last_modified {
            let dt = aws_smithy_types::DateTime::from_str(
                s,
                aws_smithy_types::date_time::Format::DateTime,
            )
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidSpec,
                    format!("invalid last_modified '{s}': {e}"),
                )
            })?;
            Some(SystemTime::try_from(dt).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidSpec,
                    format!("last_modified '{s}' out of range: {e}"),
                )
            })?)
        } else {
            None
        };

        backend
            .put_object(
                &bucket,
                &key,
                body,
                obj.content_type.as_deref(),
                if obj.metadata.is_empty() {
                    None
                } else {
                    Some(obj.metadata.clone())
                },
                last_modified,
            )
            .await
            .map_err(|e| Error::new(ErrorKind::Seed, format!("{ctx}: {e}")))?;
    }
    Ok(())
}

/// Generate deterministic content of the given size, seeded from the key name.
/// Same key + size always produces the same bytes.
fn generate_deterministic_content(key: &str, size: u64) -> Vec<u8> {
    let seed: u8 = key.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
    (0..size).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// Find expected content for an S3 object by looking up setup files (upload case)
/// or setup objects (copy/sync case).
fn find_expected_content(
    spec: &TestSpec,
    key: &str,
    placeholders: &PlaceholderMap,
    working_dir: &Path,
) -> Option<Vec<u8>> {
    let setup = spec.setup.as_ref()?;
    // Check setup.files — the CLI uploaded a local file to this key
    for file in &setup.files {
        let resolved_path = resolve_string(&file.path, placeholders);
        // Heuristic: if the key matches the file path (or its basename), use this file's content
        if key == resolved_path
            || key.ends_with(&format!("/{resolved_path}"))
            || key == resolved_path.rsplit('/').next().unwrap_or("")
        {
            if let Some(content) = &file.content {
                return Some(content.as_bytes().to_vec());
            }
            if let Some(size) = file.size {
                return Some(generate_deterministic_content(&resolved_path, size));
            }
            // File exists on disk from setup
            let path = working_dir.join(&resolved_path);
            return std::fs::read(&path).ok();
        }
    }
    // Check setup.objects — the object was seeded directly
    for obj in &setup.objects {
        let obj_key = resolve_string(&obj.key, placeholders);
        if key == obj_key {
            if let Some(content) = &obj.content {
                return Some(content.as_bytes().to_vec());
            }
            if let Some(size) = obj.size {
                return Some(generate_deterministic_content(&obj_key, size));
            }
        }
    }
    None
}

/// Find expected content for a downloaded file by looking up setup objects.
fn find_expected_content_for_file(
    spec: &TestSpec,
    path: &str,
    placeholders: &PlaceholderMap,
) -> Option<Vec<u8>> {
    let setup = spec.setup.as_ref()?;
    for obj in &setup.objects {
        let obj_key = resolve_string(&obj.key, placeholders);
        // Heuristic: if the file path matches the object key (or its basename)
        if path == obj_key
            || path.ends_with(&format!("/{obj_key}"))
            || path == obj_key.rsplit('/').next().unwrap_or("")
        {
            if let Some(content) = &obj.content {
                return Some(content.as_bytes().to_vec());
            }
            if let Some(size) = obj.size {
                return Some(generate_deterministic_content(&obj_key, size));
            }
        }
    }
    None
}

/// Create local files from the spec's `setup.files` in the working directory.
pub fn create_local_files(
    files: &[LocalFile],
    working_dir: &Path,
    placeholders: &PlaceholderMap,
) -> Result<(), Error> {
    for file in files {
        if !file.platform.is_empty() && check_platform(&file.platform).is_some() {
            continue;
        }

        let path = working_dir.join(resolve_string(&file.path, placeholders));

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::new(ErrorKind::LocalFile, format!("{}: {e}", path.display()))
            })?;
        }

        if let Some(target) = &file.symlink_to {
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &path).map_err(|e| {
                Error::new(
                    ErrorKind::LocalFile,
                    format!("symlink {}: {e}", path.display()),
                )
            })?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(target, &path).map_err(|e| {
                Error::new(
                    ErrorKind::LocalFile,
                    format!("symlink {}: {e}", path.display()),
                )
            })?;
            continue;
        }

        let content = if let Some(c) = &file.content {
            c.as_bytes().to_vec()
        } else if let Some(size) = file.size {
            generate_deterministic_content(&file.path, size)
        } else {
            Vec::new()
        };

        std::fs::write(&path, &content)
            .map_err(|e| Error::new(ErrorKind::LocalFile, format!("{}: {e}", path.display())))?;

        if let Some(s) = &file.last_modified {
            let dt = aws_smithy_types::DateTime::from_str(
                s,
                aws_smithy_types::date_time::Format::DateTime,
            )
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidSpec,
                    format!("invalid last_modified '{s}': {e}"),
                )
            })?;
            let mtime = SystemTime::try_from(dt).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidSpec,
                    format!("last_modified '{s}' out of range: {e}"),
                )
            })?;
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(mtime))
                .map_err(|e| {
                    Error::new(
                        ErrorKind::LocalFile,
                        format!("set mtime {}: {e}", path.display()),
                    )
                })?;
        }

        #[cfg(unix)]
        if let Some(perms) = &file.permissions {
            use std::os::unix::fs::PermissionsExt;
            let mode = u32::from_str_radix(perms, 8).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidSpec,
                    format!("invalid permissions {perms}: {e}"),
                )
            })?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).map_err(
                |e| {
                    Error::new(
                        ErrorKind::LocalFile,
                        format!("set permissions {}: {e}", path.display()),
                    )
                },
            )?;
        }
    }
    Ok(())
}

/// Write AWS config file to the working directory.
///
/// Returns the path to the config file. Credentials are not written —
/// they are injected via environment variables by `build_env`.
pub fn write_aws_config(
    config: Option<&ConfigSetup>,
    working_dir: &Path,
    endpoint_url: Option<&str>,
) -> Result<PathBuf, Error> {
    let aws_dir = working_dir.join(".aws");
    std::fs::create_dir_all(&aws_dir)
        .map_err(|e| Error::new(ErrorKind::Config, format!("create .aws dir: {e}")))?;

    let config_path = aws_dir.join("config");

    let region = config.map(|c| c.region.as_str()).unwrap_or("us-east-1");

    let mut config_content = format!("[default]\nregion = {region}\n");

    if let Some(url) = endpoint_url {
        config_content.push_str(&format!("endpoint_url = {url}\n"));
    }

    if let Some(cfg) = config
        && !cfg.s3.is_empty()
    {
        config_content.push_str("s3 =\n");
        for (k, v) in &cfg.s3 {
            config_content.push_str(&format!("  {k} = {v}\n"));
        }
    }

    std::fs::write(&config_path, &config_content)
        .map_err(|e| Error::new(ErrorKind::Config, format!("write config: {e}")))?;

    Ok(config_path)
}

/// Build the environment map for CLI execution.
pub fn build_env(
    env: &TestEnv,
    config_path: &Path,
    region: &str,
    spec_env: &HashMap<String, String>,
    config_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    if let Some(url) = env.backend().endpoint_url() {
        result.insert("AWS_ENDPOINT_URL".into(), url.into());
    }
    result.insert(
        "AWS_ACCESS_KEY_ID".into(),
        env.credentials.access_key_id.clone(),
    );
    result.insert(
        "AWS_SECRET_ACCESS_KEY".into(),
        env.credentials.secret_access_key.clone(),
    );
    if let Some(token) = &env.credentials.session_token {
        result.insert("AWS_SESSION_TOKEN".into(), token.clone());
    }
    result.insert("AWS_DEFAULT_REGION".into(), region.into());
    result.insert(
        "AWS_CONFIG_FILE".into(),
        config_path.to_string_lossy().into(),
    );
    result.insert("HOME".into(), env.working_dir.to_string_lossy().into());
    result.insert("TZ".into(), "UTC".into());
    result.insert("LANG".into(), "en_US.UTF-8".into());
    result.insert("LC_ALL".into(), "en_US.UTF-8".into());
    result.insert("TERM".into(), "dumb".into());
    result.insert("NO_COLOR".into(), "1".into());
    result.insert("AWS_PAGER".into(), String::new());

    // Config-level env overrides
    for (k, v) in config_env {
        result.insert(k.clone(), v.clone());
    }
    // Command-level env overrides (highest priority)
    for (k, v) in spec_env {
        result.insert(k.clone(), v.clone());
    }

    result
}

// ---------------------------------------------------------------------------
// Spec discovery
// ---------------------------------------------------------------------------

/// Recursively discover all `.toml` spec files under a root directory.
///
/// Returns paths sorted for deterministic ordering.
pub fn discover_specs(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut specs = Vec::new();
    discover_specs_recursive(root, &mut specs)?;
    specs.sort();
    Ok(specs)
}

fn discover_specs_recursive(dir: &Path, specs: &mut Vec<PathBuf>) -> Result<(), Error> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::new(ErrorKind::Io, e))?;
        let path = entry.path();
        if path.is_dir() {
            discover_specs_recursive(&path, specs)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            specs.push(path);
        }
    }
    Ok(())
}

/// Probe-only: dump post-command S3 state for every bucket the spec touched.
///
/// Writes into the supplied buffer instead of printing directly so the
/// entire probe output for a single spec can be emitted in one `print!`,
/// keeping output contiguous when specs run in parallel.
async fn probe_object_state_into(env: &TestEnv, buf: &mut String) {
    use std::fmt::Write;
    let mut buckets: Vec<(String, String)> = env
        .placeholders
        .iter()
        .map(|(p, r)| (p.clone(), r.clone()))
        .collect();
    buckets.sort();
    for (placeholder, resolved) in &buckets {
        let objects = match env.backend().list_objects(resolved, None).await {
            Ok(o) => o,
            Err(e) => {
                let _ = writeln!(
                    buf,
                    "--- bucket {placeholder} ({resolved}): list failed: {e} ---"
                );
                continue;
            }
        };
        let _ = writeln!(
            buf,
            "--- bucket {placeholder} ({resolved}): {} object(s) ---",
            objects.len()
        );
        for entry in objects {
            let head = env
                .backend()
                .fetch_object_for_assertion(resolved, &entry.key)
                .await;
            match head {
                Ok(Some((h, _body))) => {
                    let _ = writeln!(buf, "  key: {:?}", entry.key);
                    let _ = writeln!(buf, "    content_type:           {:?}", h.content_type());
                    let _ = writeln!(buf, "    content_length:         {:?}", h.content_length());
                    let _ = writeln!(buf, "    e_tag:                  {:?}", h.e_tag());
                    let _ = writeln!(buf, "    storage_class:          {:?}", h.storage_class());
                    let _ = writeln!(
                        buf,
                        "    server_side_encryption: {:?}",
                        h.server_side_encryption()
                    );
                    let _ = writeln!(buf, "    checksum_type:          {:?}", h.checksum_type());
                    let _ = writeln!(buf, "    checksum_crc32:         {:?}", h.checksum_crc32());
                    let _ = writeln!(
                        buf,
                        "    checksum_crc32c:        {:?}",
                        h.checksum_crc32_c()
                    );
                    let _ = writeln!(
                        buf,
                        "    checksum_crc64nvme:     {:?}",
                        h.checksum_crc64_nvme()
                    );
                    let _ = writeln!(buf, "    checksum_sha1:          {:?}", h.checksum_sha1());
                    let _ = writeln!(buf, "    checksum_sha256:        {:?}", h.checksum_sha256());
                    let _ = writeln!(buf, "    cache_control:          {:?}", h.cache_control());
                    let _ = writeln!(
                        buf,
                        "    content_encoding:       {:?}",
                        h.content_encoding()
                    );
                    let _ = writeln!(
                        buf,
                        "    content_disposition:    {:?}",
                        h.content_disposition()
                    );
                    let _ = writeln!(
                        buf,
                        "    content_language:       {:?}",
                        h.content_language()
                    );
                    let metadata = h.metadata().cloned().unwrap_or_default();
                    if metadata.is_empty() {
                        let _ = writeln!(buf, "    metadata:               (empty)");
                    } else {
                        let _ = writeln!(buf, "    metadata ({} entries):", metadata.len());
                        let mut entries: Vec<_> = metadata.iter().collect();
                        entries.sort();
                        for (k, v) in entries {
                            let _ = writeln!(buf, "      {k:?} = {v:?}");
                        }
                    }
                }
                Ok(None) => {
                    let _ = writeln!(buf, "  key: {:?} — HEAD returned None", entry.key);
                }
                Err(e) => {
                    let _ = writeln!(buf, "  key: {:?} — HEAD failed: {e}", entry.key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{OutputAssertion, PlatformExpectedOverride, parse_spec};

    fn placeholders() -> PlaceholderMap {
        HashMap::from([
            ("{bucket}".into(), "my-bucket-abc".into()),
            ("{bucket-2}".into(), "my-bucket-def".into()),
        ])
    }

    #[test]
    fn test_resolve_string() {
        let pm = placeholders();
        assert_eq!(
            resolve_string("s3://{bucket}/key", &pm),
            "s3://my-bucket-abc/key"
        );
    }

    #[test]
    fn test_resolve_strings() {
        let pm = placeholders();
        let input = vec!["s3://{bucket}/a".into(), "{bucket-2}/b".into()];
        let resolved = resolve_strings(&input, &pm);
        assert_eq!(resolved, vec!["s3://my-bucket-abc/a", "my-bucket-def/b"]);
    }

    #[test]
    fn test_resolve_no_placeholders() {
        let pm = placeholders();
        assert_eq!(
            resolve_string("no-placeholders-here", &pm),
            "no-placeholders-here"
        );
    }

    #[test]
    fn test_normalize_output() {
        let pm = placeholders();
        let reverse = reverse_placeholders(&pm);
        let output = "upload: file to s3://my-bucket-abc/key\n";
        assert_eq!(
            normalize_output(output, &reverse),
            "upload: file to s3://{bucket}/key\n"
        );
    }

    #[test]
    fn test_normalize_longest_first() {
        let pm = HashMap::from([
            ("{short}".into(), "s3-compat-bucket".into()),
            ("{long}".into(), "s3-compat-bucket-foo".into()),
        ]);
        let reverse = reverse_placeholders(&pm);
        // The longer value "s3-compat-bucket-foo" must be replaced first,
        // otherwise "s3-compat-bucket" would partially match it.
        let output = "a]s3-compat-bucket-foo[b]s3-compat-bucket[c";
        let normalized = normalize_output(output, &reverse);
        assert_eq!(normalized, "a]{long}[b]{short}[c");
    }

    #[test]
    fn test_generate_bucket_name() {
        let name = generate_bucket_name("ls/basic_listing", "{bucket}", "mock");
        assert_eq!(name, "s3-compat-mock-bucket-ls-basic-listing");

        let name2 = generate_bucket_name("ls/basic_listing", "{bucket-2}", "mock");
        assert_eq!(name2, "s3-compat-mock-bucket-2-ls-basic-listing");
    }

    #[test]
    fn test_check_platform_empty() {
        assert!(check_platform(&[]).is_none());
    }

    #[test]
    fn test_check_platform_current_os() {
        let current = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "windows"
        };
        assert!(check_platform(&[current.into()]).is_none());
        // A platform we're definitely not on:
        let other = if current == "linux" {
            "windows"
        } else {
            "linux"
        };
        let reason = check_platform(&[other.into()]);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains(current));
    }

    #[test]
    fn test_check_target() {
        assert!(check_target(&TestTarget::Both, true).is_none());
        assert!(check_target(&TestTarget::Both, false).is_none());
        assert!(check_target(&TestTarget::MockOnly, true).is_none());
        assert!(check_target(&TestTarget::MockOnly, false).is_some());
        assert!(check_target(&TestTarget::ProdOnly, false).is_none());
        assert!(check_target(&TestTarget::ProdOnly, true).is_some());
    }

    #[test]
    fn test_run_report() {
        let mut report = RunReport::default();
        report.record(&TestOutcome::Pass);
        report.record(&TestOutcome::Pass);
        report.record(&TestOutcome::Fail {
            message: "bad".into(),
        });
        report.record(&TestOutcome::Deviation {
            behavior: "x".into(),
            rationale: "y".into(),
        });
        report.record(&TestOutcome::Skipped { reason: "z".into() });

        assert_eq!(report.passed, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.deviations, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.total(), 5);
    }

    // --- Setup phase tests ---

    #[test]
    fn test_generate_deterministic_content() {
        let a1 = generate_deterministic_content("file-a.txt", 64);
        let a2 = generate_deterministic_content("file-a.txt", 64);
        let b = generate_deterministic_content("file-b.txt", 64);

        assert_eq!(a1.len(), 64);
        assert_eq!(a1, a2, "same key+size must produce identical bytes");
        assert_ne!(a1, b, "different keys must produce different bytes");
    }

    #[test]
    fn test_create_local_files_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![LocalFile {
            path: "hello.txt".into(),
            content: Some("hello world".into()),
            size: None,
            symlink_to: None,
            permissions: None,
            permissions_windows: None,
            last_modified: None,
            platform: vec![],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("hello.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_create_local_files_with_size() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![LocalFile {
            path: "sized.bin".into(),
            content: None,
            size: Some(256),
            symlink_to: None,
            permissions: None,
            permissions_windows: None,
            last_modified: None,
            platform: vec![],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();
        let data = std::fs::read(dir.path().join("sized.bin")).unwrap();
        assert_eq!(data.len(), 256);
    }

    #[test]
    fn test_create_local_files_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![LocalFile {
            path: "a/b/c/file.txt".into(),
            content: Some("nested".into()),
            size: None,
            symlink_to: None,
            permissions: None,
            permissions_windows: None,
            last_modified: None,
            platform: vec![],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a/b/c/file.txt")).unwrap(),
            "nested"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_create_local_files_symlink() {
        let dir = tempfile::tempdir().unwrap();
        // Create the target file first
        std::fs::write(dir.path().join("target.txt"), "target content").unwrap();
        let files = vec![LocalFile {
            path: "link.txt".into(),
            content: None,
            size: None,
            symlink_to: Some("target.txt".into()),
            permissions: None,
            permissions_windows: None,
            last_modified: None,
            platform: vec![],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();
        let link_path = dir.path().join("link.txt");
        assert!(
            link_path
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn test_write_aws_config() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ConfigSetup {
            region: "eu-west-1".into(),
            s3: HashMap::from([("multipart_threshold".into(), "8MB".into())]),
            env: HashMap::new(),
        };
        let config_path =
            write_aws_config(Some(&cfg), dir.path(), Some("http://localhost:5000")).unwrap();

        let config = std::fs::read_to_string(&config_path).unwrap();
        assert!(config.contains("region = eu-west-1"));
        assert!(config.contains("endpoint_url = http://localhost:5000"));
        assert!(config.contains("multipart_threshold = 8MB"));
    }

    #[tokio::test]
    async fn test_build_env() {
        let harness = crate::harness::TestHarness::init(
            crate::harness::HarnessConfig::default(),
            PathBuf::from("/usr/bin/aws"),
            CredentialSource::mock_default(),
        )
        .await
        .unwrap();
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "build_env_test"
description = "test"
[command]
args = ["hello"]
[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let env = harness.lease(&spec, &spec.test.name).await.unwrap();
        let config_path = env.working_dir.join("config");
        let config_env = HashMap::from([("CFG_KEY".into(), "cfg_val".into())]);
        let spec_env = HashMap::from([
            ("SPEC_KEY".into(), "spec_val".into()),
            ("CFG_KEY".into(), "overridden".into()),
        ]);

        let cli_env = build_env(&env, &config_path, "us-west-2", &spec_env, &config_env);

        assert!(
            cli_env
                .get("AWS_ENDPOINT_URL")
                .unwrap()
                .starts_with("http://127.0.0.1:"),
        );
        assert_eq!(cli_env.get("AWS_DEFAULT_REGION").unwrap(), "us-west-2");
        assert_eq!(cli_env.get("AWS_ACCESS_KEY_ID").unwrap(), "mock-akid");
        assert_eq!(cli_env.get("AWS_SECRET_ACCESS_KEY").unwrap(), "mock-secret");
        assert_eq!(cli_env.get("NO_COLOR").unwrap(), "1");
        assert_eq!(cli_env.get("SPEC_KEY").unwrap(), "spec_val");
        // spec_env overrides config_env
        assert_eq!(cli_env.get("CFG_KEY").unwrap(), "overridden");
        drop(env);
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_build_env_with_session_token() {
        let creds = CredentialSource::Static(Credentials {
            access_key_id: "AKID".into(),
            secret_access_key: "SECRET".into(),
            session_token: Some("TOKEN123".into()),
        });
        let harness = crate::harness::TestHarness::init(
            crate::harness::HarnessConfig { concurrency: 1 },
            PathBuf::from("/usr/bin/echo"),
            creds,
        )
        .await
        .unwrap();
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "session_token_test"
description = "test"
[command]
args = ["hello"]
[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let env = harness.lease(&spec, &spec.test.name).await.unwrap();
        let config_path = env.working_dir.join("config");

        let cli_env = build_env(
            &env,
            &config_path,
            "us-east-1",
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(cli_env.get("AWS_SESSION_TOKEN").unwrap(), "TOKEN123");
        assert_eq!(cli_env.get("AWS_ACCESS_KEY_ID").unwrap(), "AKID");
        drop(env);
        harness.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_seed_objects_with_content_type() {
        let backend = TestBackend::mock().await.unwrap();
        let pm = HashMap::from([("{bucket}".into(), "ct-test-bucket".into())]);
        backend.create_bucket("ct-test-bucket").await.unwrap();
        let objects = vec![S3Object {
            bucket: "{bucket}".into(),
            key: "image.png".into(),
            content: Some("fake-png".into()),
            content_type: Some("image/png".into()),
            size: None,
            last_modified: None,
            metadata: HashMap::new(),
            storage_class: None,
            tags: HashMap::new(),
            checksum_algorithm: None,
            upload_method: None,
            acl: None,
            sse: None,
            deleted: false,
        }];

        seed_objects(&backend, &objects, &pm).await.unwrap();

        let obj = backend
            .get_object("ct-test-bucket", "image.png")
            .await
            .unwrap()
            .expect("object should exist");
        assert_eq!(obj.body.as_ref(), b"fake-png");
        assert_eq!(obj.content_type.as_deref(), Some("image/png"));
        backend.shutdown().await.unwrap();
    }

    #[test]
    fn test_write_aws_config_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = write_aws_config(None, dir.path(), None).unwrap();

        let config = std::fs::read_to_string(&config_path).unwrap();
        assert!(config.contains("region = us-east-1"));
        assert!(!config.contains("endpoint_url"));
    }

    #[test]
    fn test_write_aws_config_no_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ConfigSetup {
            region: "ap-southeast-1".into(),
            s3: HashMap::from([("multipart_threshold".into(), "64MB".into())]),
            env: HashMap::new(),
        };
        let config_path = write_aws_config(Some(&cfg), dir.path(), None).unwrap();

        let config = std::fs::read_to_string(&config_path).unwrap();
        assert!(config.contains("region = ap-southeast-1"));
        assert!(config.contains("multipart_threshold = 64MB"));
        assert!(!config.contains("endpoint_url"));
    }

    #[test]
    fn test_create_local_files_platform_skip() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![LocalFile {
            path: "skipped.txt".into(),
            content: Some("should not exist".into()),
            size: None,
            symlink_to: None,
            permissions: None,
            permissions_windows: None,
            last_modified: None,
            platform: vec!["nonexistent_os".into()],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();
        assert!(!dir.path().join("skipped.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_create_local_files_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let files = vec![LocalFile {
            path: "readonly.txt".into(),
            content: Some("read only".into()),
            size: None,
            symlink_to: None,
            permissions: Some("0444".into()),
            permissions_windows: None,
            last_modified: None,
            platform: vec![],
        }];
        create_local_files(&files, dir.path(), &HashMap::new()).unwrap();

        let meta = std::fs::metadata(dir.path().join("readonly.txt")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o444);
    }

    #[tokio::test]
    async fn test_seed_objects() {
        let backend = TestBackend::mock().await.unwrap();
        let pm = HashMap::from([("{bucket}".into(), "seed-test-bucket".into())]);
        backend.create_bucket("seed-test-bucket").await.unwrap();
        let objects = vec![
            S3Object {
                bucket: "{bucket}".into(),
                key: "alpha.txt".into(),
                size: None,
                content: Some("alpha-content".into()),
                content_type: None,
                last_modified: None,
                metadata: HashMap::new(),
                storage_class: None,
                tags: HashMap::new(),
                checksum_algorithm: None,
                upload_method: None,
                acl: None,
                sse: None,
                deleted: false,
            },
            S3Object {
                bucket: "{bucket}".into(),
                key: "beta.bin".into(),
                size: Some(32),
                content: None,
                content_type: None,
                last_modified: None,
                metadata: HashMap::new(),
                storage_class: None,
                tags: HashMap::new(),
                checksum_algorithm: None,
                upload_method: None,
                acl: None,
                sse: None,
                deleted: false,
            },
        ];

        seed_objects(&backend, &objects, &pm).await.unwrap();

        let alpha = backend
            .get_object("seed-test-bucket", "alpha.txt")
            .await
            .unwrap()
            .expect("alpha should exist");
        assert_eq!(alpha.body.as_ref(), b"alpha-content");

        let beta = backend
            .get_object("seed-test-bucket", "beta.bin")
            .await
            .unwrap()
            .expect("beta should exist");
        assert_eq!(beta.body.len(), 32);
        assert_eq!(
            beta.body.as_ref(),
            &generate_deterministic_content("beta.bin", 32)
        );

        backend.shutdown().await.unwrap();
    }

    #[test]
    fn test_resolve_expected_no_overrides() {
        let base = Expected {
            exit_code: 0,
            verify_integrity: true,
            stdout: Some(OutputAssertion::Exact("hello\n".into())),
            stderr: None,
            objects: vec![],
            files: vec![],
            user_agent: None,
            platform: HashMap::new(),
        };
        let resolved = resolve_expected(&base, None);
        assert_eq!(resolved.exit_code, 0);
        assert_eq!(
            resolved.stdout,
            Some(OutputAssertion::Exact("hello\n".into()))
        );
        assert!(resolved.stderr.is_none());
    }

    #[test]
    fn test_resolve_expected_with_deviation() {
        let base = Expected {
            exit_code: 0,
            verify_integrity: true,
            stdout: Some(OutputAssertion::Exact("baseline\n".into())),
            stderr: None,
            objects: vec![],
            files: vec![],
            user_agent: None,
            platform: HashMap::new(),
        };
        let deviation = Deviation {
            behavior: "test".into(),
            rationale: "test".into(),
            tracking_id: None,
            expected: Some(crate::spec::DeviationExpected {
                exit_code: Some(1),
                stdout: Some(OutputAssertion::Exact("new cli\n".into())),
                stderr: None,
            }),
        };
        let resolved = resolve_expected(&base, Some(&deviation));
        assert_eq!(resolved.exit_code, 1);
        assert_eq!(
            resolved.stdout,
            Some(OutputAssertion::Exact("new cli\n".into()))
        );
        // stderr falls through from base
        assert!(resolved.stderr.is_none());
    }

    #[test]
    fn test_resolve_expected_deviation_partial_override() {
        let base = Expected {
            exit_code: 0,
            verify_integrity: true,
            stdout: Some(OutputAssertion::Exact("baseline\n".into())),
            stderr: Some(OutputAssertion::Exact("warning\n".into())),
            objects: vec![],
            files: vec![],
            user_agent: None,
            platform: HashMap::new(),
        };
        let deviation = Deviation {
            behavior: "test".into(),
            rationale: "test".into(),
            tracking_id: None,
            expected: Some(crate::spec::DeviationExpected {
                exit_code: None,
                stdout: None,
                stderr: Some(OutputAssertion::Contains(vec!["new warning".into()])),
            }),
        };
        let resolved = resolve_expected(&base, Some(&deviation));
        // exit_code and stdout fall through
        assert_eq!(resolved.exit_code, 0);
        assert_eq!(
            resolved.stdout,
            Some(OutputAssertion::Exact("baseline\n".into()))
        );
        // stderr overridden
        assert_eq!(
            resolved.stderr,
            Some(OutputAssertion::Contains(vec!["new warning".into()]))
        );
    }

    #[test]
    fn test_resolve_expected_platform_override() {
        let mut platform = HashMap::new();
        platform.insert(
            current_platform().to_string(),
            PlatformExpectedOverride {
                exit_code: Some(2),
                stdout: Some(OutputAssertion::Exact("platform specific\n".into())),
                stderr: None,
            },
        );
        let base = Expected {
            exit_code: 0,
            verify_integrity: true,
            stdout: Some(OutputAssertion::Exact("generic\n".into())),
            stderr: None,
            objects: vec![],
            files: vec![],
            user_agent: None,
            platform,
        };
        let resolved = resolve_expected(&base, None);
        assert_eq!(resolved.exit_code, 2);
        assert_eq!(
            resolved.stdout,
            Some(OutputAssertion::Exact("platform specific\n".into()))
        );
    }

    #[test]
    fn test_resolve_expected_platform_then_deviation() {
        // Platform override sets exit_code=2 and platform-specific stdout.
        // Deviation overrides stdout again but not exit_code.
        // Result: exit_code from platform (2), stdout from deviation.
        let mut platform = HashMap::new();
        platform.insert(
            current_platform().to_string(),
            PlatformExpectedOverride {
                exit_code: Some(2),
                stdout: Some(OutputAssertion::Exact("platform\n".into())),
                stderr: None,
            },
        );
        let base = Expected {
            exit_code: 0,
            verify_integrity: true,
            stdout: Some(OutputAssertion::Exact("generic\n".into())),
            stderr: None,
            objects: vec![],
            files: vec![],
            user_agent: None,
            platform,
        };
        let deviation = Deviation {
            behavior: "test".into(),
            rationale: "test".into(),
            tracking_id: None,
            expected: Some(crate::spec::DeviationExpected {
                exit_code: None,
                stdout: Some(OutputAssertion::Exact("deviated\n".into())),
                stderr: None,
            }),
        };
        let resolved = resolve_expected(&base, Some(&deviation));
        // exit_code: base=0, platform override=2, deviation=None → 2
        assert_eq!(resolved.exit_code, 2);
        // stdout: base="generic", platform="platform", deviation="deviated" → "deviated"
        assert_eq!(
            resolved.stdout,
            Some(OutputAssertion::Exact("deviated\n".into()))
        );
    }

    #[test]
    fn test_discover_specs() {
        let dir = tempfile::tempdir().unwrap();
        let specs_dir = dir.path().join("commands").join("ls");
        std::fs::create_dir_all(&specs_dir).unwrap();
        std::fs::write(specs_dir.join("basic.toml"), "# spec").unwrap();
        std::fs::write(specs_dir.join("recursive.toml"), "# spec").unwrap();
        // Non-toml file should be ignored
        std::fs::write(specs_dir.join("readme.md"), "# not a spec").unwrap();

        let found = discover_specs(dir.path()).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("basic.toml"));
        assert!(found[1].ends_with("recursive.toml"));
    }

    #[test]
    fn test_discover_specs_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let found = discover_specs(dir.path()).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn test_run_mode_default_is_test() {
        // When COMPAT_MODE is unset, from_env returns Test
        assert_eq!(RunMode::from_env(), RunMode::Test);
    }

    #[test]
    fn test_collect_bucket_placeholders_from_buckets() {
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "test"
description = "test"

[[setup.buckets]]
name = "{source}"

[[setup.buckets]]
name = "{dest}"

[command]
args = ["s3", "sync", "s3://{source}/", "s3://{dest}/"]

[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let names = collect_bucket_placeholders(&spec);
        assert_eq!(names, vec!["{source}".to_string(), "{dest}".to_string()]);
    }

    #[test]
    fn test_collect_bucket_placeholders_from_objects() {
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "test"
description = "test"

[[setup.objects]]
bucket = "{bucket}"
key = "a.txt"
content = "a"

[[setup.objects]]
bucket = "{bucket-2}"
key = "b.txt"
content = "b"

[command]
args = ["hello"]

[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let names = collect_bucket_placeholders(&spec);
        assert_eq!(
            names,
            vec!["{bucket}".to_string(), "{bucket-2}".to_string()]
        );
    }

    #[test]
    fn test_collect_bucket_placeholders_deduplicates() {
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "test"
description = "test"

[[setup.objects]]
key = "a.txt"
content = "a"

[[setup.objects]]
key = "b.txt"
content = "b"

[command]
args = ["hello"]

[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let names = collect_bucket_placeholders(&spec);
        // Both objects use default {bucket}, should appear once
        assert_eq!(names, vec!["{bucket}".to_string()]);
    }

    #[test]
    fn test_collect_bucket_placeholders_empty_setup() {
        let spec = crate::spec::parse_spec(
            r#"
[test]
name = "test"
description = "test"

[command]
args = ["hello"]

[expected]
exit_code = 0
"#,
        )
        .unwrap();
        let names = collect_bucket_placeholders(&spec);
        assert!(names.is_empty());
    }

    #[test]
    fn find_expected_content_from_setup_file_by_content() {
        let toml = r#"
[test]
name = "upload"
description = "test"
tags = ["cp"]

[[setup.files]]
path = "hello.txt"
content = "hello world"

[command]
args = ["s3", "cp", "hello.txt", "s3://{bucket}/hello.txt"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let dir = tempfile::tempdir().unwrap();
        let result = find_expected_content(&spec, "hello.txt", &pm, dir.path());
        assert_eq!(result, Some(b"hello world".to_vec()));
    }

    #[test]
    fn find_expected_content_from_setup_file_by_size() {
        let toml = r#"
[test]
name = "upload"
description = "test"
tags = ["cp"]

[[setup.files]]
path = "data.bin"
size = 10

[command]
args = ["s3", "cp", "data.bin", "s3://{bucket}/data.bin"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let dir = tempfile::tempdir().unwrap();
        let result = find_expected_content(&spec, "data.bin", &pm, dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 10);
    }

    #[test]
    fn find_expected_content_from_setup_object() {
        let toml = r#"
[test]
name = "copy"
description = "test"
tags = ["cp"]

[[setup.objects]]
key = "source.txt"
content = "source data"

[command]
args = ["s3", "cp", "s3://{bucket}/source.txt", "dest.txt"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let dir = tempfile::tempdir().unwrap();
        let result = find_expected_content(&spec, "source.txt", &pm, dir.path());
        assert_eq!(result, Some(b"source data".to_vec()));
    }

    #[test]
    fn find_expected_content_no_match() {
        let toml = r#"
[test]
name = "test"
description = "test"
tags = ["cp"]

[[setup.files]]
path = "other.txt"
content = "other"

[command]
args = ["s3", "cp", "other.txt", "s3://{bucket}/other.txt"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let dir = tempfile::tempdir().unwrap();
        let result = find_expected_content(&spec, "nonexistent.txt", &pm, dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn find_expected_content_no_setup() {
        let toml = r#"
[test]
name = "test"
description = "test"
tags = ["ls"]

[command]
args = ["s3", "ls"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let dir = tempfile::tempdir().unwrap();
        let result = find_expected_content(&spec, "anything", &pm, dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn find_expected_content_for_file_from_setup_object() {
        let toml = r#"
[test]
name = "download"
description = "test"
tags = ["cp"]

[[setup.objects]]
key = "remote.txt"
content = "remote data"

[command]
args = ["s3", "cp", "s3://{bucket}/remote.txt", "remote.txt"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let result = find_expected_content_for_file(&spec, "remote.txt", &pm);
        assert_eq!(result, Some(b"remote data".to_vec()));
    }

    #[test]
    fn find_expected_content_for_file_no_match() {
        let toml = r#"
[test]
name = "download"
description = "test"
tags = ["cp"]

[[setup.objects]]
key = "other.txt"
content = "other"

[command]
args = ["s3", "cp", "s3://{bucket}/other.txt", "other.txt"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let result = find_expected_content_for_file(&spec, "nonexistent.txt", &pm);
        assert!(result.is_none());
    }

    #[test]
    fn find_expected_content_for_file_by_size() {
        let toml = r#"
[test]
name = "download"
description = "test"
tags = ["cp"]

[[setup.objects]]
key = "big.bin"
size = 256

[command]
args = ["s3", "cp", "s3://{bucket}/big.bin", "big.bin"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        let pm = PlaceholderMap::new();
        let result = find_expected_content_for_file(&spec, "big.bin", &pm);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 256);
    }
}
