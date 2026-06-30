# data_integrity

Angles swept: issues

Checksum behavior the CLI applies to uploads: which algorithm is computed by default, which algorithm is stored when `--checksum-algorithm` is specified, and the checksum type (FULL_OBJECT vs COMPOSITE) for multipart uploads. Covers the `request_checksum_calculation = when_supported` default that causes a CRC64NVME checksum to be computed and sent on every upload even without an explicit `--checksum-algorithm` flag.

## Coverage

13 scenarios — 8 spec, 0 covered, 3 unspeccable, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / single — default checksum (CRC64NVME) | cp_upload_default_checksum.toml | spec |
| cp / single — `--checksum-algorithm CRC64NVME` | cp_upload_checksum_crc64nvme.toml | spec |
| cp / single — `--checksum-algorithm CRC32` | cp_upload_checksum_crc32.toml | spec |
| cp / single — `--checksum-algorithm CRC32C` | cp_upload_checksum_crc32c.toml | spec |
| cp / single — `--checksum-algorithm SHA1` | cp_upload_checksum_sha1.toml | spec |
| cp / single — `--checksum-algorithm SHA256` | cp_upload_checksum_sha256.toml | spec |
| cp / multipart — default checksum (CRC64NVME, FULL_OBJECT) | cp_multipart_default_checksum.toml | spec |
| cp / multipart — explicit CRC32 (COMPOSITE) | cp_multipart_checksum_crc32_composite.toml | spec, prod-only |
| download — checksum validation on mismatch/corruption | — | unspeccable: fault-injection |
| download — retry on checksum error vs connection error | — | unspeccable: fault-injection |
| upload — request checksum reuse on retry | — | unspeccable: request-log |
| s3api — put-bucket-cors/lifecycle Content-MD5 | — | n/a: s3api not in scope |
| signing — Transfer-Encoding/hop-by-hop header exclusion | — | n/a: SDK-level signing internals |

## Notes

- The default upload checksum algorithm is CRC64NVME (defined at `httpchecksum.py:42`), applied when `request_checksum_calculation = when_supported` (the v2 default). This applies to both single-part and multipart uploads.
- For multipart uploads, CRC64NVME produces `checksum_type = FULL_OBJECT` (S3 combines per-part CRC64NVME into a full-object checksum). Other algorithms (CRC32, SHA256, SHA1, CRC32C) produce `checksum_type = COMPOSITE` with a `-N` suffix on the checksum value indicating part count.
- All five supported algorithms (CRC64NVME, CRC32, CRC32C, SHA1, SHA256) produce correct checksums on single-part uploads on both mock and prod.
- Download-corruption detection and retry-on-checksum-mismatch require fault injection (the mock cannot corrupt bytes mid-transfer, and real S3 won't return bad data on demand).

**Mock/harness fidelity gaps:**

- **HG-011:** Mock HEAD fails with streaming error on multipart objects uploaded with explicit `--checksum-algorithm` (non-default). This forces `cp_multipart_checksum_crc32_composite` (and analogous SHA256/SHA1/CRC32C multipart specs when authored) to `prod_only`. The default CRC64NVME multipart works correctly on mock.
