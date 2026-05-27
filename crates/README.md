# aws s3 — Native Rust Implementation

A drop-in replacement for `aws s3`, built on the
[AWS SDK for Rust](https://github.com/awslabs/aws-sdk-rust) and the
[S3 Transfer Manager](https://github.com/awslabs/aws-s3-transfer-manager-rs).
The goal is 100% behavioral compatibility with the Python CLI — same
output, same exit codes, same edge cases — with native performance.

## Quick Start

```sh
cd crates/
cargo build --release
./target/release/aws s3 ls                              # list buckets
./target/release/aws s3 cp file.txt s3://bucket/key     # upload
./target/release/aws s3 cp s3://bucket/key file.txt     # download
```

Global flags (`--region`, `--profile`, `--endpoint-url`, `--debug`, etc.)
work the same as the Python CLI. Unimplemented features exit 252 with a
stderr message.

## Build and Test

```sh
cargo test -p s3-cli
cargo clippy --all-targets
```

## Verifying Compatibility

The `compat/` sibling directory has a backwards-compatibility test
framework. It runs declarative TOML specs against both the Python CLI
(baseline) and this binary, asserting identical observable behavior.

```sh
cd ../compat/
COMPAT_CLI_BINARY=../target/release/aws ./compat.sh test
```

See `../compat/README.md` for the full workflow.

## Code Layout

```
src/
  main.rs        Entry point: arg parsing, SDK config, tracing setup
  lib.rs         handle_s3_cmd() dispatch, exit code constants
  cli.rs         clap definitions (GlobalArgs, CpArgs, LsArgs, etc.)
  config.rs      [s3] config key parsing, HTTP client/timeout builders
  context.rs     AppContext — holds S3 client, SDK config, terminal
  commands/      One module per subcommand (ls.rs, cp.rs, mv.rs, ...)
  transfer.rs    Transfer Manager client builder, upload/download, MIME guessing
  redirect.rs    Cross-region redirect interceptor
  error.rs       CommandError type, exit code mapping
  uri.rs         S3 URI parsing (s3://bucket/key)
  arn.rs         Access point ARN parser
  paths.rs       Relative path display (matches Python's os.path.relpath)
  format.rs      Date/size formatting (matches Python's column widths)
  term.rs        Terminal abstraction (testable stdout/stderr)
```

## Documentation

| Doc | What it's for |
|-----|---------------|
| `docs/compat.md` | Every known behavioral gap — the ship-gate checklist |
| `docs/design.md` | Architecture decisions and module responsibilities |
| `docs/research.md` | Python CLI reference (how it works internally) |
| `docs/exit-codes.md` | Exit code contract |
| `docs/s3-config-keys.md` | All `[s3]` config keys and their status |
| `docs/smoke-testing.md` | Manual verification recipes |
| `docs/test-traceability.md` | Python test → Rust test mapping |
