# transfer_defaults

Angles swept: issues

Default transfer tuning that s3transfer's TransferManager uses, to the extent it is observable in stored object state (upload method and part count, inferred from the ETag). The defaults originate in `awscli/customizations/s3/transferconfig.py` and are passed through to `s3transfer.manager.TransferConfig`.

## Coverage

12 scenarios — 5 spec, 1 spec (prod-only), 2 covered, 3 unspeccable, 1 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / single file below threshold → PutObject | cp_below_threshold_put_object.toml | spec |
| cp / single file at threshold → multipart | cp_at_threshold_boundary.toml | spec, prod-only |
| cp / single file above threshold → multipart | ../../commands/cp/upload_multipart.toml | covered |
| cp / 16 MiB file → multipart (2× chunksize) | cp_multipart_chunksize_part_count.toml | spec |
| cp / zero-byte file → PutObject | cp_zero_byte_put_object.toml | spec |
| cp / single file below threshold → PutObject (small) | ../../commands/cp/upload_single.toml | covered |
| mv / single file above threshold → multipart | mv_multipart_threshold.toml | spec |
| sync / file above threshold → multipart | sync_multipart_threshold.toml | spec |
| cp / s3-to-s3 large object → multipart copy | — | n/a: blocked on HG-001 (mock lacks CopyObject); behavior is prod_only for path_normalization dimension |
| max_concurrent_requests default (10) | — | unspeccable: needs a mock request log (assert in-flight requests ≤ 10) — HG-010 |
| max_bandwidth default (None) | — | unspeccable: needs throughput measurement (timing-flaky even with a request log) |
| max_queue_size default (1000) | — | unspeccable: internal scheduler queue depth, no external observable |

## Notes

- The multipart_threshold comparison is `>=` (inclusive): a file exactly at 8388608 bytes uses multipart (confirmed empirically via `s3transfer/upload.py:249`).
- The `upload_method` assertion field infers multipart from the ETag `-N` suffix pattern.
- Timing/parallelism defaults (`max_concurrent_requests`, `max_bandwidth`, `max_queue_size`, `io_chunksize`) affect request scheduling and resource usage, not stored object state — the harness has no request-log or timing assertion capability to observe them.
- s3-to-s3 multipart copy (`UploadPartCopy`) is already tracked under `path_normalization` dimension as `prod_only` (HG-001).

**Mock/harness fidelity gaps:**

- **HG-009**: Mock CompleteMultipartUpload with 1 part returns a non-multipart ETag (no `-1` suffix), while real S3 always returns `hash-N`. Forces `cp_at_threshold_boundary` to `prod_only`. Fix: mock should always append `-{part_count}` to multipart ETags.
