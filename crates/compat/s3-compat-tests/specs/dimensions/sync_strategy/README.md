Angles swept: issues

# sync_strategy

How `aws s3 sync` decides what to transfer, skip, overwrite, or delete — the comparator. Covers the default size+last-modified comparison, the directional time-comparison asymmetry (upload vs download), `--size-only`, `--exact-timestamps`, `--delete`, and the file-not-at-dest / file-not-at-src strategies.

## Coverage

15 scenarios — 11 spec, 3 covered, 0 unspeccable, 1 n/a
(10 spec mock+prod, 1 spec prod-only)

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| sync / upload / basic new files | commands/sync/local_to_s3.toml | covered |
| sync / upload / skip same-size when dest newer | sync_skip_same_size_upload.toml | spec |
| sync / upload / new file always uploads | sync_new_file_always_uploads.toml | spec |
| sync / upload / --size-only same size skip | sync_size_only_same_size_skip.toml | spec |
| sync / upload / --size-only different size transfer | sync_size_only_different_size_transfer.toml | spec |
| sync / upload / --exact-timestamps no-op on upload | sync_exact_timestamps_upload_noop.toml | spec |
| sync / upload / --delete removes dest extras | commands/sync/local_to_s3_delete.toml | covered |
| sync / upload / --delete excludes protected | sync_delete_excludes_protected.toml | spec |
| sync / upload / no --delete preserves extras | sync_no_delete_preserves_extras.toml | spec |
| sync / download / basic new objects | commands/sync/s3_to_local.toml | covered |
| sync / download / skip when S3 newer (same size) | sync_download_skip_s3_newer.toml | spec |
| sync / download / --exact-timestamps forces re-download | sync_exact_timestamps_download_redownloads.toml | spec |
| sync / download / --delete removes local extras | sync_delete_download_removes_local.toml | spec |
| sync / s3-to-s3 / new file copies | sync_s3_to_s3_new_file_copies.toml | spec, prod-only |
| sync / --no-overwrite | — | n/a: not in baseline v2.24.27 |

## Notes

- The default comparison uses size + last-modified time only — not content hashes. The time comparison is directional: for upload/copy, source newer → transfer; for download, local newer → skip (S3 newer → skip under default; --exact-timestamps changes this to require exact equality).
- `--exact-timestamps` only changes download behavior (strict equality instead of newer-wins); upload direction falls through to the default comparison unchanged.
- `--size-only` bypasses all timestamp logic; only size mismatch triggers transfer.
- Timestamp-comparison specs pin the local file mtime via `last_modified` in `[[setup.files]]` to a fixed past time, so the size+time comparison is deterministic on both mock and prod (local is unambiguously older than the object's LastModified).
- `--delete` uses the `DeleteSync` strategy for `file_not_at_src`; without it, `NeverSync` prevents any destination-only deletions.
- `--no-overwrite` exists in the AWS CLI v2 source tree but is NOT available in the installed baseline binary (v2.24.27). Scenario deferred to a future pass when the baseline is updated.

**Mock/harness fidelity gaps:**

- **HG-001** (mock lacks CopyObject): s3-to-s3 sync scenarios are `prod-only`. The mock cannot execute CopyObject, so s3-to-s3 sync comparison outcomes cannot be observed on the mock.
