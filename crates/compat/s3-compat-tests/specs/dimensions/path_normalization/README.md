# path_normalization

Path normalization governs how the CLI parses, encodes, compares, and transforms S3 URIs, local filesystem paths, and the mapping between them. Key contracts: (1) the `s3://` scheme is case-sensitive and required for all commands except `ls`; (2) user-facing paths use literal characters while the SDK URL-encodes API requests internally; (3) S3 keys are opaque byte sequences — double slashes, plus signs, and percent characters are preserved literally; (4) trailing slashes distinguish prefix-listing from object-addressing in `ls`; (5) recursive cp appends `/` to destination prefix and copies contents without preserving source directory name; (6) sync path comparison correctly handles special characters without re-download or prefix collision; (7) Object Lambda and Outpost bucket ARNs are explicitly rejected.

## Coverage

54 scenarios — 23 spec (19 mock+prod, 4 prod-only), 7 covered, 9 unspeccable, 15 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / uppercase S3:// scheme rejected | cp_uppercase_scheme_rejected.toml | spec |
| cp / dest s3://bucket infers key from source | cp_dest_bucket_only_infers_key.toml | spec |
| cp / plus sign in key preserved literally | cp_plus_in_key_literal.toml | spec |
| cp / double slash in key preserved literally | cp_double_slash_preserved.toml | spec |
| cp / percent-encoded input treated literally | cp_percent_encoding_literal.toml | spec |
| cp / recursive trailing-slash append on prefix | cp_recursive_trailing_slash_append.toml | spec |
| cp / recursive directory contents (no source dir name) | cp_recursive_no_source_dir_name.toml | spec |
| cp / recursive zero-byte dir-marker key skipped | cp_recursive_dir_marker_skipped.toml | spec |
| cp / CopySource encoding s3-to-s3 with + in key | cp_s3_to_s3_plus_in_key.toml | spec, prod-only |
| cp / multipart copy-source URL-encodes + | — | covered: cp_s3_to_s3_plus_in_key exercises same CopySource encoding path |
| cp / copy source no double-encoding unicode | cp_s3_to_s3_plus_in_key.toml | covered |
| ls / bare bucket name accepted (no s3://) | ls_bare_bucket_name.toml | spec |
| ls / uppercase S3:// scheme rejected | ls_uppercase_scheme_rejected.toml | spec |
| ls / trailing slash prefix semantics | ls_trailing_slash_prefix_semantics.toml | spec |
| ls / Object Lambda ARN rejected | ls_object_lambda_arn_rejected.toml | spec |
| ls / Outpost bucket ARN rejected | ls_outpost_bucket_arn_rejected.toml | spec |
| ls / special chars displayed raw (path_display) | dimensions/path_display/ls_special_chars_in_key.toml | covered |
| ls / paginated non-ASCII keys (v1 bug) | — | n/a: v1 Python bug fixed in v2; SDK handles pagination |
| ls / delimiter=/ vs sync no-delimiter (IAM) | — | unspeccable: requires IAM policy condition testing |
| mb / requires s3:// prefix | mb_requires_s3_prefix.toml | spec |
| mv / self-move detection (s3→s3) | mv_s3_self_move_rejected.toml | spec, prod-only |
| rm / keys with newline characters | rm_newline_in_key.toml | spec, prod-only |
| sync / spaces in keys upload comparison | sync_spaces_in_keys_upload.toml | spec |
| sync / double-slash download path mapping | sync_double_slash_download_path.toml | spec |
| sync / double-slash overwrites (//a vs /a) | — | covered: sync_double_slash_download_path pins the // → / local mapping |
| sync / prefix collision (xyz vs xyz2) | sync_prefix_collision_no_reupload.toml | spec |
| sync / --delete sort order file vs dir prefix | sync_delete_file_dir_prefix_collision.toml | spec |
| sync / URL-decodes keys once (no double-decode) | sync_no_double_decode_percent.toml | spec |
| sync / URL-decodes across pagination boundaries | — | unspeccable: requires >1000 objects for pagination boundary exercise |
| sync / encoding-neutral path comparison | — | covered: sync_prefix_collision_no_reupload + sync_spaces_in_keys_upload exercise comparison correctness |
| sync / re-downloads files with spaces | — | covered: sync_spaces_in_keys_upload proves v2 handles spaces correctly (v1 bug fixed) |
| sync / keys with spaces and single quotes | — | covered: sync_spaces_in_keys_upload uses quote in filename |
| sync / file-as-directory conflict | — | unspeccable: concurrent download order non-deterministic (determines which file wins) |
| sync / directory-as-file conflict --delete | — | unspeccable: concurrent download order non-deterministic |
| sync / trailing-slash spurious warning | — | n/a: v1 bug fixed in v2; no spurious warning emitted |
| sync / non-AWS endpoint %2F trailing slash | — | unspeccable: requires non-AWS endpoint configuration |
| sync / macOS NFC vs NFD re-upload | — | unspeccable: requires filesystem with NFD normalization control |
| sync / case-insensitive filesystem skips files | — | unspeccable: requires case-insensitive filesystem behavior control |
| sync / s3→s3 keys with spaces | sync_s3_to_s3_spaces_in_keys.toml | spec, prod-only |
| cp / recursive case conflict detection | — | unspeccable: requires case-insensitive filesystem behavior control |
| cp / download to /dev/null path display | — | unspeccable: destination is a device node (/dev/null); the harness sets up and asserts regular-file destinations |
| s3api / list-object-versions pagination (encoding) | — | n/a: s3api command, not aws s3 |
| s3api / bucket ARN not accepted in --bucket | — | n/a: s3api command, not aws s3 |
| s3 / https:// URL format accepted as path | — | n/a: v1-era feature; v2 does not support https:// URLs as S3 paths |
| s3 / s3express bucket rejected by mb | — | n/a: covered by mb validation, separate dimension |
| sync / s://<path> treated as local path | — | n/a: no-op behavior (v2 accepts any non-s3:// as local); not a pinnable contract |
| sync / unrecognized positional exits 255 | — | n/a: generic arg parsing, not path normalization specific |
| filter / --exclude relative path resolution | — | n/a: filters dimension, not path_normalization |
| filter / --include prefix accounting | — | n/a: filters dimension, not path_normalization |
| filter / bracket escaping bug | — | n/a: filters dimension, not path_normalization |
| filter / leading slash not recognized | — | n/a: filters dimension, not path_normalization |
| Windows / relpath across drives | — | n/a: Windows-specific, not testable on macOS/Linux |
| Windows / path length limits | — | n/a: Windows-specific, not testable on macOS/Linux |
| Windows / colons in filenames | — | n/a: Windows-specific, not testable on macOS/Linux |

## Notes

- Corpus correction: reconciliation claimed `ls` bare bucket name was "reverted in v2" — v2 (2.24.27) still accepts it. Fixed in reconciliation.
- Corpus correction: reconciliation claimed #2925 "uppercase S3:// accepted by cp in v1, rejected in v2" — v2 rejects it for BOTH ls and cp uniformly, but with different error messages (cp: "Invalid argument type"; ls: "Invalid bucket name").
- Corpus correction: sync trailing-slash warning (#1082) marked unspeccable — actually v2 no longer emits the warning (v1 bug fixed). Reclassified as n/a.
- v1 bugs #6211 (re-download files with spaces), #718 (space comparison), #2588 (spaces+quotes): all FIXED in v2. sync_spaces_in_keys_upload pins the correct v2 behavior.
- v1 bug #843 (prefix collision xyz/xyz2): FIXED in v2. sync_prefix_collision_no_reupload pins the correct behavior.
- Case conflict detection (#9925/#9926) requires case-insensitive filesystem behavior control not available in the harness.
- Pagination-boundary URL decoding (#909) requires >1000 objects to trigger ListObjects pagination, impractical for the spec framework.
- File-as-directory conflicts (#1538, #3218): outcome depends on non-deterministic concurrent download ordering.

**Mock/harness fidelity gaps:**

- **HG-001 (CopyObject/UploadPartCopy):** the mock lacks CopyObject, forcing s3-to-s3 scenarios (`cp_s3_to_s3_plus_in_key`, `mv_s3_self_move_rejected`, `sync_s3_to_s3_spaces_in_keys`) to `prod_only`. Adding CopyObject to the mock would move these to mock+prod. See `harness-gaps.md` HG-001.
- **HG-002 (newline-in-key storage):** `rm_newline_in_key` is `prod_only` because the mock's key storage doesn't handle `\n`; real S3 allows it. See `harness-gaps.md` HG-002.
