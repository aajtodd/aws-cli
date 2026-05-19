---
inclusion: always
foundationalType: tech
---

# Technology Stack

## Workspace

Two-crate Cargo workspace at the repo root:

- **`s3-compat-spec`** — library crate. Spec data model, TOML parsing, runner,
  assertion engine, CLI executor, golden file handling, mock/prod backend
  abstraction, test harness.
- **`s3-compat-tests`** — test crate. TOML spec files under `specs/`, golden
  files alongside specs (`*.stdout.golden`, `*.stderr.golden`), a `build.rs`
  that generates one `#[test]` per spec.

## External Dependency: s3-tm-vnext-mock

The mock S3 server comes from a **separate repository**:
[`awslabs/aws-s3-transfer-manager-rs`](https://github.com/awslabs/aws-s3-transfer-manager-rs)
on the `s3-tm-vnext-mock` branch (crate `s3-mock-server/`). It's
pulled in via a path dependency in `Cargo.toml` — clone it as a sibling
of this repo. Changes to the mock server happen in that repo, not this one.

Common reasons to edit the mock repo:
- Mock fidelity gaps discovered via `./compat.sh validate` (mock-vs-prod
  divergence)
- Missing S3 operations needed for new specs (e.g. CopyObject)

## Key Crates

- **`aws-sdk-s3`** — SDK client used for prod backend + seeding/inspecting mock
- **`aws-smithy-types`** — DateTime parsing, core AWS types
- **`s3s`** — S3 protocol server (used inside s3-mock-server)
- **`tokio`** — async runtime. **Important:** generated tests use a shared
  `OnceLock<Runtime>` via `crate::runtime().block_on(...)`, NOT `#[tokio::test]`.
  Per-test runtimes drop at end of test and abort spawned tasks (including
  mock server accept loops held by static HARNESS). See `shared_runtime_invariant.rs`
  regression guard.
- **`serde`** / **`toml`** — spec deserialization
- **`regex`** — assertion engine's regex mode
- **`similar`** — unified diff output for assertion failures
- **`crc-fast`** — CRC32C integrity verification
- **`tracing`** — structured logging (runner + mock both use this)

## Test Invocation

Never `cargo test -p s3-compat-tests` directly — it skips spec tests when
`COMPAT_CLI_BINARY` isn't set. Always go through `./compat.sh`:

```
./compat.sh test [filter]       # run specs against mock
./compat.sh probe [filter]      # run, dump output, no assertions
./compat.sh capture [filter]    # write golden files from CLI output
./compat.sh validate [filter]   # run against prod S3 (needs AWS creds)
```

## Logging

`RUST_LOG` controls tracing. Common patterns:

- `RUST_LOG=s3_compat_spec=debug` — runner setup/exec/assertions
- `RUST_LOG=s3_compat_spec=trace` — above + full env/config dumps
- `RUST_LOG=s3_compat_spec=trace,s3_mock_server=trace` — + mock request/response
- `RUST_LOG=s3_compat_spec::assertions::object=debug` — per-object field dump

## Prod Credentials

`AWS_PROFILE=<your-profile> ./compat.sh validate` is the canonical invocation
for this workspace. The harness resolves credentials once at startup via the
AWS SDK credential chain and passes static creds (AKID/SECRET/SESSION_TOKEN)
to each CLI invocation as env vars. The CLI process never sees AWS_PROFILE.
