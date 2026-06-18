Angles swept: issues

# dryrun

The `--dryrun` flag on `aws s3` cp/mv/rm/sync causes the CLI to print the operations it would perform, prefixed with `(dryrun)`, without executing any API calls that mutate state. Exit code is 0. Transfer types use `(dryrun) {type}: {src} to {dest}`; delete operations use `(dryrun) delete: {src}` (no dest). This dimension pins the output format, the no-mutation guarantee, and exit code across all directions and commands.

## Coverage

21 scenarios — 7 spec, 12 covered, 0 unspeccable, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / upload single dryrun | `commands/cp/upload_single_dryrun.toml` | covered |
| cp / upload recursive dryrun | `commands/cp/upload_recursive_dryrun.toml` | covered |
| cp / download single dryrun | `commands/cp/download_single_dryrun.toml` | covered |
| cp / download recursive dryrun | `commands/cp/download_recursive_dryrun.toml` | covered |
| cp / s3→s3 single dryrun | `commands/cp/s3_to_s3_single_dryrun.toml` | covered |
| cp / s3→s3 recursive dryrun | `commands/cp/s3_to_s3_recursive_dryrun.toml` | covered |
| mv / upload single dryrun | `commands/mv/upload_single_dryrun.toml` | covered |
| mv / upload recursive dryrun | `commands/mv/upload_recursive_dryrun.toml` | covered |
| mv / download single dryrun | `commands/mv/download_single_dryrun.toml` | covered |
| mv / download recursive dryrun | `commands/mv/download_recursive_dryrun.toml` | covered |
| mv / s3→s3 single dryrun | `commands/mv/move_s3_to_s3_single_dryrun.toml` | covered |
| mv / s3→s3 recursive dryrun | `commands/mv/move_s3_to_s3_recursive_dryrun.toml` | covered |
| rm / single dryrun | `rm_single_dryrun.toml` | spec |
| rm / recursive dryrun | `rm_recursive_dryrun.toml` | spec |
| rm / recursive dryrun + exclude filter | `rm_dryrun_exclude_filter.toml` | spec |
| sync / upload dryrun | `sync_upload_dryrun.toml` | spec |
| sync / download dryrun | `sync_download_dryrun.toml` | spec |
| sync / s3→s3 dryrun | `sync_s3_to_s3_dryrun.toml` | spec |
| sync / --delete dryrun | `sync_delete_dryrun.toml` | spec |
| dryrun / exit code semantics | — | n/a: cross-dimension (exit_codes) |
| dryrun / --generate-cli-skeleton | — | n/a: distinct s3api mechanism, not --dryrun |

## Notes

- The 12 existing `commands/` dryrun specs comprehensively cover cp and mv across all directions (local→S3, S3→local, S3→S3) and modes (single, recursive) with the no-mutation guarantee asserted via `exists = false` on destination objects/files and `exists = true` on source objects/files for mv.
- S3→S3 dryrun (cp, mv, sync) runs on the mock because dryrun emits no CopyObject — confirmed by existing specs and `sync_s3_to_s3_dryrun.toml` passing against mock.
- #4698 (v1 exit 255 on dryrun) is obsolete — v2 exits 0 on successful dryrun; exit_codes dimension owns this.
- #9935 (upload dryrun exits 0 without IAM check / download dryrun fails via HeadObject) is a permission-validation asymmetry — observable through exit_codes, not dryrun output format.
- `sync_delete_dryrun` pins the local source file's mtime (`last_modified`) older than the S3 object so the same-size upload comparison skips, isolating the `(dryrun) delete:` output. All 7 specs run mock+prod.
