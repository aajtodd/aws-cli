# Behavioral Gaps

Differences between the Python AWS CLI (`aws s3`) and this Rust
implementation, organized by the same dimensions used in the compat
test framework (`specs/dimensions/`).

Each entry states the observable difference. Resolved items note where
the fix lives. Open items include context needed to implement.

> Based on aws-cli 2.34.34 (`6cdc68ba6`).

---

## Content-Type Detection

**Resolved.** `transfer::guess_content_type` via `mime_guess2`.
Deterministic across platforms (compiled-in database). Controlled by
`--no-guess-mime-type` and `--content-type`.

Known MIME string variants (e.g., `.woff` → `application/font-woff` vs
Python's `font/woff`) tracked as deviations in compat specs.

## Exit Codes

**Resolved.** Exit code constants in `lib.rs::exit_code` match
`awscli/constants.py`. See `exit-codes.md` for the full contract.

Unimplemented features exit 252 (ParamValidation) with a stderr message.

## Output Format

**Resolved (partial).** `ls` column formatting, transfer result lines,
error message format — all match Python. Progress output
(`Completed X/Y (rate) with N file(s) remaining`) is **open** — blocked
on TM event API.

## Error Format

**Resolved.** Service errors formatted as:
`\nAn error occurred ({code}) when calling the {operation} operation: {message}`

Leading `\n` on stderr matches Python. Implemented in `error.rs` +
`handle_s3_cmd`.

## Dryrun

**Resolved (single-file + recursive Local→S3).** Output format matches:
`(dryrun) {type}: {src} to {dest}`. Recursive uses TM walker APIs.

**Needs investigation:** Python's S3→S3 single-file dryrun appears to
call HeadObject before printing (fails on missing creds). Our impl
prints unconditionally without API calls, needs a spec to pin down.

## Recursive Paths

**Resolved (partial).** `cp --recursive` Local→S3 and S3→Local are
implemented via TM `upload_objects`/`download_objects` (object keys and
local paths match Python; exit code matches via `FailedTransferPolicy::Continue`).
Recursive download creates the destination directory (and parents) to match
Python.

**Open:**
- `mv --recursive` — not yet wired (reuses the same TM APIs).
- `cp/mv --recursive` S3→S3 — blocked on `tm.copy()`.
- `sync` — blocked on comparison logic.
- **Per-file output / progress** during recursive transfers — not emitted
  (no per-object TM event API); recursive transfers are silent.
- **Content-type inference on recursive upload** — `upload_objects` has no
  content-type hook, so recursive uploads store `application/octet-stream`
  where Python infers per file (e.g. `.txt` → `text/plain`). See
  Content-Type Detection; needs a TM per-file content-type capability.

## Path Display

**Resolved.** `paths::format_local_path` matches Python's
`os.path.relpath` behavior. 11 tests.

## Output Suppression

**Open.** `--quiet`, `--only-show-errors`, `--no-progress` — flags are
parsed but not wired to suppress output (no progress output exists yet
to suppress).

## Terminal / TTY

**Open.** No progress bar, no `\r` overwrite, no ANSI escape handling.
Blocked on TM event API + TTY detection for output mode switching.

## Streaming

**Open.** `cp - s3://bucket/key` (stdin) and `cp s3://bucket/key -`
(stdout) not implemented. `TransferUri` has no Stdio variant.

## Filters

**Open.** `--include`/`--exclude` parsed but not evaluated. Blocked on
recursive operations.

## Config

**Resolved (partial).** SDK-mapped keys wired: `addressing_style`,
`use_accelerate_endpoint`, `use_arn_region`,
`s3_disable_multiregion_access_points`. TM-mapped keys wired:
`multipart_threshold`, `multipart_chunksize`.

**Open:**
- `max_concurrent_requests` — TM has no equivalent knob
- `max_bandwidth` — TM has no rate limiter
- `payload_signing_enabled` — TM unconditionally disables; no hook
- `us_east_1_regional_endpoint` — needs custom endpoint params plugin

## Transfer Defaults

**Resolved (partial).** Multipart threshold and chunksize wired to TM.

**Open:** Multipart threshold may not be taking effect against mock
(9MB file uploaded via PutObject — needs investigation).

## Credential Resolution

**Resolved (mostly).** Provider order, env vars, profile selection, SSO,
assume-role, container, IMDS v2, web-identity, caching — all match.

**Open:**
- IMDSv1 fallback: Rust is v2-only (impact: old EC2 AMIs)
- MFA prompting: No SDK support; profiles with `mfa_serial` fail
- `AWS_SECURITY_TOKEN` (legacy): Not honored
- `AWS_CREDENTIAL_EXPIRATION`: Not honored
- STS assume-role disk cache: In-memory only

**Needs specs:** Error behavior when credentials are absent or invalid —
Python emits `Unable to locate credentials` (or `InvalidAccessKeyId`,
`ExpiredToken`, etc.) at specific points in the command lifecycle. The
exact message, exit code, and *when* the error surfaces (e.g., Python's
S3→S3 dryrun calls HeadObject and fails on missing creds; our dryrun
prints without API calls) are all observable differences worth testing.

## User Agent

**Open.** Python includes CLI version, OS, subcommand
(`command/s3.ls`), transfer client markers. We use SDK default. Not
audited or customized.

## Debug Output

**Accepted divergence.** Python uses stdlib `logging` format. We use
`tracing_subscriber::fmt`. Information categories match (request IDs,
signing, retries) but format differs.

## Signal Handling

**Resolved (partial).** SIGPIPE reset to SIG_DFL at startup (no exit
255 on broken pipe). SIGINT graceful cleanup not implemented.

## Data Integrity

**Resolved (partial).** TM handles checksums on upload. Compat framework
verifies CRC32C on every transfer where content is known. Download
integrity verification behavior not yet audited.

## Timezone

**Resolved.** `ls` displays timestamps in local timezone via `jiff`.
Matches Python's `datetime.fromtimestamp()` behavior.

## Locale / Encoding

**Not audited.** Unicode filenames, macOS NFD normalization, `LC_CTYPE`
fixup — no testing or verification done.

## Glacier / Storage Class

**Open.** No glacier-aware download blocking, no `--force-glacier-transfer`,
no exit code 2 for glacier warnings.

## Metadata / Tags

**Open.** `--copy-props`, `--metadata` not implemented. Blocked on S3→S3
copy.

## Sync Strategy

**Open.** Sync not implemented. Blocked on TM walker APIs.

## Path Normalization

**Resolved.** S3 URIs, access point ARNs (standard, MRAP, Outposts),
trailing slashes — all handled. Object Lambda and Outposts bucket ARNs
rejected with Python-exact messages. 23 tests in `uri.rs` + `arn.rs`.

---

## Cross-Cutting (Not Dimension-Specific)

### `--no-verify-ssl`

**Open.** Python disables TLS verification. We reject (exit 252).
`aws-smithy-http-client` has no public API to disable verification.

**To resolve:** Upstream exposes toggle on `TrustStore` or
`TlsContextBuilder`.

### `--ca-bundle`

**Open.** Python uses provided PEM as sole trust anchor. We reject
(exit 252). TM builds per-thread HTTP clients with no hook for
caller-supplied TLS context.

**To resolve:** TM needs `S3ClientConfig::with_tls_context(...)`;
smithy-rs needs non-panicking PEM validation.

### `--cli-read-timeout` Semantics

**Accepted divergence.** Python: per-socket-read (time between chunks).
Rust SDK: time-to-first-byte. Low practical impact for S3.

### S3→S3 Copy

**Open.** Python uses server-side `CopyObject` (< 5GB) or multipart
`UploadPartCopy` (≥ 8MB threshold). Not implemented — waiting on TM
`tm.copy()`.

### Download to `/dev/null`

**Open.** TM's temp-file-then-rename fails on device nodes. Needs
investigation — detect non-regular files and use direct-write path.

### Retry Behavior

**Verified match.** Standard mode identical (backoff, token bucket,
error classification). `x-amz-retry-after` honored in Rust
(improvement over Python). Legacy mode not supported.

### Endpoint Resolution

**Verified match.** Same Smithy endpoint ruleset. All addressing modes,
FIPS, dualstack, accelerate, ARN routing, S3 Express — identical.
Cross-region redirect resolved via interceptor.

### CLI Output Flags (No-Ops)

**Accepted divergence.** `--output`, `--query`, `--no-paginate`,
`--cli-binary-format`, `--cli-error-format` — parsed, ignored. Matches
Python (these have no effect on `aws s3`). `--cli-auto-prompt` is the
one real difference — Python prompts interactively; we don't.
