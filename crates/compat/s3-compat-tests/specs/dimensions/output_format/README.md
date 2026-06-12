# output_format

How `aws s3` commands format their stdout/stderr output: the bespoke text format for ls (column widths, PRE prefix, timestamp/size/key layout), the transfer result line template (`{type}: {src} to {dest}`), progress display (`Completed X/Y (rate) with N file(s) remaining` using `\r` overwrite), the presign URL output, the dryrun prefix, and the fact that high-level s3 commands silently ignore `--output json/text/table/yaml`. This dimension does NOT cover output suppression flags (see `output_suppression`) or error message format (see `error_format`).

## Coverage

21 scenarios — 4 spec, 10 covered, 2 unspeccable, 5 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / upload ignores --output flag | cp_ignores_output_flag.toml | spec |
| cp / upload failure line format | error_format/cp_upload_failure_format.toml | covered |
| cp / upload success line format | commands/cp/upload_single.toml | covered |
| cp / download success line format | commands/cp/download_single.toml | covered |
| cp / upload progress format (bytes) | commands/cp/upload_single.toml | covered |
| cp / upload with non-ASCII paths | unicode_in_transfer_output.toml | spec |
| ls / ignores --output flag | ls_ignores_output_flag.toml | spec |
| ls / object line format (date size key) | commands/ls/basic.toml | covered |
| ls / PRE prefix format | commands/ls/prefix.toml | covered |
| ls / recursive progress | — | n/a: ls is a listing command and emits no transfer progress |
| mv / success line format (move:) | commands/mv/upload_single.toml | covered |
| mv / dryrun format ((dryrun) move:) | commands/mv/upload_single_dryrun.toml | covered |
| presign / URL output format | presign_url_format.toml | spec |
| rm / success line format (delete:) | commands/rm/single.toml | covered |
| rm / recursive progress (file count without bytes) | — | unspeccable: progress is frequency-throttled (results.py `_should_print_progress_now`); the harness cannot induce a delete batch slow/large enough to cross the interval (no pacing / large fixtures), so the FILE_PROGRESS_FORMAT variant never renders |
| sync / success line format | commands/sync/local_to_s3.toml | covered |
| sync / no summary line at completion | — | n/a: v2 s3 sync emits no completion summary; the per-file + progress lines are the complete output (covered by the sync specs) |
| sync / progress with 'calculating totals' prefix | — | unspeccable: the `_STILL_CALCULATING_TOTALS` prefix only appears while enumeration outruns transfers; frequency-throttled progress + no large fixtures means the harness cannot induce that mid-enumeration state |
| s3api / --output json/text/table/yaml modes | — | n/a: not aws s3 (s3api is a different command surface) |
| --version output format | — | n/a: not aws s3 (global CLI behavior) |
| pager behavior / AWS_PAGER | — | n/a: not aws s3 (pager applies to s3api/service commands; s3 transfer output streams and never invokes pager) |

## Notes

- Two scenarios are unspeccable because the relevant progress variants only render mid-operation: progress is frequency-throttled (`results.py` `_should_print_progress_now`), so small/fast test fixtures yield only the final line. Inducing an intermediate render needs an operation slow/large enough that the harness cannot control (no transfer pacing, no large fixtures). The progress template itself is covered (`commands/cp/upload_single.toml`).
- Corpus correction: the research doc states `s3 ls` timestamps are "UTC" per PRs #2784/#3097, but the v2 source (`subcommands.py:926`) calls `astimezone(tzlocal())` — timestamps are LOCAL time. This matches the existing timezone dimension spec. The research doc's UTC claim is from v1-era documentation PRs that are no longer accurate for v2.
