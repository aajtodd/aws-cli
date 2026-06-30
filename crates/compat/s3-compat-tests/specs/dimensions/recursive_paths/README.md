# recursive_paths

Angles swept: issues

How `cp`/`mv`/`sync --recursive` derive the key/path structure of a recursive transfer — which local file maps to which S3 key (and vice-versa), directory-structure preservation, the dest-key = `{prefix}{relative_path}` mapping, trailing-slash handling on source and dest, and the walk (which files are included: all regular files recursively including hidden/dotfiles, in S3-key-compatible sorted order, following symlinks by default per Python os.walk defaults).

## Coverage

30 scenarios — 10 spec, 11 covered, 1 unspeccable, 8 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / recursive upload nested dirs with dotfiles | cp_upload_nested_dirs_dotfiles.toml | spec |
| cp / recursive upload sorted key order | cp_upload_sorted_key_order.toml | spec |
| cp / recursive upload single file in dir | cp_upload_single_file_in_dir.toml | spec |
| cp / recursive upload to bucket root (no prefix) | cp_upload_to_bucket_root.toml | spec |
| cp / recursive download nested dir structure | cp_download_nested_dir_structure.toml | spec |
| cp / recursive symlink followed by default | cp_upload_symlink_followed.toml | spec |
| cp / recursive broken symlink skip-with-warning | cp_upload_broken_symlink_skipped.toml | spec |
| cp / recursive circular symlink skip-with-warning | cp_upload_circular_symlink_skipped.toml | spec |
| cp / recursive unreadable file skipped | cp_upload_unreadable_file_skipped.toml | spec |
| cp / recursive upload flat dir | commands/cp/upload_recursive.toml | covered |
| cp / recursive download with subdir | commands/cp/download_recursive.toml | covered |
| cp / recursive no source dir name preservation | dimensions/path_normalization/cp_recursive_no_source_dir_name.toml | covered |
| cp / recursive trailing-slash append on dest prefix | dimensions/path_normalization/cp_recursive_trailing_slash_append.toml | covered |
| cp / recursive dir-marker key skipped in download | dimensions/path_normalization/cp_recursive_dir_marker_skipped.toml | covered |
| cp / s3→s3 recursive key structure (dryrun) | commands/cp/s3_to_s3_recursive_dryrun.toml | covered |
| cp / recursive nonexistent prefix exits 0 | dimensions/exit_codes/cp_recursive_nonexistent_prefix.toml | covered |
| mv / recursive upload key structure (dryrun) | commands/mv/upload_recursive_dryrun.toml | covered |
| mv / recursive download key structure (dryrun) | commands/mv/download_recursive_dryrun.toml | covered |
| sync / upload nested key structure | sync_upload_nested_key_structure.toml | spec |
| sync / local→s3 flat key structure | commands/sync/local_to_s3.toml | covered |
| sync / s3→local flat path structure | commands/sync/s3_to_local.toml | covered |
| cp / recursive non-regular file (socket/pipe) skipped | — | unspeccable: setup.files cannot create socket/pipe/device special files |
| cp / recursive undecodable filename exit 2 | — | n/a: requires non-UTF-8 filename bytes on macOS APFS (not reproducible) |
| cp / recursive case-conflict detection on download | — | n/a: requires case-insensitive filesystem behavior control |
| sync / --recursive flag accepted as no-op | — | n/a: flag acceptance is arg-parsing, not key-derivation |
| cp / recursive Windows path incompatibilities | — | n/a: Windows-specific, not testable on macOS |
| cp / recursive cross-region mv source-delete failure | — | n/a: IAM/cross-region credential behavior, not key derivation |
| cp / recursive credential refresh race | — | n/a: credential timing race, not key derivation |
| cp / recursive hanging at completion | — | n/a: concurrency/shutdown race, not key derivation |
| sync / s3→local recursive --delete empty dirs retained | — | n/a: sync_strategy dimension (what sync deletes vs retains) |

## Notes

- The walk logic (filegenerator.py:202) uses `os.listdir` which includes hidden/dotfiles — no exclusion of names starting with `.`. The `normalize_sort` (line 275) sorts entries with `os.sep` → `/` replacement to match S3 ListObjects key ordering.
- The key derivation (utils.py:294, `find_dest_path_comp_key`) computes `rel_path = src_path[len(src['path']):]` for dir_op=True, stripping the source root entirely; `dest_path = dest['path'] + rel_path` when `use_src_name=True`.
- Symlink, broken-symlink, circular-symlink, and unreadable-file scenarios are gated to `platform = ["macos", "linux"]` because symlink creation and chmod 000 are unix-only. The chmod 000 spec assumes a non-root runner (root bypasses permission checks).
- Only the socket/pipe/device special-file scenario remains `unspeccable` — `[[setup.files]]` has no field to create these (HG-007).
- The `covered` scenarios span multiple spec directories; the key-derivation contract is exercised through flat (upload_recursive, local_to_s3), nested (download_recursive, s3_to_s3_recursive_dryrun, mv upload/download dryrun), and path_normalization (no source dir name, trailing-slash append, dir-marker skip) specs.
- Corpus issues #2069/#6160 (source dir name stripping) and #4424 (trailing-slash append) are requests-change refs — both request different behavior from what v2 does. The baseline is locked by path_normalization specs.

**Mock/harness fidelity gaps:**

- **HG-007 (special-file creation in setup):** `[[setup.files]]` cannot create sockets, pipes, or device special files. This forces 1 scenario (non-regular-file skip) to `unspeccable`. See `harness-gaps.md` HG-007.
