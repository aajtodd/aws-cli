# metadata_tags

Angles swept: issues

Object metadata, tagging, system HTTP headers, and storage parameters that the CLI forwards to PutObject (upload) and CopyObject (s3→s3) via `RequestParamsMapper._set_general_object_params`, `_set_metadata_params`, `_set_metadata_directive_param`, `_set_sse_request_params`, and `_set_grant_params`. Covers `--metadata`, `--metadata-directive`, `--storage-class`, `--cache-control`, `--content-encoding`, `--content-disposition`, `--content-language`, `--expires`, `--acl`, `--grants`, `--sse`, `--sse-kms-key-id`, `--website-redirect`, `--tagging`, `--tagging-directive`, and `--request-payer`.

## Coverage

28 scenarios — 13 spec (9 mock+prod, 4 prod-only), 0 covered, 7 unspeccable, 8 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / upload / user metadata | cp_upload_user_metadata.toml | spec |
| cp / recursive / user metadata | cp_recursive_upload_metadata.toml | spec |
| cp / upload / storage class | cp_upload_storage_class.toml | spec |
| cp / upload / cache-control | cp_upload_cache_control.toml | spec |
| cp / upload / content-encoding | cp_upload_content_encoding.toml | spec |
| cp / upload / content-disposition | cp_upload_content_disposition.toml | spec |
| cp / upload / content-language | cp_upload_content_language.toml | spec |
| cp / upload / expires | — | unspeccable: HG-008 (no ExpectedObject assertion field for this header/ACL) |
| cp / upload / website-redirect | — | unspeccable: HG-008 (no ExpectedObject assertion field for this header/ACL) |
| cp / upload / acl (private) | — | unspeccable: HG-008 (no ExpectedObject assertion field for this header/ACL) |
| cp / upload / sse aws:kms | cp_upload_sse_kms.toml | spec, prod-only |
| cp / s3→s3 / metadata-directive COPY | cp_s3_to_s3_metadata_directive_copy.toml | spec, prod-only |
| cp / s3→s3 / metadata-directive REPLACE | cp_s3_to_s3_metadata_directive_replace.toml | spec, prod-only |
| cp / s3→s3 / auto metadata-directive (--metadata triggers REPLACE) | cp_s3_to_s3_auto_metadata_directive.toml | spec, prod-only |
| sync / upload / user metadata (global) | sync_upload_metadata.toml | spec |
| sync / upload / storage class | sync_upload_storage_class.toml | spec |
| cp / upload / grants (non-owner) | — | unspeccable: bucket ObjectOwnership configuration |
| cp / upload / tagging | — | unspeccable: GetObjectTagging assertion |
| cp / upload / sse-c (customer key) | — | unspeccable: SSE-C requires key material management |
| mv / upload / metadata | — | unspeccable: same as cp upload; mv = cp + delete |
| cp / s3→s3 / tagging-directive | — | n/a: requires --copy-props + GetObjectTagging (HG-008) |
| cp / upload / request-payer | — | n/a: requires requester-pays bucket |
| sync / s3→s3 / metadata override limitations | — | n/a: s3→s3 sync not in scope this pass (HG-001) |
| sync / s3→s3 / copy-props default | — | n/a: blocked on CopyObject (HG-001) |
| sync / s3→s3 / copy-props none | — | n/a: blocked on CopyObject (HG-001) |
| sync / s3→s3 / copy-props metadata-directive | — | n/a: blocked on CopyObject (HG-001) |
| sync / s3→s3 / acl application | — | n/a: blocked on CopyObject (HG-001) + ObjectOwnership |
| sync / s3→s3 / metadata preservation | — | n/a: blocked on CopyObject (HG-001) |

## Notes

- `--content-type` is owned by the `content_type` dimension and not re-specced here.
- `--metadata` uses JSON map syntax `{"key":"value"}` as passed to the CLI; the harness asserts the stored metadata keys via `ExpectedObject.metadata`.
- The mock stores user metadata, storage_class, and the cache-control/content-encoding/content-disposition/content-language headers, and returns them on HeadObject — all assertable → mock+prod. (HG-008 Tier 1 added the four header assertion fields to `ExpectedObject`.) Still unspeccable: `--expires` and `--website-redirect` (the mock does not store these two; needs a field + mock support — HG-008 Tier 2) and `--acl`/`--tagging` (need GetObjectAcl/GetObjectTagging in harness + mock — HG-008 Tier 3). `--sse` asserts `server_side_encryption` and is a prod_only spec (mock does not apply SSE).
- The mock ignores `--sse` value (always returns AES256 regardless of the requested algorithm).
- s3→s3 copy specs are prod_only due to mock lacking CopyObject (HG-001).
- `--acl private` and `--acl bucket-owner-full-control` succeed on modern BucketOwnerEnforced buckets; other canned ACLs and `--grants` to non-owner fail with `AccessControlListNotSupported`.
- Corpus entries for `s3api`-specific behavior (#244, #5329, #1254, #6712, #1124) are out of scope (`aws s3` dimension only).
- Corpus entries for mtime/timestamp behavior (#6601, #3069, #2000, #2208, #2209) map to the `sync_strategy` or `timezone` dimension and are not re-specced here.

**Mock/harness fidelity gaps:**

- **HG-008** — Harness `ExpectedObject` lacks assertion fields for CacheControl, ContentEncoding, ContentDisposition, ContentLanguage, Expires, WebsiteRedirectLocation, and lacks GetObjectTagging/GetObjectAcl API calls. Forces 8 scenarios to prod_only where the stored value cannot be programmatically asserted even though real S3 stores them correctly.
- **HG-001** — Mock lacks CopyObject. Forces all s3→s3 metadata-directive and copy-props scenarios to prod_only.
