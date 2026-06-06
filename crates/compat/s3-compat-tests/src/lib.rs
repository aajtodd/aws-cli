use s3_compat_spec::harness::{HarnessConfig, TestHarness};
use s3_compat_spec::runner::{
    CredentialSource, RunMode, TestOutcome, check_platform, check_target, resolve_expected,
    run_spec,
};
use s3_compat_spec::spec::parse_spec;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Shared tokio runtime for all generated spec tests.
///
/// Per-test runtimes (`#[tokio::test]`) drop at the end of each test, which
/// aborts any tasks they spawned — including the mock servers' accept loops
/// held by the static `HARNESS`. Using a single shared runtime keeps those
/// tasks alive across the whole test binary run.
pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build shared tokio runtime")
    })
}

static HARNESS: tokio::sync::OnceCell<TestHarness> = tokio::sync::OnceCell::const_new();

async fn harness() -> &'static TestHarness {
    HARNESS
        .get_or_init(|| async {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_test_writer()
                .try_init()
                .ok();

            let cli_binary_raw =
                std::env::var("COMPAT_CLI_BINARY").expect("COMPAT_CLI_BINARY must be set");
            let cli_binary = if cli_binary_raw.contains('/') || cli_binary_raw.contains('\\') {
                // Relative or absolute path — resolve directly
                std::path::Path::new(&cli_binary_raw)
                    .canonicalize()
                    .unwrap_or_else(|e| {
                        panic!("CLI binary not found at path: {cli_binary_raw}: {e}")
                    })
            } else {
                // Bare name — search PATH
                which::which(&cli_binary_raw)
                    .unwrap_or_else(|_| panic!("CLI binary not found on PATH: {cli_binary_raw}"))
            };

            let is_mock = std::env::var("COMPAT_TARGET").as_deref() != Ok("prod");
            let credential_source = if is_mock {
                CredentialSource::mock_default()
            } else {
                CredentialSource::Resolve
            };

            TestHarness::init_with_target(
                HarnessConfig::default(),
                cli_binary,
                credential_source,
                is_mock,
            )
            .await
            .expect("failed to init test harness")
        })
        .await
}

/// Derive a unique spec ID from the file path.
///
/// Extracts the relative path under `specs/` (e.g., "commands/cp/upload_single_dryrun")
/// which is guaranteed unique across the corpus, unlike `test.name` which can collide
/// across commands (e.g., both cp and mv have "upload_single_dryrun").
fn spec_id_from_path(spec_path: &str) -> String {
    // Find "specs/" marker and take everything after it, minus extension
    if let Some(idx) = spec_path.find("specs/") {
        let rel = &spec_path[idx + "specs/".len()..];
        rel.strip_suffix(".toml").unwrap_or(rel).to_string()
    } else {
        // Fallback: use the file stem
        std::path::Path::new(spec_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}

/// Run a single spec file through the full lifecycle.
///
/// Called by generated tests. Handles lease and outcome assertion.
/// Skips if `COMPAT_CLI_BINARY` is not set.
pub async fn run_spec_file(spec_path: &str) {
    if std::env::var("COMPAT_CLI_BINARY").is_err() {
        println!("SKIPPED: COMPAT_CLI_BINARY not set");
        return;
    }

    let harness = harness().await;

    let toml_content = std::fs::read_to_string(spec_path)
        .unwrap_or_else(|e| panic!("failed to read spec {spec_path}: {e}"));
    let spec = parse_spec(&toml_content)
        .unwrap_or_else(|e| panic!("failed to parse spec {spec_path}: {e}"));

    if let Some(reason) = check_platform(&spec.test.platform) {
        println!("SKIPPED: {reason}");
        return;
    }
    if let Some(reason) = check_target(&spec.test.target, harness.is_mock()) {
        println!("SKIPPED: {reason}");
        return;
    }

    let env = harness
        .lease(&spec, &spec_id_from_path(spec_path))
        .await
        .unwrap_or_else(|e| panic!("failed to lease env for {spec_path}: {e}"));

    let expected = resolve_expected(&spec.expected, spec.deviation.as_ref());
    let mode = RunMode::from_env();
    let outcome = run_spec(
        &spec,
        &env,
        &expected,
        std::path::Path::new(spec_path),
        mode,
    )
    .await;

    // Drop env to return backend before potentially panicking
    drop(env);

    match outcome {
        Ok(TestOutcome::Pass) => {
            // If the spec has a deviation and the test passed against
            // deviation-merged expectations, report as Deviation not Pass.
            if let Some(dev) = &spec.deviation {
                println!("DEVIATION: {} ({})", dev.behavior, dev.rationale);
            }
        }
        Ok(TestOutcome::Fail { message }) => panic!("FAIL: {message}"),
        Ok(TestOutcome::Skipped { reason }) => println!("SKIPPED: {reason}"),
        Ok(TestOutcome::Deviation {
            behavior,
            rationale,
        }) => {
            println!("DEVIATION: {behavior} ({rationale})");
        }
        Err(e) => panic!("ERROR: {e}"),
    }
}

#[cfg(test)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
}
