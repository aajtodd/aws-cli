# Python CLI Reference

Reference for the Python AWS CLI's S3 implementation. This documents the
behavior we need to match, extracted from `awscli/customizations/s3/`.

> Based on aws-cli 2.34.34 (`6cdc68ba6`). Re-verify after major version bumps.

## Command Surface

Nine subcommands, three complexity tiers:

**Tier 1 — SDK-only (no transfer manager):**
- `ls` — ListObjectsV2 paginator + ListBuckets
- `mb` — CreateBucket
- `rb` — DeleteBucket (+ ListObjectsV2 + DeleteObject with `--force`)
- `presign` — generate_presigned_url
- `website` — PutBucketWebsite / DeleteBucketWebsite

**Tier 2 — Transfer manager, single/recursive:**
- `cp` — upload/download/copy (single or recursive with `--recursive`)
- `mv` — same as cp + delete source on success
- `rm` — DeleteObject (single or recursive)

**Tier 3 — Transfer manager, orchestration:**
- `sync` — FileGenerator → Comparator → FileInfoBuilder → S3TransferHandler

## Shared Arguments

Transfer commands (cp, mv, sync) share these arguments, defined as module-level
dicts in `subcommands.py`:

| Arg | Type | Used By |
|-----|------|---------|
| `--recursive` | flag | cp, mv, rm, sync (implicit) |
| `--dryrun` | flag | cp, mv, rm, sync |
| `--quiet` | flag | cp, mv, rm, sync |
| `--follow-symlinks` | flag (default true) | cp, mv, sync |
| `--no-follow-symlinks` | negation flag | cp, mv, sync |
| `--no-guess-mime-type` | flag | cp, mv, sync |
| `--content-type` | string | cp, mv, sync |
| `--exclude` | append filter | cp, mv, rm, sync |
| `--include` | append filter | cp, mv, rm, sync |
| `--acl` | choices | cp, mv, sync |
| `--grants` | append | cp, mv, sync |
| `--sse` | choices (AES256, aws:kms, aws:kms:dsse) | cp, mv, sync |
| `--sse-c` | string | cp, mv, sync |
| `--sse-c-key` | string (blob) | cp, mv, sync |
| `--sse-kms-key-id` | string | cp, mv, sync |
| `--sse-c-copy-source` | string | cp, mv, sync |
| `--sse-c-copy-source-key` | string (blob) | cp, mv, sync |
| `--storage-class` | choices | cp, mv, sync |
| `--website-redirect` | string | cp, mv, sync |
| `--cache-control` | string | cp, mv, sync |
| `--content-disposition` | string | cp, mv, sync |
| `--content-encoding` | string | cp, mv, sync |
| `--content-language` | string | cp, mv, sync |
| `--expires` | string | cp, mv, sync |
| `--metadata` | map | cp, mv, sync |
| `--metadata-directive` | choices (COPY, REPLACE) | cp, mv, sync |
| `--expected-size` | string | cp, mv |
| `--page-size` | integer | ls, rm |
| `--human-readable` | flag | ls |
| `--summarize` | flag | ls |
| `--request-payer` | optional choice (requester) | ls, cp, mv, rm, sync |
| `--force` | flag | rb |
| `--ignore-glacier-warnings` | flag | cp, mv, sync |
| `--force-glacier-transfer` | flag | cp, mv, sync |
| `--only-show-errors` | flag | cp, mv, rm, sync |
| `--no-progress` | flag | cp, mv, rm, sync |
| `--validate-same-s3-paths` | flag | mv |
| `--copy-props` | choices (none, metadata-directive, default) | cp, mv, sync |
| `--checksum-mode` | choices (ENABLED) | cp, mv, sync |
| `--checksum-algorithm` | choices (CRC64NVME, CRC32, SHA256, SHA1, CRC32C) | cp, mv, sync |
| `--bucket-name-prefix` | string | ls |
| `--bucket-region` | string | ls |
| `--no-overwrite` | flag | cp, mv, sync |

## Output Formats

### Transfer Output

```
SUCCESS:   "{transfer_type}: {src} to {dest}"
           "{transfer_type}: {src}"                    (when dest is None, e.g. rm)
DRY_RUN:   "(dryrun) {transfer_type}: {src} to {dest}"
FAILURE:   "{transfer_type} failed: {src} to {dest} {exception}"  → stderr
WARNING:   "{message}"                                              → stderr
ERROR:     "fatal error: {exception}"                               → stderr
CTRL_C:    "cancelled: ctrl-c received"                             → stderr
```

Transfer type strings: `upload`, `download`, `copy`, `move`, `delete`.

### Progress Format

```
Completed {bytes}/{total} ({speed}) with {N} file(s) remaining
```

- `~` prefix on totals while calculating
- `(calculating...)` suffix while totals unknown
- TTY: `\r` ending, padded to overwrite previous content
- Non-TTY: `\n` ending

### ls Output

```
# Bucket listing:
{YYYY-MM-DD HH:MM:SS} {bucket_name}

# Object listing (non-recursive, with Delimiter="/"):
{YYYY-MM-DD HH:MM:SS} {size:>10} {basename}
                           PRE {prefix}/

# Object listing (recursive, no Delimiter):
{YYYY-MM-DD HH:MM:SS} {size:>10} {full_key}

# Summary (--summarize):
\n
   Total Objects: {count}
      Total Size: {size}
```

Date is in local time. Size is right-justified in 10 chars (or human-readable
with unit suffix). PRE is right-justified to align with the 30-char date+size
column.

### Output Suppression

- `--quiet` — no output at all
- `--only-show-errors` — no progress, no success messages. Only failures/warnings/errors.
- `--no-progress` — success/failure printed, no progress line.

## Error Format

Service errors use `botocore.exceptions.ClientError.MSG_TEMPLATE`:

```
An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}
```

The 301 PermanentRedirect error is enhanced with the correct endpoint
(`awscli/customizations/s3errormsg.py`).

## Argument Validation

Validation runs in this order (from `S3TransferCommand._run_main`):

1. `_convert_path_args()` — normalize paths
2. `check_path_type()` — validate command × path combination
3. `_normalize_s3_trailing_slash()` — append `/` to bare bucket URIs
4. `_validate_streaming_paths()` — stdin/stdout restrictions
5. `_validate_path_args()` — mv same-path, checksum direction, local existence
6. `_validate_sse_c_args()` — SSE-C key/algorithm pairing
7. `_validate_not_s3_express_bucket_for_sync()` — directory bucket restriction

### Cross-Arg Validation Rules

- `--sse-c-copy-source` only valid for S3→S3 copies
- `--checksum-algorithm` only valid for upload and S3→S3
- `--checksum-mode` only valid for download
- `--no-overwrite` rejected with streaming downloads (`-`)
- mv: rejects same source and dest path
- sync: rejects S3 Express directory buckets
- Streaming (`-`): only compatible with non-recursive cp

### Error Messages

These exact strings must be matched:

```
"usage: aws s3 {cmd} {usage}\nError: Invalid argument type"
"Cannot mv a file onto itself: {src} - {dest}"
"Streaming currently is only compatible with non-recursive cp commands"
"The user-provided path %s does not exist."
"Cannot use sync command with a directory bucket."
"--sse-c-copy-source is only supported for copy operations."
"--no-overwrite parameter is not supported for streaming downloads"
```

## Transfer Config

The Python CLI reads `~/.aws/config` `[s3]` section:

```
multipart_threshold = 8MB
multipart_chunksize = 8MB
max_concurrent_requests = 10
max_queue_size = 1000
max_bandwidth = None
preferred_transfer_client = auto
target_bandwidth = None
io_chunksize = 256KB
```

Accepts human-readable sizes (e.g. "10MB") and rates (e.g. "10MB/s").

## Cross-Region Redirect

botocore's `S3RegionRedirectorv2` automatically handles 301 PermanentRedirect
by doing a HeadBucket to discover the correct region, then retrying. This is
transparent to the CLI — `aws s3 ls s3://bucket-in-other-region` works without
`--region`. See [compat.md](compat.md) for the Rust SDK difference.

## Test Structure

```
tests/
├── unit/customizations/s3/
│   ├── test_subcommands.py     # Path validation, ListCommand, RbCommand
│   ├── test_utils.py           # URI parsing, size formatting, content type
│   ├── test_results.py         # Output formatting
│   ├── test_filegenerator.py   # File/object enumeration
│   ├── test_filters.py         # Include/exclude
│   ├── test_comparator.py      # Sync comparison
│   ├── test_transferconfig.py  # Config parsing
│   ├── test_s3handler.py       # Transfer handler
│   └── test_subscribers.py     # Progress/lifecycle
├── functional/s3/
│   ├── test_ls_command.py      # 21 tests
│   ├── test_cp_command.py      # ~50 tests
│   ├── test_mv_command.py      # ~20 tests
│   ├── test_rm_command.py      # ~10 tests
│   ├── test_sync_command.py    # ~30 tests
│   ├── test_mb_command.py      # ~8 tests
│   ├── test_rb_command.py      # ~8 tests
│   ├── test_presign_command.py # ~5 tests
│   └── test_website_command.py # ~5 tests
└── unit/customizations/
    └── test_s3errormsg.py      # 301 redirect, KMS error enhancement
```

See [test-traceability.md](test-traceability.md) for the mapping to Rust equivalents.
