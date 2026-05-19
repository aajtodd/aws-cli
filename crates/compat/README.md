# S3 CLI Backwards Compatibility Testing Framework

Tests the observable behavior of an S3 CLI against a captured baseline (`aws s3`).
Declarative TOML specs describe scenarios; golden files capture baseline output;
the runner diffs actual vs expected.

## Crates

- **s3-compat-spec** — Library: spec types, runner engine, assertion engine, CLI executor, golden file manager, test harness.
- **s3-compat-tests** — Test crate: TOML spec files, golden files, generated tests.

## Quick Start

```bash
# Run all compat tests against Python CLI (default)
./compat.sh test

# Run against the native Rust CLI
COMPAT_CLI_BINARY=../target/release/aws ./compat.sh test

# Run a specific test
./compat.sh test basic_object_listing
```

> **Note:** Tests require `COMPAT_CLI_BINARY` to be set (compat.sh defaults to `aws`
> from PATH). Plain `cargo test -p s3-compat-tests` skips spec tests unless the env
> var is set. Build the Rust CLI first with `cd .. && cargo build --release`.

## compat.sh — Test Runner

```bash
./compat.sh test [filter]       # Run specs against mock, assert golden files
./compat.sh probe [filter]      # Show CLI output (stdout/stderr/exit_code), no assertions
./compat.sh capture [filter]    # Write golden files from CLI output
./compat.sh validate [filter]   # Run specs against prod S3, compare to golden files
```

### Logging

Logging is controlled via `RUST_LOG`, independent of the command:

```bash
# Runner debug logs (setup, execution, assertions)
RUST_LOG=s3_compat_spec=debug ./compat.sh test

# Full trace (runner + mock server request handling)
RUST_LOG=s3_compat_spec=trace,s3_mock_server=trace ./compat.sh test

# Debug a specific failing test
RUST_LOG=s3_compat_spec=debug ./compat.sh test basic_object_listing
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `COMPAT_CLI_BINARY` | CLI binary to test (resolved from PATH) | `aws` |
| `COMPAT_TARGET` | `mock` or `prod` | `mock` |
| `COMPAT_KEEP_TEMPDIR` | Keep temp working dirs for inspection (set to any value) | (unset) |
| `RUST_LOG` | Tracing filter (debug shows setup/exec, trace adds config/env/output) | (none — silent) |

### Workflow: Writing a New Spec

```bash
# 1. Write the spec
vim specs/commands/ls/my_new_test.toml

# 2. Probe: see what the baseline CLI produces
./compat.sh probe my_new_test

# 3. Capture: write golden files from baseline output
./compat.sh capture my_new_test

# 4. Verify: run in test mode
./compat.sh test my_new_test

# 5. Debug if something fails
RUST_LOG=s3_compat_spec=debug ./compat.sh test my_new_test

# 6. Validate against prod S3 (requires AWS credentials)
AWS_PROFILE=my-profile ./compat.sh validate my_new_test
```

### Workflow: Validating Mock Fidelity

Golden files are captured from the mock by default. To verify the mock matches
real S3 behavior:

```bash
# Run all specs against prod, diff against mock-captured golden files
AWS_PROFILE=my-profile ./compat.sh validate

# Divergences indicate mock fidelity gaps:
# - Fix the mock to match prod behavior, OR
# - Mark the spec as target = "mock_only" or "prod_only"
```

### How Prod Credentials Work

When running against prod (`validate` or `COMPAT_TARGET=prod`):

1. The harness resolves credentials once at startup using the standard AWS SDK
   credential chain (respects `AWS_PROFILE`, env vars, instance metadata, etc.)
2. Resolved credentials (access key, secret, session token) are stored in memory
3. Each CLI invocation receives credentials as explicit env vars
   (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`)
4. The CLI never sees `AWS_PROFILE` — it gets static creds directly

This means `AWS_PROFILE` only needs to be set in your shell when launching
compat.sh. The CLI process runs in a fully isolated environment.

### How Prod Buckets Work

- Bucket names include a run-unique ID: `s3-compat-{run_id}-bucket-{spec_name}`
- On init, the harness deletes all stale `s3-compat-*` buckets from previous runs
- Between tests, buckets are emptied and reused
- On process exit, final buckets are orphaned but cleaned up on next run

## Spec Format

Specs are TOML files in `specs/commands/{command}/`:

```toml
[test]
name = "basic_object_listing"
description = "ls lists objects with date/size/key column formatting"
tags = ["ls", "output-format"]

[[setup.buckets]]
name = "{bucket}"

[[setup.objects]]
key = "file1.txt"
content = "hello"
last_modified = "2024-06-15T14:30:00Z"

[command]
args = ["s3", "ls", "s3://{bucket}/"]

[expected]
exit_code = 0

[expected.stdout]
mode = "golden"
```

### Setup

```toml
# Explicit bucket creation (required for empty bucket tests)
[[setup.buckets]]
name = "{bucket}"

[[setup.buckets]]
name = "{dest}"

# Objects (buckets created implicitly if setup.buckets is omitted)
[[setup.objects]]
bucket = "{bucket}"          # default: "{bucket}"
key = "file.txt"
content = "hello"            # inline content
size = 1048576               # OR generated content of this size
content_type = "text/plain"  # optional
last_modified = "2024-06-15T14:30:00Z"  # optional, mock-only
metadata = { author = "test" }          # optional
```

### Assertion Modes

| Mode | Description |
|------|-------------|
| `golden` | Compare against `.stdout.golden` / `.stderr.golden` file |
| `exact` | Inline exact string match |
| `contains` | All substrings must appear |
| `regex` | All patterns must match |
| `unordered` | All lines present, any order |

Prefer `golden` or `exact` over `contains` — be as precise as possible.

### Golden Files

- Stored alongside specs: `my_test.stdout.golden`, `my_test.stderr.golden`
- Golden file exists → compare against it
- Golden file absent → assert output is empty
- No golden files at all → error ("run with COMPAT_MODE=capture")
- Capture writes only non-empty outputs
- Golden files are mock-captured by default; validate against prod periodically

### Placeholders

- `{bucket}`, `{source}`, `{dest}` — replaced with deterministic bucket names per spec
- Custom names declared via `setup.buckets`
- Output is normalized (bucket names → placeholders) before comparison

## Design

See `../docs/` for CLI design and compatibility documentation.

Key concepts:
- **Baseline** — the reference CLI implementation (currently `aws s3`)
- **Golden files** — captured baseline output, the source of truth
- **Deviations** — intentional differences from baseline, tracked with rationale
- **TestHarness** — manages mock server backends, leases test environments
- **TestEnv** — per-test environment with backend, placeholders, working dir, credentials
- **Mock fidelity** — validated by running specs against prod and diffing
