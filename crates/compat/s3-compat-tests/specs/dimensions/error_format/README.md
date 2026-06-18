Angles swept: issues

# error_format

How `aws s3` renders error messages to stderr — the message template, exit
code, and leading-newline presence. Four distinct formats exist in the v2
baseline:

1. **ERROR_FORMAT** `fatal error: {exception}` (results.py:339) — exit 1, no leading `\n`.
2. **FAILURE_FORMAT** `{type} failed: {src} to {dest} {exception}` (results.py:335) — exit 1, no leading `\n`.
3. **PATH_VALIDATION** `\nThe user-provided path {path} does not exist.` (subcommands.py:1829) — exit 255, HAS leading `\n`.
4. **CLIENT_ERROR** `\n{exception}` (errorhandler.py ClientErrorHandler) — exit 254, HAS leading `\n`.

Additionally, `mb` and `rb` have inline exception handlers producing FAILURE_FORMAT-like
messages (`make_bucket failed:` / `remove_bucket failed:`) via subcommands.py.

## Coverage

14 scenarios — 6 spec, 5 covered, 1 unspeccable, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / single upload to nonexistent bucket | cp_upload_failure_format.toml | spec, prod-only |
| cp / single download nonexistent key | download_nonexistent_key.toml | spec |
| cp / local source missing | local_source_missing.toml | spec |
| cp / recursive upload failure (per-file) | — | covered (cp_upload_failure_format.toml — same FAILURE_FORMAT code path, repeated per file) |
| ls / nonexistent bucket (ClientError) | — | covered (commands/ls/nonexistent.toml — pins `\n{exception}`, exit 254, golden) |
| mb / create failure (BucketAlreadyOwnedByYou) | mb_create_failure_format.toml | spec, prod-only |
| mv / local source missing | — | covered (local_source_missing.toml — same subcommands.py path validation, mv uses same CpCommand base) |
| rb / nonexistent bucket | — | covered (dimensions/exit_codes/rb_nonexistent_bucket.toml — pins `remove_bucket failed:` format via regex) |
| rb / not empty (BucketNotEmpty) | rb_not_empty.toml | spec, prod-only |
| rm / nonexistent key (idempotent, no error) | — | n/a: no error emitted (exit 0, success output) — pinned by commands/rm/nonexistent.toml |
| sync / nonexistent destination bucket | — | covered (dimensions/exit_codes/sync_nonexistent_bucket.toml — pins exact `fatal error:` text) |
| sync --delete / nonexistent source | sync_delete_nonexistent_source.toml | spec |
| sync / per-file transfer failure (mid-transfer) | — | unspeccable: deterministic server fault injection — mock lacks fault-injection API (dead code) and real S3 will not fault on demand |
| access-denied / invalid-credential rendering | — | n/a: same format templates with different error codes — format already pinned by existing specs; the error CODE is an exit_codes/credential_resolution concern |

## Notes

- **Leading-newline asymmetry:** PATH_VALIDATION and CLIENT_ERROR paths emit a leading `\n`
  before the message; ERROR_FORMAT and FAILURE_FORMAT do not. The leading `\n` originates
  from `write_error()` in errorhandler.py (for ClientError) and from
  `RuntimeError` propagation in subcommands.py (for path validation). The transfer result
  printer (results.py) does not prepend `\n`.

- **Mock/harness fidelity gaps:**
  - **HG-005** — Mock CreateBucket is idempotent (always succeeds) and DeleteBucket on
    non-empty bucket returns 500/InternalError instead of 409/BucketNotEmpty. Forces
    `mb_create_failure_format` and `rb_not_empty` to `prod_only`. Fix: add
    BucketAlreadyOwnedByYou and BucketNotEmpty error conditions to the mock's handlers.
  - Mock PutObject does NOT enforce bucket existence — upload-to-nonexistent-bucket error
    scenarios (`cp_upload_failure_format`) are `prod_only`. This is the same gap noted in
    spec-authoring.md (Bucket Lifecycle Semantics).
  - **Mid-transfer server fault** (500 during multipart part): the mock's fault-injection
    API is dead code and real S3 will not fault on demand. Marks per-file sync/recursive
    transfer failure as `unspeccable` for deterministic testing.

- **Corpus corrections:** none. The 4 existing specs and their error messages match
  observed v2.24.27 behavior.
