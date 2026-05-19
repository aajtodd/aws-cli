---
inclusion: always
foundationalType: product
---

# S3 CLI Backwards Compatibility Testing Framework

## What This Is

A Rust testing framework for verifying that any new S3 CLI implementation is
backwards-compatible with the reference CLI (`aws s3`). Produces a test suite
where the pass/fail count IS the compatibility report — no separate report
generator, no custom dashboard.

## Core Model

**Baseline** is the reference CLI whose behavior we preserve. Today that's
`aws s3`; the framework never hardcodes this. Specs describe observable CLI
behavior (stdout, stderr, exit code, resulting S3 object state, resulting
local file state).

**Deviation** is an intentional difference from baseline. Specs declare
deviations explicitly with rationale — they're not failures, they're part
of the report. This lets us ship a CLI that *improves* on baseline (SIGPIPE
handling, progress-to-stderr) without breaking the compat story.

## Three Assertion Dimensions

1. **CLI output** — stdout, stderr, exit code. Asserted via `exact`, `regex`,
   `contains`, `unordered`, or `golden` modes.
2. **S3 state** — `[[expected.objects]]` checks content, metadata, checksums,
   etc. via HeadObject + GetObject.
3. **Local filesystem state** — `[[expected.files]]` checks content, size,
   existence for download/sync specs.

Integrity is verified automatically (CRC32C) for every transfer when content
is known from setup. Specs don't need to opt in.

## Two Backends, Same Specs

- **mock** (`COMPAT_TARGET=mock`, default): fast, deterministic, CI-safe.
  Uses an in-process S3 mock server.
- **prod** (`COMPAT_TARGET=prod`, opt-in): real AWS S3. Credential-required.
  Used for mock-fidelity validation — run the same specs against both backends
  and flag divergence.

A spec passing on mock is provisional; a spec passing on prod is validated.
The `compat.sh validate` workflow is the mock-fidelity gate.

## Non-Negotiable Workflow

Every new spec goes through probe → test → validate against prod before it's
considered done. Mock-only confidence is false confidence.

## Scope

- IN scope: `aws s3` subcommands (ls, cp, mv, rm, sync, mb, rb, presign, website).
- OUT of scope: `aws s3api` (model-driven, separate concern), new commands
  without a compat baseline (cat, pipe, du, find).
