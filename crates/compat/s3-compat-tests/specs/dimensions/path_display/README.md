# path_display

Angles swept: issues

How `aws s3` renders paths and keys in command output — the path/key TEXT itself (raw characters vs percent-encoded, the `s3://bucket/key` URI form, and local path form). This dimension covers the character encoding of paths in ls listings and transfer success lines; the surrounding line template (column format, dryrun prefix, error line structure) belongs to output_format/dryrun/error_format.

## Coverage

12 scenarios — 2 spec, 8 covered, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / single upload — path rendering | `../../commands/cp/upload_single.toml` | covered |
| cp / single download — path rendering | `../../commands/cp/download_single.toml` | covered |
| cp / recursive upload — special chars raw in transfer output | `cp_special_chars_in_transfer_output.toml` | spec |
| cp / recursive download — path rendering | `../../commands/cp/download_recursive.toml` | covered |
| ls / non-recursive — special chars raw in key display | `ls_special_chars_in_key.toml` | spec |
| mv / single upload — path rendering | `../../commands/mv/upload_single.toml` | covered |
| sync / upload — path rendering | `../../commands/sync/local_to_s3.toml` | covered |
| sync / download — path rendering | `../../commands/sync/s3_to_local.toml` | covered |
| cp/mv/sync / dryrun — path rendering | `../../dimensions/dryrun/` (multiple specs) | covered |
| cp/mv / s3-to-s3 dryrun — path rendering | `../../commands/cp/s3_to_s3_single_dryrun.toml` | covered |
| ls / locale-unencodable chars — placeholder rendering | — | n/a: locale-dependent behavior; harness runs in UTF-8 and cannot control locale encoding |
| sync / progress output — path rendering during transfer | — | n/a: progress output is non-deterministic (rate-variance + \r overwrite); path text within progress IS raw but the full line is not stably assertable |

## Notes

- The existing `ls_special_chars_in_key.toml` was audited and confirmed: v2 prints keys RAW (space, `&`, `+`, `=`, non-ASCII `café`, literal `%`). #1805 was a v1 bug fixed in v2.
- The new `cp_special_chars_in_transfer_output.toml` extends the same RAW-rendering proof to transfer output lines (the `upload: {local} to s3://{bucket}/{key}` template).
- Transfer output local paths use `os.path.relpath` (relative to `$PWD`). This is pinned by `commands/cp/upload_single.toml` asserting `./hello.txt`. The relative-path behavior is the baseline even when absolute paths are given (#8383, #2530 request changing this — see IC-013).
- The `output_format/unicode_in_transfer_output.toml` pins non-ASCII (UTF-8) rendering in cp transfer output specifically. That spec is properly under output_format (it tests the full line format, not just the path chars).
- `commands/cp/upload_recursive.toml` and `download_recursive.toml` pin the path-stripping behavior (source dir prefix removed from destination display) with ASCII keys.

**Mock/harness fidelity gaps:**

- **HG-001** (mock lacks CopyObject): s3-to-s3 cp/mv/sync actual transfer output (non-dryrun) cannot be tested on mock. Dryrun IS testable and covers the path rendering (same `_format_s3_path` code path). A special-char s3-to-s3 transfer spec would need `target = "prod_only"` but is deferred since the code path is already proven by the upload special-char spec and the s3-to-s3 dryrun spec.
