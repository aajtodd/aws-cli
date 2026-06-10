---
inclusion: fileMatch
fileMatchPattern: "s3-compat-tests/specs/**/*.toml"
---

# Spec Authoring Guide

You are authoring or editing a backwards-compatibility spec. Follow this
workflow end-to-end. **Mock-only confidence is false confidence** — the
non-negotiable workflow ends with prod validation.

## Capture the Full Contract; Let Known Gaps Fail

A spec encodes the *complete* observable behavior of the baseline (Python)
CLI — every output line, exit code, and object/file property the command
produces. Write the whole contract even when the Rust CLI does not yet
match it.

Running a spec against the Rust CLI is the gap report: a spec that passes
against Python and fails against Rust pinpoints exactly what is missing or
divergent. Do NOT trim or weaken assertions to make Rust pass — that hides
the gap and defeats the suite.

Use a `[deviation]` block ONLY for a difference that is accepted and
permanent — an intentional, documented divergence we will not change (e.g.
a deliberate MIME-string variant). A not-yet-implemented or upstream-blocked
behavior is NOT a deviation: leave the spec asserting the baseline and let
it fail until the behavior lands.

## The Non-Negotiable Workflow

For every new spec, in this order:

1. **Write** the spec TOML.
2. **Probe against prod first** (when `AWS_PROFILE` is available):
   `AWS_PROFILE=<your-profile> COMPAT_TARGET=prod ./compat.sh probe <spec_name_substring>`
   Prod is the source of truth for baseline CLI behavior. Mock probe shows
   "what the framework + mock produce together" — circular if the mock
   diverges from prod. Iterate your spec's assertions against prod output.
   If no `AWS_PROFILE` is available, fall back to mock probe:
   `./compat.sh probe <spec_name_substring>`
3. **Test against mock**: `./compat.sh test <filter>` must pass.
4. **Validate against prod**: `AWS_PROFILE=<your-profile> ./compat.sh validate <filter>` must pass.
5. If step 3 fails but step 4 passes (or prod probe in step 2 diverged from
   mock probe), that's a mock fidelity gap. Triage:
   - Fix mock fidelity (edit the upstream s3-mock-server repo —
     [`awslabs/aws-s3-transfer-manager-rs`](https://github.com/awslabs/aws-s3-transfer-manager-rs)
     branch `s3-tm-vnext`).
   - Mark spec `target = "mock_only"` (rare — needs justification).
   - Adjust assertions to wildcard legitimate prod-only variance
     (e.g. timestamps, transfer rates).

Skipping step 4 silently lets mock bugs hide real CLI-baseline divergences.
Previous mock fidelity gaps (SSE default, checksum_type, ETag/checksum on
seeded objects, DeleteObject idempotency) were ALL found via prod validation.

## Spec File Structure

```toml
[test]
name = "some_behavior"           # must match filename without .toml
description = "one-line summary"
tags = ["command_name", "dimension"]

[[setup.buckets]]                # optional; {bucket} is auto-registered
name = "{bucket}"

[[setup.objects]]                # seed S3 state before CLI runs
key = "hello.txt"
content = "hello world"          # or base64:... for binary
content_type = "text/plain"      # optional

[[setup.files]]                  # seed local files in working dir
path = "src/hello.txt"
content = "hello world"

[command]
args = ["s3", "cp", "hello.txt", "s3://{bucket}/hello.txt"]

[expected]
exit_code = 0

[expected.stdout]
mode = "exact"                   # exact | regex | contains | unordered | golden
value = "upload: hello.txt to s3://{bucket}/hello.txt\n"

[[expected.objects]]             # verify S3 state after CLI
key = "hello.txt"
content = "hello world"
content_type = "text/plain"

[[expected.files]]               # verify local filesystem after CLI
path = "hello.txt"
content = "hello world"
size = 11
```

## Provenance: the `[source]` Block

Every spec should record where its locked behavior came from and why, so
the spec file alone is auditable — no external tracker. Add a `[source]`
block:

```toml
[source]
refs = ["aws/aws-cli#523", "boto/s3transfer#87"]   # repo-qualified: org/repo#NUM or full URL
cli_ref = "awscli/customizations/s3/results.py:180" # path:line in the pinned v2 baseline
python_test = "tests/unit/customizations/s3/test_results.py::ResultPrinterTest"  # optional
rationale = "#523: per-file failure line dropped the '{src} to {dest}' segment"  # optional, terse, factual
```

- **`refs`** — upstream issues/PRs that motivated the spec. Always
  repo-qualified (`org/repo#NUM` or a full URL); a bare number is
  ambiguous because behavior is drawn from several repos (`aws/aws-cli`,
  `boto/s3transfer`, `boto/botocore`, …). Issue vs PR is not distinguished
  — GitHub resolves either form. **Curate by topic, not by conclusion:**
  cite issues/PRs genuinely about THIS behavior — do NOT bulk-copy a
  dimension's inventory evidence (e.g. a sync-comparison issue does not
  belong on an `ls`-display spec). A ref that reaches the *wrong* conclusion
  is still good provenance if it documents the confusion the spec resolves
  (e.g. a PR claiming UTC on a spec that proves local). `refs` trace *why*
  the behavior was contested; `cli_ref` proves *what* the code does — keep
  both when available.
- **`cli_ref`** — the baseline source location as `path:line`, valid at the
  pinned v2 commit recorded in `crates/docs/compat.md` (so line numbers
  don't drift). Optional.
- **`python_test`** — the upstream Python CLI test that encodes the same
  behavior, if one exists. Optional.
- **`rationale`** — optional, terse. `refs`/`cli_ref`/`python_test`
  usually *are* the rationale; only add prose when it captures non-obvious
  context the links don't. State it as a fact. **Do not editorialize about
  the spec's purpose** — "locks the behavior", "so it can't regress",
  "prevents a regression" describe the entire compat suite and add nothing.
  Agents must follow this too. If the refs speak for themselves, omit it.

The whole block is optional, but specs derived from the behavioral corpus
should carry it. There is no separate status ledger or coverage tool —
the spec suite is the record, and `[source]` is how each spec stays
self-explanatory.

## Assertion Mode Decision Tree

| Situation | Use |
|---|---|
| Single deterministic line, no variance | `exact` |
| Output contains a rate, timestamp, or other non-deterministic field | `regex` with pinned invariants + wildcarded variance |
| Multi-line output where only some lines matter | `contains` (substring) |
| Set of N lines whose order is not deterministic | `unordered` |
| Long fixed output (e.g. multi-line `ls`) | `golden` (capture via `./compat.sh capture`) |

**Default to `exact`.** Only reach for `regex` when there's actual variance.
Pure-command specs (`rm`, `mb`, `rb`, `presign`) don't have progress lines
and should be `exact`. Transfer specs (`cp`, `sync`, `mv`) need `regex`
because the progress line has `Completed N Bytes/N Bytes ({rate}) ...` with
a non-deterministic rate.

## Regex Patterns for Transfer Output

The CLI progress indicator emits `\r` to overwrite a progress line with the
final transfer line. The framework splits on `\r|\n|\r\n` so each "segment"
matches as its own terminal-line.

```toml
[expected.stdout]
mode = "regex"
value = [
    # Final progress line — pin total bytes and final file count, wildcard rate
    "^Completed 11 Bytes/11 Bytes \\([^)]+\\) with 1 file\\(s\\) remaining$",
    # Transfer line per file (order non-deterministic with concurrent transfers)
    "^upload: \\./hello\\.txt to s3://\\{bucket\\}/hello\\.txt$",
]
```

`regex` semantics: **each pattern must match at least one terminal-line.**
It does NOT assert "exactly N of each" — if you need strict set equality,
use `unordered`.

## Placeholder Conventions

- `{bucket}` is auto-registered for every spec. Use it in command args and
  expected output.
- For multi-bucket specs, declare additional placeholders explicitly:
  ```toml
  [[setup.buckets]]
  name = "{source}"
  [[setup.buckets]]
  name = "{dest}"
  ```
- Output normalization replaces resolved bucket names with placeholders
  before assertions run, so specs are portable across mock/prod runs
  (which use different bucket names).

## Bucket Lifecycle Semantics

- **Buckets declared in `[[setup.buckets]]`** are pre-created before the
  CLI runs.
- **Buckets referenced only in command args** (e.g. for `mb` specs) are
  NOT pre-created. The CLI creates them. Cleanup still finds them.
- **For `mb` specs**: don't declare `[[setup.buckets]]`. The default
  `{bucket}` placeholder is auto-registered; `create_buckets` only creates
  what's in setup.

## Assertion Fields on `[[expected.objects]]`

Partial match — declared fields are asserted, absent fields are ignored.
Available fields:

- `exists: bool` (default `true`)
- `content: string` — verified as bytes (UTF-8 string in TOML)
- `size: u64`
- `content_type: string`
- `e_tag: string`
- `storage_class: string` (SDK enum name, e.g. `"STANDARD"`)
- `server_side_encryption: string` (e.g. `"AES256"`)
- `checksum_type: string` (`"FULL_OBJECT"` or `"COMPOSITE"`)
- `checksum_sha256`, `checksum_sha1`, `checksum_crc32`,
  `checksum_crc32c`, `checksum_crc64nvme` (base64 strings)
- `metadata: { key = "value", ... }` — partial match by default
- `metadata_strict: true` — fail if unexpected keys present

Integrity is verified automatically via CRC32C when `content` is declared
(or derivable from setup). No opt-in needed.

## Using Probe to Iterate

Probe is your primary authoring tool. It shows:

- `exit_code`
- stdout/stderr as numbered terminal-lines (post-split-on-`\r|\n`)
- stdout/stderr raw and normalized for byte-level inspection
- Every object in every bucket the spec touched, with full HeadObject
  state (all 15+ fields + metadata map)

Use probe output to decide what to assert. If a field (e.g. `checksum_type`)
shows up in probe but you don't assert on it, a future CLI regression on
that field won't be caught.

## Common Failure Modes

- **`expected exit code 0, got 254`**: not a behavioral failure, credentials
  are stale/missing. Re-auth and retry.
- **Regex `^...$` doesn't match**: remember terminal-lines have embedded
  `\r` artifacts removed. Test patterns against probe's numbered output,
  not the raw string.
- **Flaky `unordered` assertions**: CLI output of long progress lines can
  get padded with trailing spaces. Use `regex` with `^...$` anchors instead.
- **Spec passes mock, fails prod**: usually a mock fidelity gap. Check
  the mock-prod fidelity research in the project's planning docs, or
  triage by probing both backends and diffing the output.
