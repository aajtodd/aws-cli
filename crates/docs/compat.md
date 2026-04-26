# Behavioral Gaps

Differences between the Rust SDK and botocore/Python CLI that affect
behavioral compatibility. Each gap needs resolution before the affected
commands can pass compat tests.

## Cross-Region Bucket Redirect

**Impact:** All S3 operations against buckets in a different region than
the configured default.

**Python CLI behavior:** botocore's `S3RegionRedirectorv2` intercepts 301
PermanentRedirect errors, does a HeadBucket to discover the correct region,
and retries the request transparently. `aws s3 ls s3://bucket-in-us-west-2`
works from any region without `--region`.

**Rust SDK behavior:** Returns the 301 as a service error. No automatic
redirect.

**Options:**
1. Implement redirect logic (HeadBucket → extract region → rebuild client → retry)
    - We don't need to rebuild client, SDK allows for per/operation overrides. 
2. Always resolve bucket region via HeadBucket before first operation
3. Require `--region` (breaks compat, not viable)

**Python tests:** `tests/unit/botocore/test_utils.py` (`S3RegionRedirectorv2`),
`tests/unit/customizations/test_s3errormsg.py` (`test_301_error_message`).

## Error Message Format

**Impact:** All error output.

**Python CLI behavior:** Service errors formatted as:
```
An error occurred ({error_code}) when calling the {operation_name} operation: {error_message}
```
Source: `botocore/exceptions.py` `ClientError.MSG_TEMPLATE`.

**Rust SDK behavior:** `SdkError::Display` prints "service error" with no
structured format. Error code and message are available via
`ProvideErrorMetadata`, but the operation name is not carried on the error.

**Resolution:** `format_sdk_error(err, operation_name)` extracts code and
message from `ProvideErrorMetadata` and formats to match. Operation name
passed from the call site. Implemented.

## SDK Option Semantics

**Impact:** ListObjectsV2 and other operations where `None` vs empty string
matters.

**Python CLI behavior:** botocore always sends `Prefix` (even as empty string)
and only sends `Delimiter` for non-recursive listing.

**Rust SDK behavior:**
- `.delimiter("/")` → `Some("/")` (sent in request)
- Not calling `.delimiter()` → `None` (omitted from request)
- `.prefix("")` → `Some("")` (sent as `Prefix=`)

**Resolution:** Match the Python CLI's behavior explicitly. Non-recursive
listing sets `delimiter("/")` and `prefix("")`. Recursive listing sets
`prefix("")` and omits delimiter. Implemented.

## Access Point ARN Parsing

**Impact:** Commands targeting access points, outpost ARNs, MRAP ARNs.

**Python CLI behavior:** `find_bucket_key()` in `utils.py` handles access
point ARNs (`arn:aws:s3:...`), outpost ARNs, and MRAP ARNs via regex
matching. Object Lambda and outpost bucket ARNs are rejected.

**Rust implementation:** Currently only handles `s3://bucket/key` format.
ARN parsing not yet implemented.

**Python tests:** `test_utils.py` has 15+ ARN parsing tests.

## Content-Type Detection

**Impact:** cp, mv, sync uploads.

**Python CLI behavior:** Uses Python's `mimetypes.guess_type()` to detect
content type from file extension.

**Rust implementation:** Not yet implemented. Will use `mime_guess` crate.
Some extensions will produce different results — these need to be cataloged
and tracked as known deviations.

## `--request-payer` Optional Value

**Impact:** ls, cp, mv, rm, sync with `--request-payer`.

**Python CLI behavior:** Uses `nargs='?'` with `const='requester'` — the
flag can be used with or without a value. `--request-payer` alone means
`--request-payer requester`.

**Rust implementation:** Currently requires an explicit value. Clap's
`default_missing_value` should handle this but needs verification.

## Streaming (`-`) Support

**Impact:** cp with stdin/stdout.

**Python CLI behavior:** `cp - s3://bucket/key` reads from stdin.
`cp s3://bucket/key -` writes to stdout. Only compatible with non-recursive
cp.

**Rust implementation:** `TransferUri` does not have a Stdio variant.
Not yet implemented.

## `--no-verify-ssl` Wiring

**Impact:** All commands when SSL verification is disabled.

**Python CLI behavior:** Disables SSL certificate verification on the
HTTP client.

**Rust implementation:** Parsed as a global flag but not applied to the
SDK client configuration.
