# AGENTS.md

Agent-facing quick reference for the S3 CLI backwards-compatibility testing
framework. For the deep design context, see `.kiro/steering/product.md`,
`tech.md`, `structure.md`. For authoring specs, see
`.kiro/steering/spec-authoring.md` (auto-loaded when you open a `.toml`
spec file).

## What This Is

Tests a new S3 CLI against the reference CLI (`aws s3`). Declarative TOML
specs describe scenarios; runner executes the CLI, diffs output/state
against expectations. Two crates:

- `s3-compat-spec/` — library (runner, assertion engine, backends, harness)
- `s3-compat-tests/` — specs under `specs/`, one `#[test]` per TOML file

## Build / Test Commands

```bash
./compat.sh test [filter]        # run specs against mock
./compat.sh probe [filter]       # run, dump output + post-command state, no assertions
./compat.sh capture [filter]     # write golden files from CLI output
./compat.sh validate [filter]    # run against prod S3 (needs AWS creds)

cargo build
cargo test -p s3-compat-spec --lib    # library unit tests only
cargo fmt
cargo clippy --all-targets
```

**Never invoke `cargo test -p s3-compat-tests` directly** — spec tests skip
when `COMPAT_CLI_BINARY` isn't set. `./compat.sh` wires everything up.

## Credentials (prod validation)

```bash
AWS_PROFILE=<your-profile> ./compat.sh validate [filter]
```

Credentials resolved once at harness init via SDK chain, passed to CLI
invocations as static `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` /
`AWS_SESSION_TOKEN`. Exit code 254 in `validate` output = stale credentials,
not a behavioral failure.

## Logging

`RUST_LOG` controls tracing:

```bash
RUST_LOG=s3_compat_spec=debug ./compat.sh test <filter>
RUST_LOG=s3_compat_spec=trace ./compat.sh probe <filter>         # + env/config dumps
RUST_LOG=s3_compat_spec::assertions::object=debug ./compat.sh test <filter>
```

## The Non-Negotiable Spec Authoring Workflow

1. Write the spec.
2. Probe against prod first (when `AWS_PROFILE` is available):
   `AWS_PROFILE=<your-profile> COMPAT_TARGET=prod ./compat.sh probe <name>`.
   Prod is the source of truth for baseline behavior. If no profile is
   available, fall back to `./compat.sh probe <name>` against mock.
3. `./compat.sh test <name>` — passes against mock.
4. `AWS_PROFILE=<your-profile> ./compat.sh validate <name>` — passes
   against prod.

Step 4 is mandatory. Mock-only confidence is false confidence. Divergences
from prod either indicate mock fidelity gaps (fix upstream at
`s3-tm-vnext-mock`) or real behavioral
compatibility issues we should capture in the spec.

## Code Quality Expectations

- `cargo fmt` clean
- `cargo clippy --all-targets` zero warnings in the compat crates (one
  pre-existing `list_parts` warning in upstream `s3-mock-server` is OK)
- All tests pass: library tests (`cargo test -p s3-compat-spec --lib`),
  regression-guard test, spec tests against mock, spec tests against prod
- Do NOT silence warnings with `#[allow(...)]`. Fix the root cause
- Do NOT delete tests to pass

## Hard Constraints

- **Mock server lives in a different repo** (`s3-tm-vnext-mock`). Path
  dependency. Mock changes go there, not here.
- **Test tokio runtime is shared**, via `crate::runtime().block_on(...)`.
  Never use `#[tokio::test]` in generated spec tests — per-test runtimes
  drop and kill the mock server's accept loop. The regression guard at
  `s3-compat-spec/tests/shared_runtime_invariant.rs` catches this.
- **Placeholders in specs**: `{bucket}` is auto-registered. Additional
  buckets go in `[[setup.buckets]]`.

## Where to Learn More

| Question | Path |
|---|---|
| Why this framework exists | `.kiro/steering/product.md` |
| Tech stack + dependencies | `.kiro/steering/tech.md` |
| Directory layout | `.kiro/steering/structure.md` |
| How to write a spec | `.kiro/steering/spec-authoring.md` (auto-loads on `.toml`) |
| Design + research | See `../docs/` for CLI design and compat docs |
