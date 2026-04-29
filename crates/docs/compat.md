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

**Python CLI behavior:** Uses `nargs='?'` with `const='requester'` and
`choices=['requester']` — the flag can be used with or without a value.
`--request-payer` alone means `--request-payer requester`.

**Rust implementation:** Clap `num_args = 0..=1` with
`default_missing_value = "requester"` and `value_parser = ["requester"]`.
Bare flag, `--request-payer requester`, and `--request-payer=requester`
all yield `Some("requester")`; absent yields `None`.

**Known quirk (matches Python):** `--request-payer` placed immediately
before a positional arg (e.g. `cp --request-payer ./src s3://b/dst`)
causes the positional to be consumed as the flag's value and rejected
by `value_parser`. Same limitation in Python argparse — `choices`
rejection catches it there too. Workaround: use `--request-payer=requester`
or place the flag after the positional args.

**Status:** Landed. See `cli::tests::*_request_payer_*` (5 tests).

## Streaming (`-`) Support

**Impact:** cp with stdin/stdout.

**Python CLI behavior:** `cp - s3://bucket/key` reads from stdin.
`cp s3://bucket/key -` writes to stdout. Only compatible with non-recursive
cp.

**Rust implementation:** `TransferUri` does not have a Stdio variant.
Not yet implemented.

## `--no-verify-ssl`

**Impact:** All commands when SSL verification is disabled.

**Python CLI behavior:** Disables SSL certificate verification on the HTTP
client. Takes precedence over `--ca-bundle` and `AWS_CA_BUNDLE`.
(`awscli/customizations/globalargs.py:resolve_verify_ssl`,
`tests/unit/customizations/test_globalargs.py` `test_no_verify_ssl_overrides_cli_cert_bundle`.)

**Rust implementation:** Currently rejected at argument-parse time with
exit 252. `aws-smithy-http-client` provides no public API to disable TLS
certificate verification — no `dangerous`/`no_verify`/`ServerCertVerifier`
hook on `TlsContext` or the rustls provider (verified against
`rust-runtime/aws-smithy-http-client/src/client/tls/` at the pinned
behavior version). Silently accepting the flag would create a false
sense of security; the user should know their verification choice had
no effect.

**Status:** Tracked; not yet resolved.

**Candidate path to resolution:** Expose a verification toggle on
`TrustStore` (or a new `TlsContextBuilder::with_certificate_verification(bool)`)
upstream in smithy-rs. Once available, swap the rejection for a custom
TLS context with verification disabled, matching Python's behavior. No
CLI API change; only the internal wiring flips from reject to honor.

## `--ca-bundle`

**Impact:** All commands. Users pointing the CLI at a custom CA bundle
(enterprise proxies, testing against local endpoints with self-signed
certs, etc.).

**Python CLI behavior:** urllib3 uses the provided PEM as the sole trust
anchor. Applies uniformly to every request across every command.

**Rust implementation:** Currently rejected at argument-parse time with
exit 252.

**Why rejected rather than partially supported:** there's a bifurcation
in how commands reach S3:
- `ls`, `mb`, `rb`, `rm`, `presign`, `website` use a single
  `aws_sdk_s3::Client` whose HTTP transport we control directly via
  `build_http_client` — here a `--ca-bundle` could be honored
  straightforwardly.
- `cp`, `mv`, `sync` go through the Rust Transfer Manager. TM's managed
  runtime builds per-thread `aws_smithy_http_client::Builder` instances
  inside `runtime/managed.rs:215` with no hook today for threading a
  caller-supplied `TlsContext` through.

Honoring `--ca-bundle` only on the non-TM path would mean the flag
silently works for listing and silently fails for transfers — a
compat/security gap worse than a clean rejection.

**Status:** Tracked; not yet resolved. Two upstream gaps, both captured
in bosun.md "Upstream Contributions":

1. `aws-sdk-s3-transfer-manager`: add `S3ClientConfig::with_tls_context(...)`
   (or a more general per-HTTP-client customization hook) so TM's
   managed runtime threads the caller's TLS context into every
   per-thread HTTP client.
2. `aws-smithy-http-client`: non-panicking `TrustStore` validation so
   malformed PEMs surface as structured errors rather than panics on
   a tokio worker thread.

**Resolution sequence:**
1. Upstream TM exposes `with_tls_context` (or equivalent).
2. Upstream smithy-rs exposes a non-panicking PEM validation path.
3. CLI refactors `build_context` to return the `aws_sdk_s3::config::Builder`
   alongside the built client; `cp.rs::build_tm` switches from
   `.client(...)` to `.s3_config(...)` with our TLS context threaded
   through.
4. Rejection in `main.rs` lifts; `build_http_client` gains the
   `TrustStore::empty().with_pem_certificate(pem)` branch back.

## `--cli-read-timeout` Semantics

**Impact:** `--cli-read-timeout` users expecting per-socket-read semantics.

**Python CLI behavior:** `read_timeout` is applied per-socket-read — the
time between successive chunks of data arriving on an established connection.
A 60-second read timeout on a 5-minute download succeeds as long as no
individual read stalls for more than 60 seconds.

**Rust SDK behavior:** `TimeoutConfig::read_timeout` is time-to-first-byte
from request initiation, not per-chunk. A long-running download whose
server takes more than the configured timeout to start responding fails,
but once streaming begins, the timeout no longer applies per-chunk.

**Status:** Tracked; may become obsolete as the Rust SDK's timeout model
evolves. No action needed now.

## `--debug` Output Format

**Impact:** Users scraping debug output expecting Python's format.

**Python CLI behavior:** `_set_logging` installs a stdlib `logging` handler
on `botocore`, `awscli`, `s3transfer`, `urllib3` at DEBUG. Format string
is defined by `LOG_FORMAT` in `awscli/clidriver.py`; output includes
log level, timestamp, logger name, and message.

**Rust implementation:** `--debug` installs `tracing_subscriber::fmt()` at
DEBUG writing to stderr. Format is tracing's default (ANSI colors,
ISO-8601 timestamps, span context). Captures equivalent information but
layout differs.

**Status:** Tracked as open question. Is format-level parity worth the
engineering cost? Nothing user-facing should depend on debug output
format, but tooling may. No action now; revisit if users complain.

## CLI Output / Formatting Flags (s3 No-Ops)

**Impact:** Users passing these flags to `aws s3 <cmd>`.

**Context:** These flags control output formatting, pagination, binary
input encoding, pager behavior, interactive prompting, and error
formatting at the CLI driver level. Python's `aws s3` commands do not
honor any of them on output — `aws s3 ls` help explicitly states
"`--output` and `--no-paginate` arguments are ignored for this command,"
and all s3 output is hand-formatted text written directly to stdout.

**Python CLI behavior on `aws s3`:**

| Flag | Behavior |
|------|----------|
| `--output {json,text,table,yaml,yaml-stream,off}` | Ignored; s3 emits hand-formatted text |
| `--query <jmespath>` | Ignored; s3 output isn't structured |
| `--no-paginate` | Ignored on ls; transfer commands self-paginate |
| `--no-cli-pager` | Effective no-op; s3 writes directly to stdout |
| `--cli-binary-format {base64,raw-in-base64-out}` | Applies to blob CLI inputs; s3 takes none |
| `--cli-error-format {legacy,json,yaml,text,table,enhanced}` | Applies to SDK error formatting; s3 formats errors via botocore's `ClientError.MSG_TEMPLATE` directly |
| `--cli-auto-prompt` / `--no-cli-auto-prompt` | Prompts interactively for missing args before dispatch |

**Rust implementation:** All seven flags are parsed by clap (required
for cli.json coverage) and ignored. No runtime effect. Matches Python's
behavior on s3 for six of seven.

**Divergence:** `--cli-auto-prompt` is the only genuine behavior
difference — Python drops into an interactive prompt loop for missing
args before dispatching the subcommand; we do nothing. Low-impact on
s3 because s3 subcommand args are simple and positional. If a spec
emerges that requires interactive prompting, revisit.

**Status:** Documented as intentional no-op. No code change needed.

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

## S3-Specific Config Keys

**Impact:** All commands when users rely on `~/.aws/config` `[s3]` section
or `AWS_S3_*` env vars to configure S3 behavior.

**Python CLI behavior:** botocore reads the `[s3]` subsection of
`~/.aws/config` (and per-profile sections) plus `AWS_S3_*` env vars. These
configure S3-specific behavior including:

| Config Key | Env Var | Default | Effect |
|------------|---------|---------|--------|
| `addressing_style` | `AWS_S3_ADDRESSING_STYLE` | `auto` | `path`/`virtual`/`auto` — host-style vs path-style addressing |
| `use_arn_region` | `AWS_S3_USE_ARN_REGION` | `true` | Use region from access point ARN |
| `us_east_1_regional_endpoint` | `AWS_S3_US_EAST_1_REGIONAL_ENDPOINT` | `regional` | `regional` vs `legacy` for `us-east-1` |
| `use_accelerate_endpoint` | `AWS_S3_USE_ACCELERATE_ENDPOINT` | `false` | Transfer Acceleration endpoint |
| `use_dualstack_endpoint` | `AWS_USE_DUALSTACK_ENDPOINT` | `false` | IPv6 dualstack endpoint |
| `signature_version` | — | Determined by service | `s3`/`s3v4`/`s3v4a` |
| `payload_signing_enabled` | — | Default varies | Sign request payload |
| `s3_disable_multiregion_access_points` | `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS` | `false` | Block MRAP usage |

**Rust SDK / aws-config behavior:** The `aws-sdk-s3` crate's `Config`
builder supports most of these at the code level (`.force_path_style(bool)`,
`.use_arn_region(bool)`, `.accelerate(bool)`, `.use_dualstack_endpoint(bool)`,
etc.). But `aws-config` does NOT auto-parse the `[s3]` section of
`~/.aws/config` or the `AWS_S3_*` env vars. The SDK knows how to use these
values; it just doesn't discover them from the standard AWS CLI config.

**Resolution:** Parse the `[s3]` section and `AWS_S3_*` env vars ourselves
and apply them to the S3 `Config` builder. This is a wiring job, not a
capability gap. Priority keys:

1. `addressing_style` — common, used by users with bucket names containing dots
2. `use_accelerate_endpoint` — user-visible performance feature
3. `use_dualstack_endpoint` — IPv6 requirement for some environments
4. `use_arn_region` — access point ARN usage
5. `signature_version` — needed for SigV4a (MRAP) and legacy cases

**Python tests affected:**
- `test_can_support_addressing_mode_config` (presign) — `addressing_style`
- Various cp/sync tests that implicitly depend on default addressing

Marked as FIXME in `main.rs` `build_context()`.
