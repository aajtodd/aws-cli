# S3 Config Keys

Every `[s3]` config-file key, `AWS_S3_*` environment variable, and
`~/.aws/config`-level knob that affects `aws s3` behavior. This is the
authoritative reference for the S3 configuration surface.

> Based on aws-cli 2.34.34. Sources: `awscli/customizations/s3/transferconfig.py::DEFAULTS`,
> `botocore/configprovider.py::DEFAULT_S3_CONFIG_VARS`.

## How Config Keys Are Read

All keys live in the `[profile NAME]` section's nested `s3 =` subsection
of `~/.aws/config`. The Rust SDK's profile parser stores sub-sections as
raw strings — we parse them ourselves in `config.rs`.

**Priority chain per key:** `AWS_S3_<KEY>` env var > profile `[s3]`
sub-section > SDK default. Matches Python's `SectionConfigProvider`.

## Transfer Tuning Keys

From `transferconfig.py::DEFAULTS`. Affect cp, mv, sync only.

| Key | Default | Python Behavior | Status |
|-----|---------|-----------------|--------|
| `multipart_threshold` | 8 MiB | Min object size triggering multipart. Accepts human-readable (`64MB`). | Wired → TM `multipart_threshold` |
| `multipart_chunksize` | 8 MiB | Part size for multipart transfers. Human-readable OK. | Wired → TM `part_size` |
| `max_concurrent_requests` | 10 | Request concurrency cap. | Not wired — TM has no equivalent knob |
| `max_bandwidth` | None | Bandwidth cap (bytes/sec). `10MB/s`, `80Mb/s`. | Not wired — TM has no rate limiter |
| `target_bandwidth` | None | CRT bandwidth target hint. | Parsed, stored; TM implementation pending |
| `max_queue_size` | 1000 | Classic s3transfer queue depth. | No-op (TM implementation detail) |
| `io_chunksize` | 256 KiB | Classic s3transfer file I/O chunk. | No-op (TM manages internally) |
| `preferred_transfer_client` | `auto` | `auto`/`classic`/`crt`/`default`. Selects transfer backend. | No-op (TM-only) |
| `should_stream` | — | CRT-specific streaming flag. | No-op |
| `disk_throughput` | — | CRT-specific disk hint. | No-op |
| `direct_io` | — | CRT-specific O_DIRECT flag. | No-op |

## SDK-Level S3 Keys

From `botocore/configprovider.py::DEFAULT_S3_CONFIG_VARS`. Affect all commands.

| Key | Env Var | Python Behavior | Status |
|-----|---------|-----------------|--------|
| `addressing_style` | `AWS_S3_ADDRESSING_STYLE` | `path`/`virtual`/`auto`. Forces addressing mode. | Wired → `force_path_style(true)` |
| `use_accelerate_endpoint` | `AWS_S3_USE_ACCELERATE_ENDPOINT` | Boolean. Routes through Transfer Acceleration. | Wired → `accelerate(true)` |
| `use_dualstack_endpoint` | `AWS_USE_DUALSTACK_ENDPOINT` | Boolean. IPv6-capable dualstack endpoints. | Auto-read by aws-config |
| `use_arn_region` | `AWS_S3_USE_ARN_REGION` | Boolean. Route to ARN's region for access points. Default `true`. | Wired → `use_arn_region(bool)` |
| `s3_disable_multiregion_access_points` | `AWS_S3_DISABLE_MULTIREGION_ACCESS_POINTS` | Boolean. Blocks MRAP ARN requests. | Wired → `disable_multi_region_access_points(true)` |
| `payload_signing_enabled` | — | Boolean. Controls SigV4 payload signing. | Not wired — TM unconditionally disables; no config hook |
| `us_east_1_regional_endpoint` | `AWS_S3_US_EAST_1_REGIONAL_ENDPOINT` | `regional`/`legacy`. Controls us-east-1 endpoint hostname. | Not wired — needs custom endpoint params plugin |

## Session-Level Keys (Affecting S3)

| Key | Env Var | Python Behavior | Status |
|-----|---------|-----------------|--------|
| `region` | `AWS_DEFAULT_REGION`, `AWS_REGION` | Region for requests. | Handled by `--region` and aws-config chain |
| `signature_version` | — | Historical (`s3v4`/`v4`). S3 is SigV4-only now. | No-op |
| `s3_endpoint_url` | `AWS_ENDPOINT_URL_S3` | S3-specific endpoint override. | Handled by aws-config service endpoint resolution |
| `retry_mode` | `AWS_RETRY_MODE` | `legacy`/`standard`/`adaptive`. | Handled by aws-config |

## CRT Process Lock Context

Several transfer tuning keys exist because of the CRT process lock
(`awscrt.s3.CrossProcessLock`). Python CLI uses it to enforce one CRT
instance per host — concurrent CLI processes silently fall back to
classic s3transfer.

The keys `preferred_transfer_client`, `target_bandwidth`, and
`max_concurrent_requests` serve dual purposes: CRT arbitration (not
relevant to us) and user intent signaling ("I'm one of many processes,
limit yourself"). The user-intent dimension matters for any future
multi-process coordination.
