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
├── config.rs       # [s3] config key parsing, HTTP client/timeout builders
├── context.rs      # AppContext (client, sdk_config, globals, s3_config_keys, terminal)
├── error.rs        # CommandError, CommandErrorKind, format_sdk_error, exit code mapping
├── transfer.rs     # TM client builder, upload_single, download_single, guess_content_type
├── redirect.rs     # RegionRedirectInterceptor, region_redirect_classifier
├── format.rs       # Date/time formatting (jiff), human-readable sizes
├── term.rs         # TermOutput/Terminal traits, StdTerminal, InMemoryTerminal
├── uri.rs          # S3Uri, TransferUri, path type validation
├── arn.rs          # Access point ARN structural parser
├── paths.rs        # format_local_path (Python os.path.relpath parity)
└── commands/
    ├── mod.rs      # dispatch(S3Command, &AppContext)
    ├── ls.rs       # ListBuckets / ListObjectsV2 (pagination, recursive, summarize)
    ├── cp.rs       # Upload / download via Transfer Manager
    ├── mv.rs       # Upload/download + delete source
    ├── rm.rs       # DeleteObject (single + recursive)
    ├── mb.rs       # CreateBucket
    ├── rb.rs       # DeleteBucket
    ├── presign.rs  # GetObject presigned URL
    └── website.rs  # Get/PutBucketWebsite
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
    pub sdk_config: SdkConfig,
    pub globals: GlobalArgs,
    pub s3_config_keys: S3ConfigKeys,
    pub term: Box<dyn Terminal>,
}
```

- `client` — pre-configured S3 SDK client (with redirect interceptor)
- `sdk_config` — full SDK config (for building TM clients with same settings)
- `globals` — parsed global flags (`--region`, `--profile`, `--endpoint-url`, etc.)
- `s3_config_keys` — parsed `[s3]` config section (addressing_style, thresholds, etc.)
- `term` — injectable terminal for output. Production uses `StdTerminal`;
  tests use `InMemoryTerminal` backed by `vt100::Parser`

## Terminal Abstraction

Two traits separate output from terminal control:

- `TermOutput` — `write(&self, &str)`, `writeln(&self, &str)`. A single
  output stream (stdout or stderr).
- `Terminal` — `out()`, `err()`, `is_tty()`, `width()`. The whole terminal.

`StdTerminal` wraps real stdout/stderr. `InMemoryTerminal` (test-only)
wraps `vt100::Parser` for verifying terminal output including `\r`
overwrites and ANSI sequences.

Macros `termout!`, `termoutln!`, `termerr!`, `termerrln!` provide formatted
output through the terminal abstraction.

## CLI Structure

All clap definitions live in `cli.rs` for scannability. The full command
surface is visible in one file.

```
aws [globals] s3 <subcommand> [args]
```

`GlobalArgs` covers the options from `awscli/data/cli.json`. S3 subcommands:
`ls`, `cp`, `mv`, `rm`, `sync`, `mb`, `rb`, `presign`, `website`.

Transfer commands (cp, mv, sync) share args via flattened structs:
- `TransferArgs` — encryption, storage class, metadata, content headers,
  dryrun, quiet, progress control, MIME type control
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

`CommandError` carries a `CommandErrorKind` that maps to exit codes:

| Kind | Exit Code | Use |
|------|-----------|-----|
| Failure | 1 | Transfer task failed |
| Warning | 2 | Glacier warnings |
| ParamValidation | 252 | Invalid args, unimplemented features |
| Configuration | 253 | Bad profile, missing config |
| Client | 254 | Service errors (NoSuchBucket, AccessDenied) |
| General | 255 | Unexpected errors |

Service errors are formatted to match `botocore.exceptions.ClientError`:
```
An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}
```

Client errors are prefixed with `\n` on stderr (matches Python behavior).

See [exit-codes.md](exit-codes.md) for the full exit code contract.

## Transfer Architecture

Transfer commands (cp, mv) use the S3 Transfer Manager for uploads and
downloads. The TM manages its own thread pool with CPU-pinned HTTP clients
for bandwidth saturation.

`transfer.rs` provides:
- `build_tm(ctx)` — constructs a TM client from AppContext config
  (multipart_threshold, multipart_chunksize, concurrency mode)
- `upload_single(ctx, source, bucket, key, content_type)` — single-file upload
- `download_single(ctx, bucket, key, dest)` — single-file download
- `guess_content_type(path)` — MIME inference from file extension

Content-type inference uses a compiled-in database (deterministic across
platforms). Controlled by `--no-guess-mime-type` and `--content-type` flags.

## Runtime Strategy

The binary uses a multi-threaded tokio runtime with a small fixed worker
pool (2 threads). This provides concurrency for SDK calls and region
resolution without over-provisioning — transfer commands use the Transfer
Manager's own thread pool for data-path concurrency.

SIGPIPE is reset to `SIG_DFL` at startup on unix.

## Test Strategy

Tests are organized by what they verify:

- **CLI parsing** (`cli.rs`) — clap definitions, global args, error cases
- **SDK interactions** (`commands/*.rs`) — smithy-mocks tests verifying correct
  API call parameters and output formatting
- **Formatting** (`format.rs`) — date/size formatting, ported from Python CLI
- **URI parsing** (`uri.rs`) — S3 URI parsing, TransferUri, path type validation
- **Terminal** (`term.rs`) — InMemoryTerminal correctness
- **Config** (`config.rs`) — [s3] key parsing, profile resolution

See [test-traceability.md](test-traceability.md) for the mapping between
Python CLI tests and Rust equivalents.
