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

A verified behavior becomes a spec. If you run the CLI to confirm how
something behaves, the artifact is a committed spec that locks it — never a
throwaway probe you check once and discard. The spec suite is the record;
a manual point-in-time check that isn't captured is lost. And cover the
behavior *space*, not one input: a spec should exercise the representative
cases that can change the outcome (e.g. each class of special character, not
just one), so the suite answers the behavior exhaustively rather than
anecdotally.

## Where a spec lives: dimension vs command

Every spec runs *some* command, so "it uses ls" never decides placement. Ask:
**if this behavior were wrong, would it be wrong in more than one command?**

- Yes → it is a cross-cutting aspect: `dimensions/<dim>/`, named by the aspect
  (timezone, error_format, exit_codes, path_display, content_type, …), tagged
  with the command that exercises it.
- No — it is specific to one command's own function (mb creating a bucket, ls's
  `PRE` marker, source/dest routing) → `commands/<cmd>/`.

On the line (an error *during* a command — exit code + message): file under the
dimension whose contract you are pinning; use `commands/<cmd>/` only when the
point is the command's core operation. When unsure, it is a dimension — that is
the coverage measured toward 100%. Always tag the command so specs stay
findable per-command across both trees.

## A dimension is covered per command, not once

The same dimension can behave differently across commands or modes, and each
variant that can differ is its own contract and its own spec. A dimension spec
exercised through one command does **not** cover that dimension for the others —
never mark a dimension done from a single command. Enumerate the commands and
modes where the behavior appears, and cover each scenario. The coverage unit is
**(dimension × command/mode)**, not the dimension alone.

Example — `error_format` is not one contract:
- single-operation failure (`cp`/`mv`/`ls`, one object): `fatal error: {exception}`, exit 1
- recursive/sync per-file failure: `{type} failed: {src} to {dest} {exception}`
- pre-transfer local-path validation: `The user-provided path {p} does not exist.`, exit 255

These render and exit differently; each is a separate spec under `error_format`.

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
  both when available. **Make the ref's relationship to the baseline legible
  in `rationale`** when it isn't obvious: a ref may *confirm/define* the
  baseline (a doc, a by-design decision, or a bug resolved as the current
  behavior), *request CHANGING* it (an open or declined feature request — the
  cited behavior is the contested current baseline, NOT what the issue wants:
  write "#NNNN requests X; v2 still does Y, so this spec locks the Y baseline"
  and never bend the spec toward the request), or be *a bug fixed into* the
  baseline (the spec pins the post-fix behavior). State the relationship as a
  durable fact; never record open/closed status (point-in-time, banned).
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
  **Carry no point-in-time or test state anywhere in the spec** (description
  or rationale) — no "verified", no "Rust matches / diverges", no pass/fail
  snapshot. A spec describes the baseline behavior and its provenance;
  whether an implementation conforms is what running the suite reports, and
  it changes over time.

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

**Value format by mode:**
- `exact` — a single string (`value = "...\n"`), compared verbatim after
  bucket/timestamp normalization.
- `regex` / `contains` / `unordered` — an ARRAY of strings (`value = ["..."]`):
  `regex` each pattern must match ≥1 terminal-line; `contains` each substring
  must appear in the stream; `unordered` the set of lines must match in any
  order — entries are full lines with NO trailing `\n`.
- `golden` — no inline `value`; the sibling `.{stdout,stderr}.golden` file holds it.

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

## Streaming Input (`[command].stdin`)

For streaming specs that read stdin (`cp - s3://…`), pipe input bytes with the
optional `stdin` field on `[command]`:
```toml
[command]
args = ["s3", "cp", "-", "s3://{bucket}/k"]
stdin = "bytes to pipe to the CLI"
```

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
- **Errors requiring a nonexistent bucket:** the mock's PutObject/CreateBucket
  do NOT enforce bucket existence (uploading to a missing bucket SUCCEEDS on the
  mock), so cp/mv upload-to-nonexistent-bucket *error* scenarios are `prod_only`.
  But ListObjectsV2 on a nonexistent bucket DOES return NoSuchBucket on the mock,
  so `ls`/`sync` failing on a nonexistent bucket are mock+prod. Use a hardcoded,
  clearly-nonexistent bucket name (not `{bucket}`) for these.

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
