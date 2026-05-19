# s3-compat-spec

Library crate for the S3 CLI backwards compatibility testing framework.

Provides the spec data model, runner engine, assertion engine, CLI executor,
golden file manager, and mock server integration. Reusable infrastructure
that `s3-compat-tests` and Rust integration tests build on.

## Modules

- `spec` — TOML spec data model and parsing (`parse_spec()`)
