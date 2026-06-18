Angles swept: issues

# timezone

How `aws s3` displays timestamps. The ONLY place `aws s3` prints a timestamp is the `s3 ls` LastModified column — `_make_last_mod_str` (subcommands.py:920) calls `.astimezone(tzlocal())`, hardcoded to the machine's LOCAL timezone and formatted as bare `YYYY-MM-DD HH:MM:SS` (19-char, ljust). This is NOT governed by `cli_timestamp_format` (which affects s3api and other services). cp/mv/sync/rm/mb/rb emit no timestamps.

## Coverage

8 scenarios — 2 spec, 1 covered, 0 unspeccable, 5 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| ls / LastModified converts to local tz | ls_local_conversion.toml | spec, mock-only |
| ls / cli_timestamp_format has no effect | ls_cli_timestamp_format_ignored.toml | spec, mock-only |
| ls / no timezone offset or Z suffix in output | ../commands/ls/basic.toml | covered |
| cp / no timestamp in output | — | n/a: cp emits no timestamps |
| mv / no timestamp in output | — | n/a: mv emits no timestamps |
| rm / no timestamp in output | — | n/a: rm emits no timestamps |
| sync / no timestamp in output | — | n/a: sync emits no timestamps |
| mb, rb / no timestamp in output | — | n/a: mb/rb emit no timestamps |

## Notes

- Both specs are `mock_only` because asserting a converted timestamp requires a controlled `LastModified`, which real S3 does not allow (object timestamps are server-set at upload time).
- The runner pins `TZ=UTC` by default; `ls_local_conversion` overrides `TZ=America/Phoenix` via `command.env` to prove the conversion (14:30Z → 07:30 local).
- Corpus correction: #2784/#3097 were catalogued as "ls shows UTC" in the research inventory — v2 actually shows LOCAL (confirmed by source and observation). Corrected in `timezone.reconciliation.md` (2026-06-09).
- #669 (jsonl: "compare s3's last modified time using server's timezone") is about `s3 sync` time comparison, not `ls` display — corrected from an extraction mislabel; belongs to `sync_strategy`, not this dimension.
- #5242 requests adding a timezone offset to `s3 ls` output; v2 still omits it. Tracked as IC-012.
- `cli_timestamp_format` (#2397, #3610, #9800) is a global output-formatter config for s3api/other services; `s3 ls` has its own hardcoded format and never consults it. The config option itself belongs to the `config` dimension; this dimension pins only that `s3 ls` ignores it.
