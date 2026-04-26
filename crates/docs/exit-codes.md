# Exit Codes

Exit codes match the Python AWS CLI exactly. Source: `awscli/constants.py`.

## Code Table

| Code | Constant | Meaning | Example |
|------|----------|---------|---------|
| 0 | — | Success | Successful ls, cp, sync |
| 1 | `FAILURE` | S3 transfer task failure | `ls s3://bucket/no-match`, failed upload |
| 2 | `WARNING` | S3 transfer task warning | Glacier object skipped |
| 252 | `PARAM_VALIDATION_ERROR` | Argument validation error | Unknown flag, invalid path type |
| 253 | `CONFIGURATION_ERROR` | Configuration error | Bad profile, missing config |
| 254 | `CLIENT_ERROR` | Service/client error | NoSuchBucket, AccessDenied |
| 255 | `GENERAL_ERROR` | General/unexpected error | I/O failure, unexpected exception |

## Mapping

### Error → Exit Code

| Error Type | Exit Code | Python Handler |
|------------|-----------|----------------|
| Service error (SDK) | 254 | `ClientErrorHandler` |
| Invalid argument | 252 | `ParamValidationErrorsHandler` |
| Bad config/profile | 253 | `ConfigurationErrorHandler` |
| I/O / unexpected | 255 | `GeneralErrorHandler` |
| Transfer failure | 1 | `S3TransferCommand._get_rc()` |
| Transfer warning | 2 | `S3TransferCommand._get_rc()` |

### Command-Specific Behavior

**ls:**
- No args (bucket listing) → 0 (even if no buckets)
- `s3://bucket/` with no key filter → 0 (even if empty)
- `s3://bucket/prefix` with no matches → 1
- Service error (NoSuchBucket) → 254

**cp/mv/sync:**
- All transfers succeed → 0
- Any transfer fails → 1
- Glacier warning (no `--force-glacier-transfer`) → 2

**mb/rb:**
- Success → 0
- Service error → 254
- Invalid path → 252

## Rust Implementation

```rust
pub mod exit_code {
    pub const FAILURE: i32 = 1;
    pub const WARNING: i32 = 2;
    pub const PARAM_VALIDATION_ERROR: i32 = 252;
    pub const CONFIGURATION_ERROR: i32 = 253;
    pub const CLIENT_ERROR: i32 = 254;
    pub const GENERAL_ERROR: i32 = 255;
}
```

`handle_s3_cmd` maps `Error` variants to exit codes:
- `Error::SdkService` → 254 (`CLIENT_ERROR`)
- `Error::InvalidUri` → 252 (`PARAM_VALIDATION_ERROR`)
- `Error::Io` → 255 (`GENERAL_ERROR`)

Clap parse errors use `PARAM_VALIDATION_ERROR` (252) for actual errors,
0 for `--help` and `--version` (distinguished via `clap::Error::use_stderr()`).
