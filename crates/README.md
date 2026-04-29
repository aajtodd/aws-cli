# aws s3 — Native Rust Implementation

A native Rust implementation of the `aws s3` CLI, built on the
[AWS SDK for Rust](https://github.com/awslabs/aws-sdk-rust) and the
[S3 Transfer Manager](https://github.com/awslabs/aws-s3-transfer-manager-rs).

## Architecture

```
main.rs          CLI entry point, arg parsing, SDK config, tracing
  ├── cli.rs     clap definitions (GlobalArgs, per-command Args)
  ├── config.rs  SDK config builders (HTTP client, timeouts, [s3] sub-section)
  ├── context.rs AppContext (S3 client, SdkConfig, globals, terminal)
  └── lib.rs     dispatch: handle_s3_cmd(S3Command, &AppContext) -> u8

commands/
  ├── ls.rs      ListBuckets / ListObjectsV2
  ├── cp.rs      Upload / download via Transfer Manager
  ├── mb.rs      CreateBucket
  ├── rb.rs      DeleteBucket
  ├── rm.rs      DeleteObject (single + recursive)
  ├── presign.rs GetObject presigned URL
  └── website.rs Get/PutBucketWebsite

uri.rs           S3 URI + access point ARN parsing
arn.rs           Dedicated ARN structural parser (no regex)
paths.rs         Local path display (Python relative_path parity)
format.rs        Date/size formatting (Python parity)
term.rs          Terminal abstraction (stdout/stderr, testable)
error.rs         CommandError type, exit code policy
```

Transfer commands (cp, mv, sync) use the S3 Transfer Manager's managed
runtime — per-thread tokio runtimes with CPU-pinned HTTP clients for
bandwidth saturation. Non-transfer commands use the SDK S3 client
directly.

## Build and Test

```sh
cd crates/
cargo build
cargo test -p s3-cli
cargo clippy --all-targets
```

The binary is at `target/debug/aws` (or `target/release/aws`).

```sh
# Smoke test against real S3
AWS_PROFILE=your-profile ./target/debug/aws --region us-east-2 s3 ls
```

See `docs/smoke-testing.md` for comprehensive manual test recipes.

## Current State

**Commands:** ls, mb, rb, rm, presign, website — complete. cp upload +
download single-file — working via TM. mv, sync, cp recursive — not
yet implemented (blocked on TM upstream work).

**Global flags:** `--region`, `--endpoint-url`, `--profile`,
`--no-sign-request`, `--cli-read-timeout`, `--cli-connect-timeout`,
`--debug` — wired. `--no-verify-ssl`, `--ca-bundle` — rejected at
arg-parse (upstream SDK gaps).

**Config:** `[s3]` sub-section parsing helper exists but is not yet
wired into client construction. SDK-mapped keys (`addressing_style`,
`use_accelerate_endpoint`, `use_arn_region`, etc.) are the next
wiring target.

**Tests:** 214 passing + 6 ignored, zero clippy warnings.

## Documentation

| Doc | Purpose |
|-----|---------|
| `docs/design.md` | Architecture, module layout, runtime, dependencies |
| `docs/research.md` | Python CLI reference (commands, args, output formats) |
| `docs/compat.md` | SDK vs botocore behavioral differences — the ship-gate ledger |
| `docs/s3-config-keys.md` | Every `[s3]` config key + `AWS_S3_*` env var, classified |
| `docs/test-traceability.md` | Python→Rust test mapping, exit codes, known gaps |
| `docs/exit-codes.md` | Exit code contract |
| `docs/smoke-testing.md` | Manual test recipes for flag behaviors |

## Dependencies

- `aws-config` (explicit features: `rt-tokio`, `default-https-client`, `sso`, `credentials-process`)
- `aws-sdk-s3`
- `aws-s3-transfer-manager` (git dep, branch `s3-tm-vnext`)
- `aws-smithy-http-client` (rustls + aws-lc)
- `aws-smithy-types`, `aws-smithy-runtime-api`
- `aws-runtime`, `aws-types`
- `clap` (derive)
- `tokio` (multi-thread runtime)
- `tracing-subscriber` (fmt, env-filter — for `--debug`)
- `thiserror`
- `rustls`, `rustls-pemfile` (retained for CA bundle re-enable path)
