---
inclusion: always
foundationalType: structure
---

# Repository Structure

```
compat/                                 # repo root
├── Cargo.toml                          # workspace manifest
├── compat.sh                           # entry point for all invocations
├── README.md                           # usage guide
├── AGENTS.md                           # agent-facing quick reference
├── .kiro/
│   └── steering/                       # Kiro steering files (this directory)
├── s3-compat-spec/                     # library crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── spec.rs                     # TestSpec data model + TOML parsing
│   │   ├── runner.rs                   # run_spec pipeline: setup → execute → assert
│   │   ├── executor.rs                 # tokio::process::Command wrapper
│   │   ├── backend.rs                  # TestBackend: mock + prod
│   │   ├── harness.rs                  # TestHarness: per-run setup, credential resolution
│   │   ├── golden.rs                   # golden file capture/compare
│   │   ├── assertions/
│   │   │   ├── mod.rs                  # text assertions (exact, regex, contains, unordered)
│   │   │   ├── object.rs               # S3 object assertions
│   │   │   └── file.rs                 # local file assertions
│   │   └── error.rs                    # single Error { kind, source } type
│   └── tests/
│       └── shared_runtime_invariant.rs # regression guard for the mock-reuse bug
└── s3-compat-tests/                    # test crate
    ├── Cargo.toml
    ├── build.rs                        # generates #[test] functions from TOML specs
    ├── src/
    │   └── lib.rs                      # shared runtime, run_spec_file, TestHarness singleton
    └── specs/                          # one TOML file per spec; golden files alongside
        └── commands/
            ├── ls/
            ├── cp/
            ├── sync/
            ├── rm/
            ├── mb/
            ├── rb/
            └── ...
```

## Key Paths Outside This Directory

- `../../../s3-tm-vnext-mock` (relative) or
  [`awslabs/aws-s3-transfer-manager-rs`](https://github.com/awslabs/aws-s3-transfer-manager-rs)
  on the `s3-tm-vnext-mock` branch — s3-mock-server source.
  Path-dependency from this directory's `Cargo.toml`. Edit there for mock fidelity
  fixes or new S3 operations.
- `../docs/` — CLI design docs, compat docs, research.

## Conventions

- **One test per TOML file.** `build.rs` generates the `#[test]` functions
  from the `specs/` directory tree.
- **Golden files live next to their spec**: `foo.toml` → `foo.stdout.golden`,
  `foo.stderr.golden`. Absent file means "expect empty."
- **Placeholders**: `{bucket}` is the default bucket placeholder and is
  always registered in the placeholder map even if a spec doesn't declare
  setup. Additional buckets (multi-bucket specs) declared explicitly via
  `[[setup.buckets]]`.
- **Bucket lifecycle**: prod bucket names include a run-unique ID
  (`s3-compat-{run_id}-bucket-{spec_name}`). Stale buckets from prior
  runs are swept at harness init.
- **Credential flow**: harness resolves once via SDK chain; CLI invocations
  get static AKID/SECRET/SESSION_TOKEN env vars. The CLI never sees
  `AWS_PROFILE`.
