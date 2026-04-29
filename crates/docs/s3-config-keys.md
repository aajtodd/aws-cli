# S3 Config Keys Landscape

Enumerates every `[s3]` config-file key, every `AWS_S3_*` environment
variable, and every `~/.aws/config`-level knob that affects `aws s3`
behavior in the Python CLI. For each: what it does, where it's consumed,
and our decision for the Rust implementation.

This is the authoritative source for "what configuration surface does
`aws s3` accept." Anything a Python CLI user has in their config files
that affects `aws s3` should appear here with a decision.

## Decision classification

Each key falls into one of these buckets:

- **SDK-mapped** — translates to an `aws_sdk_s3::config::Config` setting.
  The Rust SDK supports an equivalent and we wire it through.
- **TM-mapped** — translates to a `aws_s3_transfer_manager::Config`
  setting. Only affects transfer commands (cp/mv/sync); no-op on
  ls/mb/rb/rm/presign/website.
- **TM-blocked** — semantically a TM concern but TM doesn't currently
  expose the knob. Tracked as upstream TM work. Honor-as-best-effort
  or document as compat gap depending on severity.
- **CRT-specific (no-op)** — key only makes sense when Python CLI uses
  the CRT transfer client. We're TM-only; the key is ignored with a
  compat note.
- **Fleet-mode bridge** — key is Python CLI's way for users to tame
  multi-process behavior. These are the first-class input to our
  eventual fleet-mode design. Honor today where trivial; design around
  them in the fleet-mode mode doc.
- **Python-quirk (no-op)** — key exists in Python because of botocore's
  history, doesn't affect any S3 behavior we care about.

## CRT Process Lock — context

Before the key table: the reason several of these keys exist is the
**CRT process lock** (`awscrt.s3.CrossProcessLock`). Python AWS CLI uses
it to enforce one CRT instance per host because CRT performs poorly
with multiple instances running simultaneously.

Mechanism (see `awscli/s3transfer/crt.py::acquire_crt_s3_process_lock`
and `awscli/customizations/s3/factory.py::_resolve_transfer_client_type_for_system`):
1. First CLI process to start acquires the lock → uses CRT
2. Concurrent CLI processes find the lock held → silently fall back to
   classic s3transfer
3. Users have no way to see which processes fell back without
   `--debug` output

**Implication for our keys:** `preferred_transfer_client` is the user's
explicit override of this auto-resolution. `target_bandwidth` is the
signal that a user has tuned for fleet use. We're TM-only with no
equivalent per-host lock needed, so these keys' *primary purpose*
(CRT arbitration) doesn't apply — but their *user intent* (I'm one of
many processes, behave accordingly) still matters. See "Fleet-mode
bridge" class.

## Source of truth

- **CLI-level S3 runtime keys:** `awscli/customizations/s3/transferconfig.py::DEFAULTS`
- **botocore session-level S3 keys:** `awscli/botocore/configprovider.py::DEFAULT_S3_CONFIG_VARS`
- **CRT client creation:** `awscli/customizations/s3/factory.py::_create_crt_client`
- **Process lock:** `awscli/s3transfer/crt.py::acquire_crt_s3_process_lock`
- **Docs:** https://docs.aws.amazon.com/cli/v1/userguide/cli-configure-files.html
  and child pages

## Key Table

All keys live in the `[profile NAME]` or `[default]` section's nested
`s3 = ...` subsection of `~/.aws/config` (or under `[s3]` at the top
level; botocore merges). Env-var column lists the canonical env-var
override if one exists.

## Mechanism: how we read these in Rust

Important discovery from auditing `aws-config`'s profile parser:

1. **`aws_config::profile::parser::load()`** returns `ProfileSet`. Each
   `Profile` exposes `get(key) -> Option<&str>` for **top-level** profile keys.

2. **Sub-properties are NOT structured.** The parser recognizes
   `[s3]`-style sub-sections syntactically (indented `key = value`
   lines under a parent key) but stores the entire sub-section as the
   raw multi-line string value of the parent key. Quote from parser
   source (`aws-runtime/src/env_config/parse.rs`): *"Sub-properties
   must be validated for compatibility with other SDKs, but they are
   not actually parsed into structured data."*

3. **`aws-config` has default-providers for only ONE S3 key:**
   `use_dualstack_endpoint` (also `use_fips_endpoint`). Everything else
   in the `[s3]` sub-section is invisible to the SDK unless a caller
   reads and applies it.

4. **Implication:** to match Python's `SectionConfigProvider`, we need
   to:
   a. Call `aws_config::profile::load(...)` to get a `ProfileSet`
   b. Call `profile.get("s3")` to get the raw sub-section string
   c. Parse that string ourselves as `key = value` lines (trivial)
   d. Read `AWS_S3_*` env vars directly via `std::env::var`
   e. Apply the results to `aws_sdk_s3::config::Builder` — not to
      `aws_config::ConfigLoader`, because these are S3-client-scoped,
      not session-scoped

5. **Priority chain per key:** `AWS_S3_<KEY>` env var > profile `[s3]`
   sub-section key > profile top-level key (for the few that are
   top-level) > SDK default. Matches Python's behavior.

This is a CLI-layer responsibility. No upstream SDK fix needed.

### Transfer tuning (CLI-level, from `transferconfig.py::DEFAULTS`)

| Key | Env var | Python behavior | Our class | Our decision |
|---|---|---|---|---|
| `multipart_threshold` | — | Min object size (bytes) that triggers multipart upload. Default 8 MiB. Accepts human-readable (`64MB`). | TM-mapped | Honor. Wire through to TM's equivalent. Accept human-readable parsing. |
| `multipart_chunksize` | — | Part size for multipart transfers. Default 8 MiB. Human-readable OK. | TM-mapped | Honor. TM's `PartSize::Fixed(n)` replaces `PartSize::Auto` when user sets this. |
| `max_concurrent_requests` | — | Classic s3transfer's request concurrency cap. Default 10. | Fleet-mode bridge | Map to TM's concurrency mode cap. Python's default of 10 is very conservative — TM's Auto mode will usually saturate beyond this. Honor the user's explicit value as an upper bound. |
| `max_queue_size` | — | Classic s3transfer's request queue depth. Default 1000. | Python-quirk (no-op) | Classic implementation detail. TM doesn't have a directly equivalent queue. Document as no-op. |
| `max_bandwidth` | — | Classic s3transfer's bandwidth cap (bytes/sec). `10MB/s`, `80Mb/s`. | TM-blocked | TM doesn't expose a bandwidth limiter today. Track as upstream TM work; compat-note until then. |
| `preferred_transfer_client` | — | `auto`/`classic`/`crt`/`default`. Selects the transfer backend. `default` aliases to `classic`. | CRT-specific (no-op) | We're TM-only. Accept any value, log at debug if it's `crt` (user expected CRT performance — now they're getting TM's equivalent or better). Compat note. |
| `target_bandwidth` | — | Hint to CRT for its bandwidth target (bytes/sec or `10GB/s`). Only used when CRT is selected. | Fleet-mode bridge | Not wired today. When fleet-mode lands, this is the canonical signal that a user has budgeted this process's share. |
| `io_chunksize` | — | Classic s3transfer's file I/O chunk size. Default 256 KiB. | Python-quirk (no-op) | Classic implementation detail. TM manages I/O chunking internally based on part size. Document as no-op. |
| `should_stream` | — | CRT-specific flag to stream file I/O rather than buffer. Rarely set by users. | CRT-specific (no-op) | No-op. |
| `disk_throughput` | — | CRT-specific disk-throughput hint. Rarely set. | CRT-specific (no-op) | No-op. |
| `direct_io` | — | CRT-specific flag to use O_DIRECT. | CRT-specific (no-op) | No-op. |

### SDK-level S3 keys (from `botocore/configprovider.py::DEFAULT_S3_CONFIG_VARS`)

| Key | Env var | Python behavior | Our class | Our decision |
|---|---|---|---|---|
| `addressing_style` | `AWS_S3_ADDRESSING_STYLE` | `path`/`virtual`/`auto`. Forces path-style or virtual-hosted-style addressing. | SDK-mapped | **Read ourselves** (profile `[s3]` sub-section + env var). Wire to `Config::builder().force_path_style(true)` when `path`. `virtual` and `auto` use SDK default. |
| `use_accelerate_endpoint` | `AWS_S3_USE_ACCELERATE_ENDPOINT` | Boolean. Routes through S3 Transfer Acceleration. | SDK-mapped | **Read ourselves**. Wire to `Config::builder().accelerate(true)`. |
| `use_dualstack_endpoint` | `AWS_USE_DUALSTACK_ENDPOINT` | Boolean. Uses IPv6-capable dualstack endpoints. | SDK-mapped | **Already auto-read** by `aws-config::default_provider::use_dual_stack`. No code needed. |
| `payload_signing_enabled` | — | Boolean. Controls SigV4 payload signing. Python's logic: explicit config wins; else HTTPS + checksum + streaming → disable; else enable. | TM-blocked | TM unconditionally disables payload signing on PutObject/UploadPart (correct default for performance). But Python honors explicit `true` from user config. TM has no hook to re-enable. Track as upstream TM gap: `payload_signing_enabled` should be configurable on TM config (default: disabled). |
| `use_arn_region` | `AWS_S3_USE_ARN_REGION` | Boolean. When using access-point ARNs, route to the ARN's region rather than the client's region. Default `true`. | SDK-mapped | **Read ourselves**. Wire to `Config::builder().use_arn_region(true/false)`. Default true in both SDKs' endpoint rulesets. |
| `s3_disable_multiregion_access_points` | `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS` | Boolean. Blocks requests to MRAP ARNs. | SDK-mapped | **Read ourselves**. Wire to `Config::builder().disable_multi_region_access_points(true)`. |

### Other relevant session config

| Key | Env var | Python behavior | Our class | Our decision |
|---|---|---|---|---|
| `region` | `AWS_DEFAULT_REGION`, `AWS_REGION` | Region for requests. | SDK-mapped | Already honored via `--region` and standard aws_config chain. |
| `signature_version` | — | `s3v4`/`v4`/etc. Historical — S3 is SigV4-only now. | Python-quirk (no-op) | Accept any value. Log at debug if non-default. Rust SDK uses SigV4 universally for S3. |
| `us_east_1_regional_endpoint` | `AWS_S3_US_EAST_1_REGIONAL_ENDPOINT` | `regional`/`legacy`. Controls whether us-east-1 uses `s3.amazonaws.com` (legacy) or `s3.us-east-1.amazonaws.com` (regional). Default `regional`. | SDK-mapped | **Read ourselves**. Endpoint resolver has `use_global_endpoint` param (defaults `false`). Not exposed on S3 Config builder — needs custom endpoint params plugin or interceptor to set `use_global_endpoint = true` when user config says `legacy`. Real divergence for users with `legacy` set. |
| `s3_endpoint_url` | `AWS_ENDPOINT_URL_S3` | S3-specific endpoint override. | SDK-mapped | Honor via aws_config's service-specific endpoint resolution. |
| `retry_mode` | `AWS_RETRY_MODE` | `legacy`/`standard`/`adaptive`. | SDK-mapped | Already honored via aws_config. Tracked separately in compat.md §retry parity. |

## Fleet-mode bridge — what this means concretely

The keys in the "Fleet-mode bridge" class — `max_concurrent_requests`,
`target_bandwidth`, and arguably `max_bandwidth` once TM supports it —
are what a user sets when they've already figured out "I'm one of N
CLI processes and need to self-limit." In Python CLI, these keys tame
CRT or classic s3transfer per-process; in our world, they tame TM.

Without an explicit fleet-mode design, the minimum bar is: **honor
these values as per-process upper bounds** and let TM's autotuning
adjust within that cap. A user who had `max_concurrent_requests = 5`
in their config because they ran 8x `aws s3 cp` in parallel shouldn't
have that value silently ignored.

When we build an explicit fleet-mode (opt-in `--fleet-size N` or
similar — see the multi-process design thread for details),
these keys remain the user-facing surface.

## Priority for wiring

Based on this classification, the order we should implement:

1. **SDK-mapped (addressing_style, use_accelerate_endpoint,
   use_dualstack_endpoint, use_arn_region, s3_disable_multiregion_access_points)**
   — Trivial, high-compat value, no design questions. One PR.
2. **TM-mapped transfer tuning (multipart_threshold, multipart_chunksize)**
   — Small, affects cp/mv/sync performance tuning. TM accepts these.
3. **Fleet-mode bridge (max_concurrent_requests)** — Honor as
   per-process cap. Design decision: do we derive a concurrency cap
   for TM from this value? Yes. Simple translation initially; revisit
   in fleet-mode design.
4. **TM-blocked (max_bandwidth)** — Track upstream. Compat note until
   TM exposes a bandwidth limiter.
5. **CRT-specific no-ops** — Accept, log at debug, document. Zero
   runtime effect. Low cost.
6. **Python-quirk no-ops** — Accept, ignore. Document.

## Out of scope for this doc

- **Retry behavior parity** (`retry_mode`, retries config) — covered
  separately in compat.md §retry.
- **Credential resolution keys** — covered in compat.md §Credential
  Resolution.
- **General endpoint resolution** (FIPS, custom CA roots, etc.) —
  covered in compat.md §Endpoint Resolution.
