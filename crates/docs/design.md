# Architecture

Native Rust implementation of `aws s3` subcommands. Library crate with a
binary target. The library entry point (`handle_s3_cmd`) accepts a parsed
command and execution context, making it embeddable in any Rust binary that
wants to provide S3 CLI functionality.

## Module Layout

```
src/
├── lib.rs          # handle_s3_cmd(S3Command, &AppContext) -> i32, exit_code constants
├── main.rs         # Parses CLI, builds SDK config, constructs AppContext
├── cli.rs          # All clap definitions: Cli, GlobalArgs, S3Command, per-command args
├── context.rs      # AppContext (client, globals, terminal)
├── error.rs        # Error enum, format_sdk_error (Python CLI-compatible format)
├── format.rs       # Date/time formatting (jiff), human-readable sizes
├── term.rs         # TermOutput/Terminal traits, StdTerminal, InMemoryTerminal
├── uri.rs          # S3Uri, TransferUri, path type validation
└── commands/
    ├── mod.rs      # dispatch(S3Command, &AppContext)
    └── ls.rs       # ls implementation (buckets, objects, recursive)
```

## Entry Point

```rust
pub async fn handle_s3_cmd(command: S3Command, ctx: &AppContext) -> i32
```

The caller owns parsing and SDK configuration. `handle_s3_cmd` dispatches
to the appropriate subcommand and maps errors to exit codes. This separation
keeps the library testable — tests inject mock clients and in-memory terminals
without subprocess overhead.

`main.rs` is the standalone binary entry point. It parses `aws [globals] s3
<subcommand>` via clap, builds an SDK client from global flags (region,
endpoint, profile), and constructs `AppContext`.

## AppContext

```rust
pub struct AppContext {
    pub client: aws_sdk_s3::Client,
    pub globals: GlobalArgs,
    pub term: Box<dyn Terminal>,
}
```

- `client` — pre-configured S3 SDK client
- `globals` — parsed global flags (`--region`, `--profile`, `--endpoint-url`, etc.)
- `term` — injectable terminal for output. Production uses `StdTerminal`;
  tests use `InMemoryTerminal` backed by `vt100::Parser`

## Terminal Abstraction

Two traits separate output from terminal control:

- `TermOutput` — `write(&self, &str)`, `writeln(&self, &str)`. A single
  output stream (stdout or stderr).
- `Terminal` — `out()`, `err()`, `is_tty()`, `width()`. The whole terminal.

`StdTerminal` wraps real stdout/stderr. `InMemoryTerminal` (test-only,
`#[cfg(test)]`) wraps `vt100::Parser` for verifying terminal output including
`\r` overwrites and ANSI sequences.

Macros `termout!`, `termoutln!`, `termerr!`, `termerrln!` provide formatted
output through the terminal abstraction.

## CLI Structure

All clap definitions live in `cli.rs` for scannability. The full command
surface is visible in one file.

```
aws [globals] s3 <subcommand> [args]
```

`GlobalArgs` covers all 18 options from `awscli/data/cli.json` (validated
by test). Nine S3 subcommands: `ls`, `cp`, `mv`, `rm`, `sync`, `mb`, `rb`,
`presign`, `website`.

Transfer commands (cp, mv, sync) share ~25 args via flattened structs:
- `TransferArgs` — encryption, storage class, metadata, content headers, etc.
- `FilterArgs` — `--include`/`--exclude` with ordering preserved

## TransferUri and Path Validation

```rust
pub enum TransferUri {
    Local(PathBuf),
    S3(S3Uri),
}

pub struct S3Uri {
    pub bucket: String,
    pub key: String,
}
```

`TransferUri` implements `FromStr` — anything starting with `s3://` parses
as S3, everything else as Local.

Path type validation matches the Python CLI's `check_path_type()`:

| PathType | Allowed Commands |
|----------|-----------------|
| LocalToS3 | cp, sync, mv |
| S3ToLocal | cp, sync, mv |
| S3ToS3 | cp, sync, mv |
| S3Only | mb, rb, rm |
| LocalOnly | NEVER |
| LocalToLocal | NEVER |

Error format matches the Python CLI exactly:
```
usage: aws s3 {cmd} {usage}
Error: Invalid argument type
```

## Error Handling

Service errors are formatted to match the Python CLI's `botocore.exceptions.ClientError`:

```
An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}
```

`format_sdk_error` extracts the error code and message via `ProvideErrorMetadata`
on the SDK error. The operation name is passed from the call site since the
Rust SDK doesn't carry it on the error (unlike botocore's `ClientError`).

See [exit-codes.md](exit-codes.md) for the full exit code contract.

## Runtime Strategy

The binary uses a multi-threaded tokio runtime with a small fixed worker
pool (2 threads). This provides concurrency for SDK calls and region
resolution without over-provisioning — transfer commands (cp, mv, sync)
use the Rust Transfer Manager, which manages its own thread pool for the
heavy data-path concurrency.

SIGPIPE is reset to `SIG_DFL` at startup on unix. Windows is deferred
(`#[cfg(windows)] unimplemented!()`).

## Dependencies

| Crate | Purpose |
|-------|---------|
| `aws-config` | SDK configuration loading |
| `aws-sdk-s3` | S3 API client |
| `aws-smithy-types` | `DateTime`, `ErrorMetadata`, `ProvideErrorMetadata` |
| `clap` (derive) | Argument parsing |
| `tokio` (rt, macros) | Async runtime |
| `thiserror` | Error derive |
| `jiff` | Local timezone conversion for date display |
| `libc` | SIGPIPE reset (unix) |

### Dev Dependencies

| Crate | Purpose |
|-------|---------|
| `aws-smithy-mocks` | Mock SDK responses for unit tests |
| `vt100` | In-memory terminal emulator for output tests |
| `aws-sdk-s3` (test-util) | Test utilities for SDK types |

## Test Strategy

Tests are organized by what they verify:

- **CLI parsing** (`cli.rs`) — clap definitions, global args, error cases
- **SDK interactions** (`commands/*.rs`) — smithy-mocks tests verifying correct
  API call parameters and output formatting
- **Formatting** (`format.rs`) — date/size formatting, ported from Python CLI's
  `test_utils.py`
- **URI parsing** (`uri.rs`) — S3 URI parsing, TransferUri, path type validation
  matrix ported from Python CLI's `test_subcommands.py`
- **Terminal** (`term.rs`) — InMemoryTerminal correctness

See [test-traceability.md](test-traceability.md) for the full mapping between
Python CLI tests and Rust equivalents.
