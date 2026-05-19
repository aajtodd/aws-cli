# s3-compat-tests

Test crate for the S3 CLI backwards compatibility testing framework.

Contains TOML spec files, golden files, and Rust integration tests.
Depends on `s3-compat-spec` for infrastructure.

## Structure

```
specs/
  commands/       # per-command specs (ls, cp, sync, mv, rm, mb, rb, presign, website)
  dimensions/     # cross-cutting specs (exit codes, output format, filters, etc.)
golden/           # captured baseline output (mirrors specs/ structure)
fixtures/         # shared fixture directories for bulk setup
tests/
  integration/    # Rust tests for complex scenarios (signals, streaming, faults)
```
