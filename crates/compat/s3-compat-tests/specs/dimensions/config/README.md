# config

Configuration resolution and parsing: the layer every `aws s3` command passes through before any S3 work. Given config files + env vars + flags, what config is resolved, and what happens when resolution fails.

Angles swept: issues

## Coverage

14 scenarios — 9 spec, 3 unspeccable, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| profile-not-found / --profile flag | profile_not_found.toml | spec |
| profile-not-found / AWS_PROFILE env | aws_profile_env_not_found.toml | spec |
| profile-flag-overrides-env | profile_flag_overrides_default_profile_env.toml | spec |
| config-file-requires-profile-prefix | config_file_requires_profile_prefix.toml | spec |
| credentials-file-section-naming | credentials_file_section_naming.toml | spec |
| AWS_CONFIG_FILE override | config_file_env_override.toml | spec |
| AWS_SHARED_CREDENTIALS_FILE override | credentials_file_env_override.toml | spec |
| duplicate-profile-parse-error | duplicate_profile_parse_error.toml | spec |
| malformed-s3-subsection | malformed_s3_subsection.toml | spec |
| region-precedence-flag-over-env-over-config | — | unspeccable: request-log (which source won is only observable via the endpoint contacted, which the mock ignores) |
| region-missing-non-s3 | — | n/a: S3 does not require region; "You must specify a region" only fires for non-S3 services |
| no-region-s3-global-endpoint | — | unspeccable: request-log (S3 silently uses us-east-1 / global endpoint; which region was resolved is not observable in stdout/stderr) |
| AWS_CONFIG_FILE-nonexistent-fallback | — | n/a: per #7714 CLI silently falls back; the fallback is indistinguishable from normal default-path resolution without filesystem control |
| UTF-8-BOM-parse-error | — | unspeccable: binary-file-seeding (`[[setup.files]]` cannot seed binary content such as BOM bytes) |

## Notes

- All 9 specs pass both mock and prod.
- `credentials_file_env_override` uses a regex alternation `(InvalidAccessKeyId|NotSignedUp)` because mock and prod return different auth-error codes for the same fake credentials.
- Region resolution precedence (flag > env > config) is unspeccable because the mock ignores region — whichever source wins, the same mock endpoint is hit. This is only observable via a request-log showing which endpoint was contacted.
- S3 never produces "You must specify a region" — it uses the global endpoint / us-east-1 fallback when no region is configured. This behavior was confirmed: `aws s3 ls --no-sign-request` with no region exits 255 with an internal Python error (`expected string or bytes-like object, got 'NoneType'`), not the clean "You must specify a region" error (which EC2 and other regional services produce).
- `#9469` (malformed s3= block): v2 still produces `'str' object has no attribute 'get'` — a raw Python AttributeError, not a user-friendly config parse error.
- `#9261` (duplicate profiles): v2 produces generic `Unable to parse config file: {path}` without identifying which profile is duplicated.
- UTF-8 BOM in a config file causes a parse failure in v2 (the contract is real), but `[[setup.files]]` writes string content only — binary bytes (like the BOM prefix `\xEF\xBB\xBF`) cannot be seeded. This is a harness gap (HG-003).

## Routed elsewhere

| Contract | Owning dimension |
|---|---|
| addressing_style (path\|virtual\|auto) effect on request URL | transfer_defaults |
| multipart_threshold / multipart_chunksize / max_concurrent_requests / max_queue_size effect | transfer_defaults |
| max_bandwidth / target_bandwidth effect | transfer_defaults |
| preferred_transfer_client (crt\|classic\|auto) | transfer_defaults |
| use_accelerate_endpoint effect | transfer_defaults |
| --no-verify-ssl / --ca-bundle / AWS_CA_BUNDLE / REQUESTS_CA_BUNDLE | tls |
| request_checksum_calculation / response_checksum_validation | data_integrity |
| payload_signing_enabled | endpoint/signing |
| credential provider chain (env creds, assume-role, SSO, credential_process, source_profile, IMDS) | credential_resolution |
| AWS_CREDENTIAL_EXPIRATION | credential_resolution |
| s3_disable_express_session_auth / auth_scheme_preference | credential_resolution |
| role_arn / source_profile / mfa_serial / external_id / credential_source | credential_resolution |
| --endpoint-url / AWS_ENDPOINT_URL / AWS_ENDPOINT_URL_S3 / endpoint_url config / services section | endpoint/signing |
| use_dualstack_endpoint / use_fips_endpoint / sigv4a_signing_region_set | endpoint/signing |
| cli_pager / AWS_PAGER / --no-cli-pager / MANPAGER | output_format |
| cli_timestamp_format / --output / output config | output_format |
| --cli-error-format / --output off | output_format |
| user_agent_appid / user-agent string format | user_agent |
| AWS_CLI_FILE_ENCODING / AWS_CLI_OUTPUT_ENCODING / PYTHONUTF8 | locale_encoding |
| retry_mode / max_attempts / AWS_MAX_ATTEMPTS / AWS_NEW_RETRIES_2026 | retry_network |
| --cli-read-timeout / --cli-connect-timeout | retry_network |
| aws configure set/get/list-profiles (not aws s3) | n/a (not aws s3) |
| --source-region for cross-region operations | transfer_defaults |
| signature_version=s3v4 per-profile effect | endpoint/signing |
| io_chunksize effect on download throughput | transfer_defaults |
