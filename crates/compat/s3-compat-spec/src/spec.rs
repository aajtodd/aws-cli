use serde::Deserialize;
use std::collections::HashMap;

/// Top-level spec from a single TOML file.
#[derive(Debug, Deserialize)]
pub struct TestSpec {
    pub test: TestMetadata,
    pub setup: Option<SetupState>,
    pub command: CommandSpec,
    pub expected: Expected,
    pub server: Option<ServerConfig>,
    pub deviation: Option<Deviation>,
    pub source: Option<Source>,
}

#[derive(Debug, Deserialize)]
pub struct TestMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub platform: Vec<String>,
    #[serde(default)]
    pub target: TestTarget,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TestTarget {
    MockOnly,
    ProdOnly,
    #[default]
    Both,
}

#[derive(Debug, Deserialize)]
pub struct SetupState {
    #[serde(default)]
    pub buckets: Vec<BucketSetup>,
    #[serde(default)]
    pub objects: Vec<S3Object>,
    #[serde(default)]
    pub files: Vec<LocalFile>,
    pub config: Option<ConfigSetup>,
    pub fixture_dir: Option<String>,
    pub generate: Option<GenerateSetup>,
}

/// A bucket to create during test setup.
#[derive(Clone, Debug, Deserialize)]
pub struct BucketSetup {
    /// Placeholder name (e.g. `"{bucket}"`, `"{source}"`, `"{dest}"`).
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateSetup {
    pub prefix: String,
    pub count: u64,
    pub size_range: [u64; 2],
}

#[derive(Clone, Debug, Deserialize)]
pub struct S3Object {
    #[serde(default = "default_bucket")]
    pub bucket: String,
    pub key: String,
    pub size: Option<u64>,
    pub content: Option<String>,
    pub content_type: Option<String>,
    pub last_modified: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub storage_class: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    pub checksum_algorithm: Option<String>,
    pub upload_method: Option<UploadMethod>,
    pub acl: Option<String>,
    pub sse: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UploadMethod {
    PutObject,
    Multipart,
}

#[derive(Debug, Deserialize)]
pub struct LocalFile {
    pub path: String,
    pub content: Option<String>,
    pub size: Option<u64>,
    /// Set the file's modification time after writing. ISO-8601, e.g.
    /// `2020-01-01T00:00:00Z` — the same format as
    /// `[[setup.objects]].last_modified`. Lets a spec pin a deterministic
    /// mtime relative to a seeded object's `LastModified`, which `sync`'s
    /// timestamp comparison depends on. Without it, files are created at
    /// "now", which races against an object's second-granularity `LastModified`.
    pub last_modified: Option<String>,
    pub symlink_to: Option<String>,
    pub permissions: Option<String>,
    pub permissions_windows: Option<WindowsPermissions>,
    #[serde(default)]
    pub platform: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WindowsPermissions {
    pub readonly: bool,
}

#[derive(Debug, Deserialize)]
pub struct ConfigSetup {
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub s3: HashMap<String, String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CommandSpec {
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub timeout: Option<u64>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Opt out of the harness-injected baseline environment, per key. The
    /// harness normally wires its baseline credentials, region, and endpoint
    /// into the highest-precedence layer (environment variables), which means a
    /// spec cannot otherwise test precedence, absence, or an alternative source
    /// for those values. Set a key `false` to make the harness inject nothing
    /// for it, handing that layer to the spec (via `env`, a seeded
    /// config/credentials file, a profile, or a flag). Defaults to injecting all.
    #[serde(default)]
    pub default_env: DefaultEnv,
    pub pipe_to: Option<Vec<String>>,
    // Possible future field: `tty: bool` — run the CLI through a pseudo-terminal
    // (pty) so it sees isatty()=true, for surfaces whose output depends on a TTY.
    // NOT needed for `aws s3`: it never calls isatty; progress / carriage-return
    // rendering is gated by --progress / --progress-multiline (not a TTY) and is
    // emitted through the pipe, so it is already observable here without a pty.
    // Retained as an option for future non-s3 surfaces. Would require a pty crate
    // (e.g. portable-pty or pty-process).
}

/// Per-key control over the harness's baseline environment injection.
///
/// Each field gates one baseline value the harness injects via environment
/// variables (the highest-precedence credential/config layer below an explicit
/// CLI flag). All default to `true` (inject), so existing specs are unaffected.
/// Set one `false` to suppress that injection entirely — the harness then writes
/// nothing for it anywhere, and the spec owns that layer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultEnv {
    /// Inject `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN`.
    /// Suppress to test credential precedence, absence ("Unable to locate
    /// credentials"), or a non-env source (profile, credential_process).
    #[serde(default = "default_true")]
    pub credentials: bool,
    /// Inject `AWS_DEFAULT_REGION`. Suppress to test region resolution from
    /// config/profile or its absence.
    #[serde(default = "default_true")]
    pub region: bool,
    /// Inject `AWS_ENDPOINT_URL`. Suppress to test endpoint resolution from
    /// config/profile or endpoint precedence. NOTE: on the mock backend the
    /// endpoint is a dynamic `127.0.0.1:PORT`; a spec suppressing this must
    /// supply the endpoint itself, so mock coverage awaits an `{endpoint}`
    /// placeholder (not yet implemented) — prod specs can hardcode the URL.
    #[serde(default = "default_true")]
    pub endpoint_url: bool,
}

impl Default for DefaultEnv {
    fn default() -> Self {
        Self {
            credentials: true,
            region: true,
            endpoint_url: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Expected {
    pub exit_code: i32,
    #[serde(default = "default_true")]
    pub verify_integrity: bool,
    pub stdout: Option<OutputAssertion>,
    pub stderr: Option<OutputAssertion>,
    #[serde(default)]
    pub objects: Vec<ExpectedObject>,
    #[serde(default)]
    pub files: Vec<ExpectedFile>,
    pub user_agent: Option<OutputAssertion>,
    #[serde(default)]
    pub platform: HashMap<String, PlatformExpectedOverride>,
}

/// Expected state of an S3 object after command execution.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ExpectedObject {
    /// Bucket placeholder (e.g. `"{bucket}"`).
    #[serde(default = "default_bucket")]
    pub bucket: String,
    /// Object key.
    pub key: String,
    /// Whether the object should exist. Use `false` for rm/delete verification.
    #[serde(default = "default_true")]
    pub exists: bool,
    /// Exact body content (for small files).
    pub content: Option<String>,
    /// Expected size in bytes (alternative to content for large files).
    pub size: Option<u64>,
    /// Expected content-type.
    pub content_type: Option<String>,
    /// Expected Cache-Control header (from `--cache-control`).
    pub cache_control: Option<String>,
    /// Expected Content-Encoding header (from `--content-encoding`).
    pub content_encoding: Option<String>,
    /// Expected Content-Disposition header (from `--content-disposition`).
    pub content_disposition: Option<String>,
    /// Expected Content-Language header (from `--content-language`).
    pub content_language: Option<String>,
    /// Expected ETag (including quotes, e.g. `"\"abc123\""`)
    pub e_tag: Option<String>,
    /// Expected storage class.
    #[serde(default, deserialize_with = "deser_from_str")]
    pub storage_class: Option<aws_sdk_s3::types::StorageClass>,
    /// Expected server-side encryption algorithm.
    #[serde(default, deserialize_with = "deser_from_str")]
    pub server_side_encryption: Option<aws_sdk_s3::types::ServerSideEncryption>,
    /// Expected checksum type (for multipart objects).
    #[serde(default, deserialize_with = "deser_from_str")]
    pub checksum_type: Option<aws_sdk_s3::types::ChecksumType>,
    /// Expected SHA-256 checksum value.
    pub checksum_sha256: Option<String>,
    /// Expected SHA-1 checksum value.
    pub checksum_sha1: Option<String>,
    /// Expected CRC-32 checksum value.
    pub checksum_crc32: Option<String>,
    /// Expected CRC-32C checksum value.
    pub checksum_crc32c: Option<String>,
    /// Expected CRC-64NVME checksum value.
    pub checksum_crc64nvme: Option<String>,
    /// Expected user metadata (partial match by default).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// If true, fail when object has metadata keys not listed in `metadata`.
    #[serde(default)]
    pub metadata_strict: bool,
    /// Expected upload mechanism, inferred from ETag (`"hex-N"` suffix = multipart).
    ///
    /// Portable across mock and prod. For protocol-level mechanism assertions
    /// (request counts, part sizes, etc.) see future `expected.mock.*` surface.
    pub upload_method: Option<UploadMethod>,
    /// Expected multipart part count, parsed from the ETag `"hex-N"` suffix
    /// (`N`). `None` on the assertion side skips the check; set it to pin the
    /// number of parts (e.g. a 16 MiB upload at the 8 MiB default chunksize = 2).
    /// A single-PUT object has no `-N` suffix and yields no part count.
    pub part_count: Option<u32>,
}

/// Deserialize an `Option<T>` from a string using `FromStr`.
fn deser_from_str<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) => s.parse::<T>().map(Some).map_err(serde::de::Error::custom),
    }
}

/// Expected state of a local file after command execution.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ExpectedFile {
    /// Path relative to working directory.
    pub path: String,
    /// Whether the file should exist. Use `false` for delete verification.
    #[serde(default = "default_true")]
    pub exists: bool,
    /// Exact file content.
    pub content: Option<String>,
    /// Expected size in bytes.
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlatformExpectedOverride {
    pub exit_code: Option<i32>,
    pub stdout: Option<OutputAssertion>,
    pub stderr: Option<OutputAssertion>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "mode", content = "value")]
pub enum OutputAssertion {
    #[serde(rename = "exact")]
    Exact(String),
    #[serde(rename = "contains")]
    Contains(Vec<String>),
    #[serde(rename = "regex")]
    Regex(Vec<String>),
    #[serde(rename = "unordered")]
    Unordered(Vec<String>),
    #[serde(rename = "golden")]
    Golden,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub faults: Vec<ServerFault>,
    #[serde(default)]
    pub integrity_faults: Vec<IntegrityFault>,
}

#[derive(Debug, Deserialize)]
pub struct IntegrityFault {
    pub operation: String,
    pub key: String,
    pub fault: String,
    pub part_number: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct FaultResponse {
    pub status_code: u16,
    pub error_code: Option<String>,
    pub delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ServerFault {
    pub operation: Option<String>,
    pub key: Option<String>,
    pub request_number: Option<usize>,
    pub status_code: Option<u16>,
    pub error_code: Option<String>,
    pub delay_ms: Option<u64>,
    pub responses: Option<Vec<FaultResponse>>,
}

#[derive(Debug, Deserialize)]
pub struct Deviation {
    pub behavior: String,
    pub rationale: String,
    pub tracking_id: Option<String>,
    pub expected: Option<DeviationExpected>,
}

#[derive(Debug, Deserialize)]
pub struct DeviationExpected {
    pub exit_code: Option<i32>,
    pub stdout: Option<OutputAssertion>,
    pub stderr: Option<OutputAssertion>,
}

/// Provenance for a spec — where the locked behavior comes from and why.
///
/// Specs are self-auditing: each carries the upstream evidence that
/// motivated it, so the spec file alone answers "where did this come from
/// and why does it exist." Optional; present on specs derived from a known
/// issue, PR, baseline source location, or upstream test.
#[derive(Debug, Deserialize)]
pub struct Source {
    /// Upstream references, repo-qualified so they are unambiguous across
    /// the repos behavior is drawn from. Each is GitHub shorthand
    /// (`org/repo#NUM`, e.g. `aws/aws-cli#523`) or a full URL.
    #[serde(default)]
    pub refs: Vec<String>,
    /// Location in the pinned v2 AWS CLI baseline as `path:line`, valid at
    /// the baseline commit recorded in `crates/docs/compat.md`.
    pub cli_ref: Option<String>,
    /// Optional pointer to the upstream Python CLI test that encodes this
    /// behavior (e.g. `tests/unit/.../test_x.py::TestCase`).
    pub python_test: Option<String>,
    /// Optional terse note for non-obvious context that `refs` / `cli_ref`
    /// / `python_test` don't already convey. State it factually — do not
    /// editorialize about the spec's purpose ("locks behavior", "prevents
    /// regression" is the point of every spec and adds nothing). Carry NO
    /// point-in-time or test state — no "verified", no "Rust matches" /
    /// "diverges", no pass/fail snapshot. Describe the baseline behavior and
    /// its provenance; running the suite reports conformance.
    pub rationale: Option<String>,
}

fn default_bucket() -> String {
    "{bucket}".into()
}

fn default_region() -> String {
    "us-east-1".into()
}

fn default_true() -> bool {
    true
}

/// Parse a TOML string into a [`TestSpec`].
pub fn parse_spec(toml_str: &str) -> Result<TestSpec, toml::de::Error> {
    toml::from_str(toml_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_source_provenance() {
        let toml = r#"
[test]
name = "rm_nonexistent"
description = "rm on a nonexistent object exits 0"
tags = ["rm"]

[command]
args = ["s3", "rm", "s3://{bucket}/missing.txt"]

[expected]
exit_code = 0

[source]
refs = ["aws/aws-cli#6926", "https://github.com/boto/s3transfer/pull/87"]
cli_ref = "awscli/customizations/s3/results.py:180"
python_test = "tests/unit/customizations/s3/test_results.py::ResultPrinterTest"
rationale = "Idempotent delete must still print the delete: line and exit 0."
"#;
        let spec = parse_spec(toml).unwrap();
        let source = spec.source.as_ref().unwrap();
        assert_eq!(
            source.refs,
            [
                "aws/aws-cli#6926",
                "https://github.com/boto/s3transfer/pull/87"
            ]
        );
        assert_eq!(
            source.cli_ref.as_deref(),
            Some("awscli/customizations/s3/results.py:180")
        );
        assert!(
            source
                .rationale
                .as_deref()
                .unwrap()
                .contains("delete: line")
        );
    }

    #[test]
    fn test_source_refs_only_no_rationale() {
        let toml = r#"
[test]
name = "refs_only"
description = "refs carry the provenance; rationale omitted"

[command]
args = ["s3", "rm", "s3://{bucket}/missing.txt"]

[expected]
exit_code = 0

[source]
refs = ["aws/aws-cli#6926"]
"#;
        let spec = parse_spec(toml).unwrap();
        let source = spec.source.as_ref().unwrap();
        assert_eq!(source.refs, ["aws/aws-cli#6926"]);
        assert!(source.rationale.is_none());
    }

    #[test]
    fn test_source_is_optional() {
        let toml = r#"
[test]
name = "no_source"
description = "spec without provenance still parses"

[command]
args = ["s3", "ls"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        assert!(spec.source.is_none());
    }

    #[test]
    fn test_parse_ls_spec() {
        let toml = r#"
[test]
name = "basic_object_listing"
description = "ls lists objects with correct date/size/key column formatting"
tags = ["ls", "output-format"]

[[setup.objects]]
key = "file1.txt"
size = 1024
last_modified = "2024-06-15T14:30:00Z"

[[setup.objects]]
key = "file2.txt"
size = 2048
last_modified = "2024-06-15T14:31:00Z"

[command]
args = ["s3", "ls", "s3://{bucket}/"]

[expected]
exit_code = 0

[expected.stdout]
mode = "exact"
value = "2024-06-15 14:30:00       1024 file1.txt\n2024-06-15 14:31:00       2048 file2.txt\n"

[expected.stderr]
mode = "exact"
value = ""
"#;
        let spec = parse_spec(toml).unwrap();
        assert_eq!(spec.test.name, "basic_object_listing");

        let setup = spec.setup.as_ref().unwrap();
        assert_eq!(setup.objects.len(), 2);
        assert_eq!(setup.objects[0].bucket, "{bucket}");
        assert_eq!(setup.objects[0].key, "file1.txt");
        assert_eq!(setup.objects[0].size, Some(1024));
        assert_eq!(setup.objects[1].size, Some(2048));

        assert_eq!(spec.expected.exit_code, 0);
        assert!(matches!(
            spec.expected.stdout,
            Some(OutputAssertion::Exact(_))
        ));
    }

    #[test]
    fn test_parse_cp_spec() {
        let toml = r#"
[test]
name = "upload_single_file"
description = "cp uploads a local file to S3"
tags = ["cp", "upload"]

[[setup.files]]
path = "src/hello.txt"
content = "Hello, World!\n"

[command]
args = ["s3", "cp", "src/hello.txt", "s3://{bucket}/hello.txt"]

[expected]
exit_code = 0

[expected.stdout]
mode = "exact"
value = "upload: src/hello.txt to s3://{bucket}/hello.txt\n"

[[expected.objects]]
bucket = "{bucket}"
key = "hello.txt"
content = "Hello, World!\n"
content_type = "text/plain"
"#;
        let spec = parse_spec(toml).unwrap();

        let setup = spec.setup.as_ref().unwrap();
        assert_eq!(setup.files.len(), 1);
        assert_eq!(setup.files[0].path, "src/hello.txt");
        assert_eq!(setup.files[0].content.as_deref(), Some("Hello, World!\n"));

        assert_eq!(spec.expected.objects.len(), 1);
        let obj = &spec.expected.objects[0];
        assert_eq!(obj.key, "hello.txt");
        assert_eq!(obj.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_parse_sync_spec() {
        let toml = r#"
[test]
name = "delete_removes_extra"
description = "sync --delete removes S3 objects not present locally"
tags = ["sync", "delete"]

[[setup.objects]]
bucket = "{bucket}"
key = "keep.txt"
size = 100
last_modified = "2024-06-15T10:00:00Z"

[[setup.objects]]
bucket = "{bucket}"
key = "remove-me.txt"
size = 200
last_modified = "2024-06-15T10:00:00Z"

[[setup.files]]
path = "sync-src/keep.txt"
size = 100

[command]
args = ["s3", "sync", "sync-src/", "s3://{bucket}/", "--delete"]

[expected]
exit_code = 0

[expected.stdout]
mode = "exact"
value = "delete: s3://{bucket}/remove-me.txt\n"

[[expected.objects]]
bucket = "{bucket}"
key = "remove-me.txt"
exists = false
"#;
        let spec = parse_spec(toml).unwrap();

        let setup = spec.setup.as_ref().unwrap();
        assert_eq!(setup.objects.len(), 2);
        assert_eq!(setup.files.len(), 1);

        assert_eq!(spec.expected.objects.len(), 1);
        assert!(!spec.expected.objects[0].exists);
    }

    #[test]
    fn test_parse_deviation_spec() {
        let toml = r#"
[test]
name = "progress_destination"
description = "Progress output goes to stderr, not stdout"
tags = ["output-format", "progress"]

[[setup.files]]
path = "src/big.bin"
size = 10485760

[command]
args = ["s3", "cp", "src/big.bin", "s3://{bucket}/big.bin"]

[expected]
exit_code = 0

[expected.stdout]
mode = "contains"
value = ["upload: src/big.bin to s3://{bucket}/big.bin", "Completed"]

[expected.stderr]
mode = "exact"
value = ""

[deviation]
behavior = "progress_output_destination"
rationale = "Baseline writes progress to stdout. We use stderr."
tracking_id = "https://github.com/aws/aws-cli/issues/5899"

[deviation.expected.stdout]
mode = "exact"
value = "upload: src/big.bin to s3://{bucket}/big.bin\n"

[deviation.expected.stderr]
mode = "contains"
value = ["Completed"]
"#;
        let spec = parse_spec(toml).unwrap();

        let dev = spec.deviation.as_ref().unwrap();
        assert_eq!(dev.behavior, "progress_output_destination");
        assert_eq!(
            dev.tracking_id.as_deref(),
            Some("https://github.com/aws/aws-cli/issues/5899")
        );

        let dev_expected = dev.expected.as_ref().unwrap();
        assert!(matches!(
            dev_expected.stdout,
            Some(OutputAssertion::Exact(_))
        ));
        assert!(matches!(
            dev_expected.stderr,
            Some(OutputAssertion::Contains(_))
        ));
    }

    #[test]
    fn test_parse_minimal_spec() {
        let toml = r#"
[test]
name = "minimal"
description = "minimal spec"

[command]
args = ["s3", "ls"]

[expected]
exit_code = 0
"#;
        let spec = parse_spec(toml).unwrap();
        assert!(spec.setup.is_none());
        assert!(spec.server.is_none());
        assert!(spec.deviation.is_none());
        assert_eq!(spec.test.target, TestTarget::Both);
        assert!(spec.expected.verify_integrity);
    }

    #[test]
    fn test_parse_malformed_toml() {
        let toml = r#"
[test]
name = 123
"#;
        let err = parse_spec(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid type"),
            "expected 'invalid type' in error, got: {msg}"
        );
    }

    #[test]
    fn test_parse_expected_object_defaults() {
        let toml = r#"
[test]
name = "obj_defaults"
description = "test"
tags = ["cp"]

[command]
args = ["s3", "cp", "f.txt", "s3://{bucket}/f.txt"]

[expected]
exit_code = 0

[[expected.objects]]
key = "f.txt"
"#;
        let spec = parse_spec(toml).unwrap();
        let obj = &spec.expected.objects[0];
        assert_eq!(obj.bucket, "{bucket}");
        assert_eq!(obj.key, "f.txt");
        assert!(obj.exists); // default true
        assert!(!obj.metadata_strict); // default false
        assert!(obj.content.is_none());
        assert!(obj.size.is_none());
        assert!(obj.content_type.is_none());
        assert!(obj.checksum_sha256.is_none());
        assert!(obj.checksum_crc32.is_none());
        assert!(obj.checksum_crc32c.is_none());
        assert!(obj.checksum_crc64nvme.is_none());
        assert!(obj.metadata.is_empty());
    }

    #[test]
    fn test_parse_expected_object_full() {
        let toml = r#"
[test]
name = "obj_full"
description = "test"
tags = ["cp"]

[command]
args = ["s3", "cp", "f.txt", "s3://{bucket}/f.txt"]

[expected]
exit_code = 0

[[expected.objects]]
bucket = "{dest}"
key = "uploaded.txt"
exists = true
content = "hello world"
size = 11
content_type = "text/plain"
storage_class = "GLACIER"
server_side_encryption = "aws:kms"
checksum_type = "COMPOSITE"
checksum_sha256 = "abc123"
checksum_crc32 = "def456"
metadata = { author = "test", version = "1" }
metadata_strict = true
"#;
        let spec = parse_spec(toml).unwrap();
        let obj = &spec.expected.objects[0];
        assert_eq!(obj.bucket, "{dest}");
        assert_eq!(obj.key, "uploaded.txt");
        assert!(obj.exists);
        assert_eq!(obj.content.as_deref(), Some("hello world"));
        assert_eq!(obj.size, Some(11));
        assert_eq!(obj.content_type.as_deref(), Some("text/plain"));
        assert_eq!(
            obj.storage_class,
            Some(aws_sdk_s3::types::StorageClass::Glacier)
        );
        assert_eq!(
            obj.server_side_encryption,
            Some(aws_sdk_s3::types::ServerSideEncryption::AwsKms)
        );
        assert_eq!(
            obj.checksum_type,
            Some(aws_sdk_s3::types::ChecksumType::Composite)
        );
        assert_eq!(obj.checksum_sha256.as_deref(), Some("abc123"));
        assert_eq!(obj.checksum_crc32.as_deref(), Some("def456"));
        assert!(obj.metadata_strict);
        assert_eq!(obj.metadata.get("author").unwrap(), "test");
        assert_eq!(obj.metadata.get("version").unwrap(), "1");
    }

    #[test]
    fn test_parse_expected_object_not_exists() {
        let toml = r#"
[test]
name = "obj_deleted"
description = "test"
tags = ["rm"]

[command]
args = ["s3", "rm", "s3://{bucket}/gone.txt"]

[expected]
exit_code = 0

[[expected.objects]]
key = "gone.txt"
exists = false
"#;
        let spec = parse_spec(toml).unwrap();
        let obj = &spec.expected.objects[0];
        assert!(!obj.exists);
    }

    #[test]
    fn test_parse_expected_file_defaults() {
        let toml = r#"
[test]
name = "file_defaults"
description = "test"
tags = ["cp"]

[command]
args = ["s3", "cp", "s3://{bucket}/f.txt", "f.txt"]

[expected]
exit_code = 0

[[expected.files]]
path = "f.txt"
"#;
        let spec = parse_spec(toml).unwrap();
        let f = &spec.expected.files[0];
        assert_eq!(f.path, "f.txt");
        assert!(f.exists); // default true
        assert!(f.content.is_none());
        assert!(f.size.is_none());
    }

    #[test]
    fn test_parse_expected_file_full() {
        let toml = r#"
[test]
name = "file_full"
description = "test"
tags = ["cp"]

[command]
args = ["s3", "cp", "s3://{bucket}/f.txt", "output.txt"]

[expected]
exit_code = 0

[[expected.files]]
path = "output.txt"
exists = true
content = "downloaded content"
size = 18
"#;
        let spec = parse_spec(toml).unwrap();
        let f = &spec.expected.files[0];
        assert_eq!(f.path, "output.txt");
        assert!(f.exists);
        assert_eq!(f.content.as_deref(), Some("downloaded content"));
        assert_eq!(f.size, Some(18));
    }

    #[test]
    fn test_parse_expected_file_not_exists() {
        let toml = r#"
[test]
name = "file_gone"
description = "test"
tags = ["mv"]

[command]
args = ["s3", "mv", "src.txt", "s3://{bucket}/src.txt"]

[expected]
exit_code = 0

[[expected.files]]
path = "src.txt"
exists = false
"#;
        let spec = parse_spec(toml).unwrap();
        let f = &spec.expected.files[0];
        assert!(!f.exists);
    }
}
