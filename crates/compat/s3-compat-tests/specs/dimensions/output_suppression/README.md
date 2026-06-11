# output_suppression

How `aws s3` transfer commands suppress, reduce, or control verbosity of stdout/stderr output via `--quiet`, `--only-show-errors`, and `--no-progress` flags, and how streaming mode implicitly engages suppression. The suppression hierarchy is: `--quiet` (no result printer installed, all output suppressed including errors) > `--only-show-errors` (only errors on stderr) > `--no-progress` (transfer lines printed, progress bar suppressed) > default (full progress + transfer lines). These flags are scoped to transfer commands (cp, mv, sync) and rm only — not ls, mb, or rb.

## Coverage

36 scenarios — 20 spec, 1 covered, 2 unspeccable, 13 n/a
(6 spec are prod-only)

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / upload --only-show-errors success | cp_upload_only_show_errors.toml | spec |
| cp / upload --quiet success | cp_upload_quiet.toml | spec |
| cp / upload --only-show-errors error | cp_upload_only_show_errors_error.toml | spec, prod-only |
| cp / upload --quiet error | cp_upload_quiet_error.toml | spec, prod-only |
| cp / upload --no-progress | cp_upload_no_progress.toml | spec |
| cp / streaming implicit suppression | cp_streaming_implicit_suppression.toml | spec |
| ls / rejects --only-show-errors | ls_rejects_only_show_errors.toml | spec |
| mv / upload --only-show-errors success | mv_upload_only_show_errors.toml | spec |
| mv / upload --quiet success | mv_upload_quiet.toml | spec |
| mv / upload --only-show-errors error | mv_upload_only_show_errors_error.toml | spec, prod-only |
| mv / upload --quiet error | mv_upload_quiet_error.toml | spec, prod-only |
| mv / upload --no-progress | mv_upload_no_progress.toml | spec |
| rm / single --quiet | rm_single_quiet.toml | spec |
| rm / single --only-show-errors | rm_single_only_show_errors.toml | spec |
| rm / recursive --only-show-errors | rm_recursive_only_show_errors.toml | spec |
| sync / --only-show-errors success | sync_only_show_errors.toml | spec |
| sync / --quiet success | sync_quiet.toml | spec |
| sync / --only-show-errors error | sync_only_show_errors_error.toml | spec, prod-only |
| sync / --quiet error | sync_quiet_error.toml | spec, prod-only |
| sync / --no-progress | sync_no_progress.toml | spec |
| cp / --only-show-errors stray newline (#2231) | cp_upload_only_show_errors.toml | covered |
| cp / carriage-return progress in non-TTY (#9526) | — | unspeccable: progress carries a non-deterministic rate (needs regex) and the engine normalizes \r — the \r-vs-\n rendering is not assertable; not tty-gated, a pty would not help |
| sync / carriage-return progress in logs (#4190) | — | unspeccable: same as #9526 — \r-vs-\n progress rendering not assertable (rate variance + \r-normalization); not tty-gated |
| sync / no progress in some environments (#2575) | — | n/a: v1 behavior; v2 emits progress unconditionally (aws s3 has no isatty gate) |
| cp / permission-denied silent exit | — | n/a: error_format/exit_codes dimension |
| cp / single-file --recursive silent | — | n/a: recursive_paths dimension; v2 actually emits warning (exit 2) |
| cp / --exclude --include silent no-op | — | n/a: filters dimension |
| sync / --exclude --include silent no-op | — | n/a: filters dimension |
| sync / silently skips files | — | n/a: sync_strategy dimension |
| --output off suppresses stdout | — | n/a: not aws s3 (v2 global output mechanism) |
| s3api empty response suppression | — | n/a: s3api, not aws s3 |
| v2 pager auto-disable / file output | — | n/a: not aws s3 (pager applies to service commands) |
| --no-paginate ignored by cp --recursive | — | n/a: no-op in Python too, not suppression behavior |
| ls --summarize summary-only option | — | n/a: feature request (not implemented in Python) |
| --no-verify-ssl warning on non-s3 | — | n/a: non-s3 context |
| aws configure sso advisory | — | n/a: not aws s3 |

## Notes

- `aws s3` never gates output on a TTY (no `isatty` in `awscli/customizations/s3/`); progress is controlled by `--progress` / `--progress-multiline` and the carriage-return form is emitted unconditionally through the pipe. The two carriage-return scenarios (#9526, #4190) are unspeccable not for lack of a pty but because progress lines carry a non-deterministic rate (forcing regex/segment matching) and the assertion engine normalizes `\r` as a line separator — so the `\r`-vs-`\n` rendering distinction cannot be asserted. A pty would not change this.
- Six specs are prod-only because they require a nonexistent-bucket error from a real S3 endpoint (the mock CreateBucket is always valid).
- `--no-progress` is accepted on cp, mv, and sync but NOT on rm. `--quiet` and `--only-show-errors` are accepted on cp, mv, sync, and rm. None of these flags are accepted on ls, mb, or rb.
- Corpus correction: #2194 ("cp single-file --recursive produces no output, exit 0") — v2 actually produces a warning on stderr ("Skipping file... File does not exist") and exits 2.
