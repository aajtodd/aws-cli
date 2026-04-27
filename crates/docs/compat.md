# Behavioral Gaps

Differences between the Rust SDK and botocore/Python CLI that affect
behavioral compatibility. Each gap needs resolution before the affected
commands can pass compat tests.

## Cross-Region Bucket Redirect

**Impact:** All S3 operations against buckets in a different region than
the configured default.

**Python CLI behavior:** botocore's `S3RegionRedirectorv2` intercepts 301
PermanentRedirect errors, does a HeadBucket to discover the correct region,
and retries the request transparently. `aws s3 ls s3://bucket-in-us-west-2`
works from any region without `--region`.

**Rust SDK behavior:** Returns the 301 as a service error. No automatic
redirect.

**Options:**
1. Implement redirect logic (HeadBucket → extract region → rebuild client → retry)
    - We don't need to rebuild client, SDK allows for per/operation overrides. 
2. Always resolve bucket region via HeadBucket before first operation
3. Require `--region` (breaks compat, not viable)

**Python tests:** `tests/unit/botocore/test_utils.py` (`S3RegionRedirectorv2`),
`tests/unit/customizations/test_s3errormsg.py` (`test_301_error_message`).

## Error Message Format

**Impact:** All error output.

**Python CLI behavior:** Service errors formatted as:
```
An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}
```
Source: `botocore/exceptions.py` `ClientError.MSG_TEMPLATE`.

**Rust SDK behavior:** `SdkError::Display` prints "service error" with no
structured format. Error code and message are available via
`ProvideErrorMetadata`, but the operation name is not carried on the error.

**Resolution:** `format_sdk_error(err, operation_name)` extracts code and
message from `ProvideErrorMetadata` and formats to match. Operation name
passed from the call site. Implemented.

## SDK Option Semantics

**Impact:** ListObjectsV2 and other operations where `None` vs empty string
matters.

**Python CLI behavior:** botocore always sends `Prefix` (even as empty string)
and only sends `Delimiter` for non-recursive listing.

**Rust SDK behavior:**
- `.delimiter("/")` → `Some("/")` (sent in request)
- Not calling `.delimiter()` → `None` (omitted from request)
- `.prefix("")` → `Some("")` (sent as `Prefix=`)

**Resolution:** Match the Python CLI's behavior explicitly. Non-recursive
listing sets `delimiter("/")` and `prefix("")`. Recursive listing sets
`prefix("")` and omits delimiter. Implemented.

## Access Point ARN Parsing

**Impact:** Commands targeting access points, outpost ARNs, MRAP ARNs.

**Python CLI behavior:** `find_bucket_key()` in `utils.py` handles access
point ARNs (`arn:aws:s3:...`), outpost ARNs, and MRAP ARNs via regex
matching. Object Lambda and outpost bucket ARNs are rejected.

**Rust implementation:** Currently only handles `s3://bucket/key` format.
ARN parsing not yet implemented.

**Python tests:** `test_utils.py` has 15+ ARN parsing tests.

## Content-Type Detection

**Impact:** cp, mv, sync uploads.

**Python CLI behavior:** Uses Python's `mimetypes.guess_type()` to detect
content type from file extension.

**Rust implementation:** Not yet implemented. Will use `mime_guess` crate.
Some extensions will produce different results — these need to be cataloged
and tracked as known deviations.

## `--request-payer` Optional Value

**Impact:** ls, cp, mv, rm, sync with `--request-payer`.

**Python CLI behavior:** Uses `nargs='?'` with `const='requester'` — the
flag can be used with or without a value. `--request-payer` alone means
`--request-payer requester`.

**Rust implementation:** Currently requires an explicit value. Clap's
`default_missing_value` should handle this but needs verification.

## Streaming (`-`) Support

**Impact:** cp with stdin/stdout.

**Python CLI behavior:** `cp - s3://bucket/key` reads from stdin.
`cp s3://bucket/key -` writes to stdout. Only compatible with non-recursive
cp.

**Rust implementation:** `TransferUri` does not have a Stdio variant.
Not yet implemented.

## `--no-verify-ssl` Wiring

**Impact:** All commands when SSL verification is disabled.

**Python CLI behavior:** Disables SSL certificate verification on the
HTTP client.

**Rust implementation:** Parsed as a global flag but not applied to the
SDK client configuration.

## Credential Resolution and Profiles

**Impact:** All commands. Credential chain behavior must match the Python CLI.

**Python CLI behavior:** Uses botocore's credential chain: env vars
(`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`),
`AWS_PROFILE`, `~/.aws/credentials`, `~/.aws/config` (with `credential_process`,
`sso-session`, `role_arn` for assume-role), instance metadata (IMDS), ECS
container credentials. Profile selection via `--profile` flag or `AWS_PROFILE`
env var. Config file location via `AWS_CONFIG_FILE`, credentials file via
`AWS_SHARED_CREDENTIALS_FILE`.

**Rust SDK behavior:** `aws-config` has its own credential chain that covers
most of the same sources but may differ in edge cases: resolution order,
error messages when credentials are missing or expired, SSO token refresh
behavior, assume-role chaining, `credential_process` execution.

**Gaps to verify:**
- Profile resolution order matches (`--profile` > `AWS_PROFILE` > `default`)
- Error messages for missing/expired credentials
- SSO login flow and token caching
- Assume-role with MFA
- `credential_process` execution and output parsing
- IMDS timeout and retry behavior

## Environment Variables

**Impact:** All commands. The CLI respects many env vars beyond credentials.

**Python CLI behavior:** Key env vars include:
- `AWS_DEFAULT_REGION` / `AWS_REGION` — region selection
- `AWS_ENDPOINT_URL` / `AWS_ENDPOINT_URL_S3` — endpoint override
- `AWS_CA_BUNDLE` — custom CA certificate
- `AWS_MAX_ATTEMPTS` — retry configuration
- `AWS_RETRY_MODE` — retry strategy (standard, adaptive, legacy)
- `AWS_DEFAULT_OUTPUT` — output format (json, text, table)
- `AWS_PAGER` — pager program
- `AWS_CLI_AUTO_PROMPT` — auto-prompt mode
- `AWS_CLI_FILE_ENCODING` — file encoding for Windows

**Rust SDK behavior:** `aws-config` handles `AWS_REGION`,
`AWS_ENDPOINT_URL`, `AWS_MAX_ATTEMPTS`, `AWS_RETRY_MODE`. CLI-specific
env vars (`AWS_PAGER`, `AWS_CLI_AUTO_PROMPT`, `AWS_CLI_FILE_ENCODING`,
`AWS_DEFAULT_OUTPUT`) are not SDK concerns — we need to handle them.

## Endpoint Resolution

**Impact:** All commands. How the final endpoint URL is determined.

**Python CLI behavior:** Endpoint resolution considers (in order):
`--endpoint-url` flag, `AWS_ENDPOINT_URL_S3` (service-specific),
`AWS_ENDPOINT_URL` (global), config file `endpoint_url` (per-service
and global sections), then the default S3 endpoint for the region.
Also handles FIPS (`--use-fips-endpoint`), dualstack
(`--use-dualstack-endpoint`), S3 Transfer Acceleration, path-style
vs virtual-hosted-style addressing, and access point/MRAP endpoints.

**Rust SDK behavior:** `aws-config` handles `AWS_ENDPOINT_URL` and
the `--endpoint-url` equivalent. Service-specific env vars, FIPS,
dualstack, and acceleration need verification. Path-style addressing
is configured via `force_path_style` on the S3 config.

**Gaps to verify:**
- Service-specific endpoint env var (`AWS_ENDPOINT_URL_S3`)
- FIPS and dualstack endpoint flags
- S3 Transfer Acceleration
- Path-style vs virtual-hosted-style default and override
- Endpoint resolution order matches Python CLI

## User Agent

**Impact:** All requests. The User-Agent header identifies the client.

**Python CLI behavior:** User-Agent includes the CLI version, Python
version, OS, and botocore version. Format:
`aws-cli/{cli_version} Python/{python_version} {os}/{os_version} botocore/{botocore_version}`
Additional features are appended (e.g. `command/s3.ls`, `md/FIPS`,
`cfg/retry-mode#standard`). The CLI also appends `S3Transfer` and
`crt` markers when using the transfer manager or CRT.

**Rust SDK behavior:** The Rust SDK sets its own User-Agent with SDK
version and OS info. We need to either override or append to match
the Python CLI's format, or define a new format that identifies this
as the native Rust path. At minimum, the user agent must identify:
- That this is the AWS CLI (not a raw SDK call)
- The CLI version
- The subcommand being executed (`command/s3.ls`)
- Whether the Rust Transfer Manager is in use

**Decision needed:** Match the Python CLI's User-Agent exactly (for
transparent replacement) or use a distinct format (for observability
of the rollout). Either way, the `command/` feature tag is important
for service-side metrics.
