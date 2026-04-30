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

**Rust implementation:** `src/arn.rs` provides a dedicated ARN parser
(no regex). `TransferUri::from_str` recognizes inputs starting with
`arn:`, parses them, and classifies by `service` + `resource` shape:

| ARN shape | Behavior |
|-----------|----------|
| Standard access point (`s3`) | Accepted; bucket field carries full ARN up to AP name, remainder becomes key |
| MRAP (`s3` with empty region) | Accepted; same shape as standard AP |
| Outposts access point (`s3-outposts`, resource `outpost/ID/accesspoint/NAME`) | Accepted |
| Outposts bucket (`s3-outposts`, resource `outpost/ID/bucket/NAME`) | Rejected with Python-exact message |
| S3 Object Lambda (`s3-object-lambda`) | Rejected with Python-exact message |
| Malformed / unknown shape | Falls through to `Local` (matches Python's lenient fallthrough) |

Both `/` and `:` are accepted as segment separators within the resource
field (`accesspoint:NAME`, `outpost:ID:accesspoint:NAME`). All partition
variants are accepted (`aws`, `aws-cn`, `aws-us-gov`, `aws-iso`, `aws-iso-b`).
SDK endpoint resolver handles routing once the ARN is in the bucket slot.

**Status:** Landed. See `uri::tests::arn_*` (16 tests) +
`arn::tests::*` (7 tests).

**Python tests:** `test_utils.py` has 15+ ARN parsing tests; our black-box
tests cover the same behavior via the public `TransferUri::from_str` path.

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
in the upstream contributions tracker:

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

**Audit status:** Completed 2026-04-29. Source-level comparison of
`botocore/credentials.py` vs `aws-config/src/default_provider/credentials.rs`
and sibling modules. Summary below.

### Verified matches

- Provider resolution order (env → profile → SSO → assume-role → container → IMDS)
- `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` environment variables
- `AWS_PROFILE` environment variable (selects profile)
- `--profile` flag override
- `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE` location overrides
- `AWS_ROLE_ARN`, `AWS_ROLE_SESSION_NAME` for assume-role
- `AWS_WEB_IDENTITY_TOKEN_FILE` for web-identity/IRSA flow
- Container credentials (`AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`,
  `AWS_CONTAINER_CREDENTIALS_FULL_URI`, `AWS_CONTAINER_AUTHORIZATION_TOKEN`)
- In-memory credential caching with expiry-based refresh
- SSO token disk cache at `~/.aws/sso/cache/`
- Assume-role chain loop detection

### Known divergences (HIGH severity)

**IMDSv1 fallback not supported.** Python supports IMDSv1 as a fallback
when IMDSv2 is unavailable. The Rust SDK is IMDSv2-only
(`imds/client.rs:68` states "ONLY supports IMDSv2"). In environments
where IMDSv2 is disabled or unreachable (some older EC2 AMIs, specific
container setups), Python would succeed and we would fail.

**MFA prompting for assume-role not supported.** Python prompts
interactively via `getpass.getpass` when a profile has `mfa_serial` set.
The Rust SDK has no MFA support at all — no `mfa_serial` field on
`RoleArn`, no token_code flow, no interactive prompt mechanism. Profiles
with `mfa_serial` will silently fail or return a cryptic STS error.

### Known divergences (Medium severity)

| Behavior | Python | Rust |
|---|---|---|
| `AWS_DEFAULT_PROFILE` | Honored as fallback for `AWS_PROFILE` | Not honored; only `AWS_PROFILE` |
| `AWS_SECURITY_TOKEN` (legacy) | Honored as fallback for `AWS_SESSION_TOKEN` | Not honored |
| `AWS_CREDENTIAL_EXPIRATION` | Honored (for static credentials with expiry) | Not honored |
| EnvProvider when `--profile` is explicit | Removed from chain | Not removed |
| SSO provider | Always available | Requires `sso` cargo feature (enabled by default in aws-config 1.x; confirmed via smithy-rs `aws-config/Cargo.toml`) |
| credential_process | Always available | Requires `credentials-process` cargo feature (enabled by default in aws-config 1.x; confirmed) |
| SSO token refresh buffer | Advisory 15min, mandatory 10min | 5min |

### Known divergences (Low severity)

- **STS assume-role disk cache:** Python's `JSONFileCache` caches
  assume-role responses to disk at `~/.aws/cli/cache/` (reused across
  process invocations). Rust has in-memory caching only. For CLI
  workloads with rapid invocations, this means more STS calls per unit
  time. Implementing an on-disk cache at the same path would be a
  compat win — Python and Rust CLIs could share cache entries.
- **Error message wording:** Specific and informative in both, but
  different text. Scripts that match on error-message substrings will
  break.
- **Legacy providers:** Python has `BotoProvider` (`~/.boto` support)
  and `OriginalEC2Provider`. Neither exists in Rust SDK. Historical
  compat only; no ship-gate impact.
- **Rust-specific: `SECRET_ACCESS_KEY` fallback.** Rust env provider
  falls back to unprefixed `SECRET_ACCESS_KEY` when `AWS_SECRET_ACCESS_KEY`
  is unset. Python does not. Rust-only behavior, low impact.

### Follow-up actions

1. **MFA prompting decision:** No Rust-SDK-layer fix available. Would
   need CLI-layer implementation — intercept STS failure, read
   profile's `mfa_serial`, prompt user, retry with `token_code`.
   Decide: ship-gate or post-GA?
2. **IMDSv1 fallback decision:** For target environments (developer
   laptops, modern EC2, containers), IMDSv2-only is probably fine.
   Document the constraint in user-facing docs. If we target older EC2
   AMIs, investigate whether the Rust SDK can be coaxed to fall back
   or whether it needs an upstream feature.
3. **Wire medium-severity env vars** if any of (`AWS_DEFAULT_PROFILE`,
   `AWS_SECURITY_TOKEN`, `AWS_CREDENTIAL_EXPIRATION`) are critical for
   target users. Implement in our config layer (read the env var
   ourselves and apply via `aws-config` builder).
4. **STS disk cache:** Low priority, but cheap and user-visible
   compat win. Implement JSONFileCache-equivalent at `~/.aws/cli/cache/`.
   Python and Rust CLIs would share the same cache directory.

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

## Retry Behavior

**Impact:** All commands. Retry matters for reliability under throttling,
transient errors, and service flakiness.

**Audit status:** Completed 2026-04-29. Source-level comparison of
`botocore/retries/` vs `aws-smithy-runtime/src/client/retries/` and
`aws-runtime/src/retries/`. Summary below.

### Verified matches

- Default retry mode: `standard` in both
- Default max attempts: 3 (including initial attempt)
- `AWS_MAX_ATTEMPTS` and `AWS_RETRY_MODE` env vars
- `max_attempts` and `retry_mode` profile keys
- Exponential backoff formula with full jitter; initial_backoff=1s, max_backoff=20s
- Retry token bucket: capacity=500, retry cost=5, timeout cost=10, regeneration=1
- All 14 throttling error codes (`Throttling`, `ThrottlingException`,
  `ProvisionedThroughputExceededException`, `RequestThrottled`, `SlowDown`,
  etc.)
- Transient errors: `RequestTimeout`, `RequestTimeoutException`
- HTTP 500/502/503/504 retried
- Adaptive mode CUBIC rate limiter parameters (BETA=0.7, SCALE=0.4, SMOOTH=0.8)
- Modeled retryable errors from service shape metadata

### Known divergences

All **Low severity** — none should affect typical S3 CLI usage.

| Behavior | Python | Rust |
|---|---|---|
| Legacy retry mode (`legacy`) | Supported | Not supported |
| `x-amz-retry-after` header | Not honored | Honored (parsed as ms, server-requested delay) |
| STS `IDPCommunicationError` special-case | Retried | Not retried |
| DynamoDB CRC32 checksum special-case | Retried | Not retried |
| `PriorRequestNotComplete` | In both throttling and transient classifiers | Throttling only |

The `x-amz-retry-after` divergence means Rust honors server-requested delays
while Python ignores them — Rust is strictly better here. STS and DynamoDB
special cases don't affect S3 commands.

### Open questions

- **Clock skew retry:** Neither audit found explicit retry on
  `RequestTimeTooSkewed`. Rust SDK has `ServiceClockSkewInterceptor` that
  prevents the error proactively. Whether either SDK retries reactively
  on skew errors is unverified.
- **Adaptive mode maturity in Rust SDK:** `VALID_RETRY_MODES` constant
  lists only `Standard` (adaptive tests are commented out in source).
  Adaptive is parseable and has an implementation, but may not be fully
  validated for production use. Worth verifying before we claim adaptive-mode
  parity.

### Status

No follow-up action required for ship-gate parity. Standard-mode retry
behavior is functionally identical.

## Endpoint Resolution

**Impact:** All commands. How the final endpoint URL is determined.

**Audit status:** Completed 2026-04-29. Source-level comparison of
`botocore/endpoint_provider.py` + `botocore/args.py` vs the SDK-generated
`aws-sdk-s3/src/config/endpoint.rs` (from the same Smithy endpoint
ruleset model). Summary below.

### Verified matches

Both implementations evaluate the **same Smithy endpoint ruleset**, so
their behavior for the following is identical:

- Default regional endpoints (`s3.{region}.amazonaws.com`)
- Legacy `us-east-1` global endpoint (`s3.amazonaws.com`) via
  `use_global_endpoint` flag; defaults to regional in both
- FIPS endpoints (`s3-fips.{region}.amazonaws.com`) via
  `use_fips_endpoint` config / `AWS_USE_FIPS_ENDPOINT` env var
- Dualstack / IPv6 endpoints (`s3.dualstack.{region}.amazonaws.com`)
- S3 Transfer Acceleration (`bucket.s3-accelerate.amazonaws.com`);
  forces virtual-hosted style; errors when combined with MRAP
- Access point ARN routing (`{ap-name}-{account}.s3-accesspoint.{region}.amazonaws.com`)
- MRAP routing (`{alias}.accesspoint.s3-global.amazonaws.com`) with
  SigV4a; errors when combined with FIPS/dualstack/accelerate
- `s3_disable_multiregion_access_points` config key
- Outposts endpoint routing (`{ap}-{account}.{outpost-id}.s3-outposts.{region}.amazonaws.com`)
- S3 Express directory buckets (`bucket.s3express-{az}.{region}.amazonaws.com`)
  with `sigv4-s3express` auth
- `disable_s3_express_session_auth` config / `AWS_S3_DISABLE_EXPRESS_SESSION_AUTH` env
- Path-style vs virtual-hosted-style addressing, including auto-fallback
  to path-style for non-DNS-compatible bucket names (dots, uppercase,
  underscores, too short)
- `use_arn_region` behavior (defaults to true; route to ARN's region
  when ARN region ≠ client region)
- `AWS_ENDPOINT_URL_S3` service-specific env var with the right priority
- `--endpoint-url` CLI flag overrides all other endpoint sources
- FIPS + custom endpoint → error (both)
- Dualstack + custom endpoint → error (both)

### Known divergences (HIGH severity)

**Cross-region bucket redirect (301) not supported in Rust SDK.**
Python CLI has `S3RegionRedirectorv2` (registered in `client.py:330`)
that intercepts 301/302/307 redirects, `AuthorizationHeaderMalformed`
errors, and Head\* 400 errors. It calls `HeadBucket` to discover the
correct region, caches the mapping, and transparently retries the
request in the correct region. The Rust SDK has **no equivalent**.
A comprehensive search of the smithy-rs codebase found no
`RegionRedirect` interceptor or equivalent retry-on-301 logic.

This is the **single most impactful endpoint divergence**. Any user
with buckets in a region different from their configured default will
get 301/PermanentRedirect errors instead of transparent redirection.
Commonly hit by users who don't pass `--region`.

**Mitigation path:** Implement an SDK interceptor in our CLI crate
that wraps operations, catches redirect-class errors, calls HeadBucket
to discover the region, caches it, and retries. Reference:
`botocore/utils.py:1570-1670` (S3RegionRedirectorv2).

### Open questions

- **`ignore_configured_endpoint_urls`:** Python supports
  `AWS_IGNORE_CONFIGURED_ENDPOINT_URLS`. Rust has
  `default_provider/ignore_configured_endpoint_urls.rs` — mechanism
  exists but env var name match unverified.

### Resolved

- **`use_arn_region` from profile config:** Verified Rust SDK does
  **not** read `s3.use_arn_region` from the profile — `aws-config`
  has no provider for it. See §S3-Specific Config Keys for the sub-section
  story and wiring plan.

### Follow-up actions

1. **Cross-region redirect interceptor** — high priority ship-gate work.
   Design and implement in our CLI layer wrapping the SDK client.
   Tracked as architectural work (cross-region redirect interceptor).
2. **Integration tests** for `AWS_ENDPOINT_URL_S3`, `--endpoint-url`
   priority chain, `use_arn_region` from profile. Cover via compat
   framework specs.

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

**Impact:** All commands when users rely on `~/.aws/config` `[s3]`
sub-section or `AWS_S3_*` env vars to configure S3 behavior.

See `crates/docs/s3-config-keys.md` for the complete landscape doc
(every key, classified, with a per-key wiring decision). This section
covers the compat gap at the mechanism level.

### Python CLI behavior

botocore uses a `SectionConfigProvider` (in `botocore/configprovider.py`)
that descends into the `[s3]` sub-section of a profile:

```ini
[profile myprofile]
s3 =
  addressing_style = path
  use_accelerate_endpoint = true
  use_arn_region = false
```

Python reads these as structured `(section, key)` pairs. Plus
`AWS_S3_*` env vars (`AWS_S3_USE_ARN_REGION`,
`AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS`, etc.).

### Rust SDK / aws-config behavior

**The Rust `aws-config::profile::parser` does NOT provide structured
sub-section access.** Verified 2026-04-29. Source:
`aws-runtime/src/env_config/parse.rs` — sub-properties are parsed but
stored as the raw multi-line string value of the parent key. The
parser's own doc comment states: *"Sub-properties must be validated for
compatibility with other SDKs, but they are not actually parsed into
structured data."*

So for `[s3]` sub-section access, `Profile::get("s3")` returns something
like `"\n  addressing_style = path\n  use_accelerate_endpoint = true"` —
a multi-line string we'd have to re-parse ourselves.

Meanwhile, `aws-config` has default-providers for only **one** of the
S3-specific keys: `use_dualstack_endpoint`. It's picked up as a
top-level profile key via `default_provider/use_dual_stack.rs`.

Every other S3 key (`addressing_style`, `use_accelerate_endpoint`,
`use_arn_region`, `s3_disable_multiregion_access_points`,
`payload_signing_enabled`) is **not read by aws-config** — neither from
the profile nor from `AWS_S3_*` env vars. The SDK accepts these on
`aws_sdk_s3::config::Builder` (`force_path_style`, `accelerate`,
`use_arn_region`, `disable_multi_region_access_points`) but we feed
them ourselves from the profile `[s3]` sub-section and env vars.

### Per-key reference

| Config Key | Env Var | Default | Effect | Status |
|---|---|---|---|---|
| `addressing_style` | — | `auto` | `path`/`virtual`/`auto` — host-style vs path-style | ✅ Wired (profile `[s3]` sub-section) |
| `use_accelerate_endpoint` | — | `false` | Transfer Acceleration endpoint | ✅ Wired (profile `[s3]` sub-section) |
| `use_dualstack_endpoint` | `AWS_USE_DUALSTACK_ENDPOINT` | `false` | IPv6 dualstack endpoint | ✅ Auto-read by aws-config |
| `use_arn_region` | `AWS_S3_USE_ARN_REGION` | `true` | Route to ARN's region | ✅ Wired (env var + profile) |
| `s3_disable_multiregion_access_points` | `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS` | `false` | Block MRAP usage | ✅ Wired (env var + profile) |
| `payload_signing_enabled` | — | varies | Sign request payload | Partial match. TM unconditionally calls `.disable_payload_signing()` on PutObject/UploadPart — matches Python's *default* behavior (HTTPS + checksum → disable). But Python **honors explicit `payload_signing_enabled = true`** from user config, overriding the default. TM has no hook to re-enable. Tracked as upstream TM gap: should be configurable (default disable, allow override). |
| `signature_version` | — | `s3v4` | `s3`/`s3v4`/`s3v4a` — historical | No-op (Rust SDK is SigV4-only for S3; SigV4a for MRAP is automatic) |

### Help System

Python's `aws s3 help` and `aws s3 cp help` use a trailing `help`
positional argument that renders man-page-style output through a pager.
`--help` is **not recognized** by Python (`Unknown options: --help`).

Our CLI uses clap's built-in `--help` flag (standard for Rust CLIs) and
does not recognize `help` as a positional. Two gaps:

1. **`help` positional not recognized** — `aws s3 help` returns exit 252
   with "unrecognized subcommand." Users migrating from Python will hit
   this immediately.
2. **Arg descriptions missing** — `TransferArgs`, `CpArgs`, `MvArgs`,
   `SyncArgs`, `RmArgs` fields have no doc comments, so `--help` output
   shows empty descriptions for ~40 flags. Global args are documented.

Resolution options for (1):
- Add `help` as a hidden subcommand that prints the same as `--help`
- Or: add `help` as a hidden subcommand that renders richer docs (closer
  to Python's man-page format)

Resolution for (2): add `///` doc comments to all arg fields in cli.rs,
matching Python's help text from `awscli/customizations/s3/subcommands.py`.
| `us_east_1_regional_endpoint` | `AWS_S3_US_EAST_1_REGIONAL_ENDPOINT` | `regional` | `regional` vs `legacy` for us-east-1 | Not wired (SDK endpoint resolver has `use_global_endpoint` param but it's not exposed on S3 Config builder — needs custom endpoint params plugin or interceptor) |
| `multipart_threshold` | — | `8MB` | Min size for multipart upload | Not wired (TM-mapped) |
| `multipart_chunksize` | — | `8MB` | Part size for multipart | Not wired (TM-mapped) |
| `max_concurrent_requests` | — | `10` | Request concurrency cap | Not wired (fleet-mode bridge) |
| `max_bandwidth` | — | `None` | Bandwidth cap (bytes/sec) | Not wired (TM-blocked) |
| `preferred_transfer_client` | — | `auto` | `auto`/`classic`/`crt` | No-op (we're TM-only; accept any value) |
| `target_bandwidth` | — | `None` | CRT bandwidth target | Not wired (fleet-mode bridge) |
| `max_queue_size` | — | `1000` | Request queue depth | No-op (classic implementation detail) |
| `io_chunksize` | — | `256KB` | File I/O chunk size | No-op (classic implementation detail) |
| `should_stream` | — | `None` | CRT streaming flag | No-op |
| `disk_throughput` | — | `None` | CRT disk hint | No-op |
| `direct_io` | — | `None` | CRT O_DIRECT flag | No-op |

See `crates/docs/s3-config-keys.md` for full classification rationale
and wiring priority.

### Mechanism gap (summary)

| Concern | Python | Rust (today) |
|---|---|---|
| Parse `[profile X]` top-level keys | botocore does | `aws-config` does |
| Parse `[s3]` sub-section inside profile | botocore's `SectionConfigProvider` descends | `aws-config` parses but doesn't expose structured sub-section access |
| `use_dualstack_endpoint` profile key | Honored | Honored (top-level provider) |
| `use_fips_endpoint` profile key | Honored | Honored (top-level provider) |
| `addressing_style`, `use_accelerate_endpoint`, `use_arn_region`, `payload_signing_enabled`, `s3_disable_multiregion_access_points`, `us_east_1_regional_endpoint` | Honored via `SectionConfigProvider('s3', ...)` | **NOT read from profile** (SDK has the knobs but doesn't discover the values from config) |
| `AWS_S3_USE_ARN_REGION`, `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS` env vars | Honored | **NOT read** |
| `AWS_S3_USE_ACCELERATE_ENDPOINT`, `AWS_S3_ADDRESSING_STYLE`, `AWS_S3_US_EAST_1_REGIONAL_ENDPOINT` env vars | Honored | **NOT read** |

### Resolution plan

Three-part wiring job in our CLI crate:

1. **Read top-level profile keys via aws-config.** `region`,
   `endpoint_url`, `use_dualstack_endpoint`, `use_fips_endpoint` —
   already handled.

2. **Read `[s3]` sub-section ourselves.** Call
   `aws_config::profile::load(...)` to get a `ProfileSet`, then
   `profile.get("s3")` to get the raw multi-line sub-section string.
   Parse it as additional `key = value` pairs. Apply to the
   `aws_sdk_s3::config::Builder` (not `aws_config::ConfigLoader` —
   these are S3-client-scoped, not session-scoped).

3. **Read `AWS_S3_*` env vars ourselves.** Simple `std::env::var`
   lookup with a precedence chain: `AWS_S3_*` env > profile `[s3]`
   sub-section > SDK default.

Per-key priority in `crates/docs/s3-config-keys.md`. Wiring order:

1. SDK-mapped keys: `addressing_style`, `use_accelerate_endpoint`,
   `use_dualstack_endpoint` (trivial — already auto-read),
   `use_arn_region`, `s3_disable_multiregion_access_points`
2. TM-mapped transfer tuning: `multipart_threshold`, `multipart_chunksize`
3. Fleet-mode bridge: `max_concurrent_requests` (per-process cap)
4. Track upstream TM work: `max_bandwidth`
5. CRT-specific no-ops (accept, log at debug, document)
6. Python-quirk no-ops (accept, ignore, document)

**Python tests affected:**
- `test_can_support_addressing_mode_config` (presign) — `addressing_style`
- Various cp/sync tests that implicitly depend on default addressing

Not yet wired into `build_context()`.
