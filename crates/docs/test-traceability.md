# Test Traceability

Maps Python AWS CLI tests to their Rust equivalents. The Python CLI tests
are the source of truth for behavioral correctness.

Status key:
- ✅ Ported — equivalent Rust test exists
- ⏳ Pending — command not yet implemented
- ➖ N/A — not applicable (SDK-level, Python-specific, or out of scope)
- ❌ Missing — should exist but doesn't

## functional/s3/test_ls_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_operations_used_in_recursive_list` | `ls::tests::run_list_objects_recursive_no_delimiter` | ✅ |
| `test_errors_out_with_extra_arguments` | `cli::tests::unknown_flag_is_error`, `unknown_flag_mentioned_in_error` | ✅ |
| `test_list_buckets_use_page_size` | `ls::tests::run_list_buckets_page_size` | ✅ |
| `test_operations_use_page_size` | `ls::tests::run_list_objects_page_size` | ✅ |
| `test_operations_use_page_size_recursive` | `ls::tests::run_list_objects_recursive_page_size` | ✅ |
| `test_success_rc_has_prefixes_and_objects` | `ls::tests::run_mixed_prefixes_and_objects_returns_0` | ✅ |
| `test_success_rc_has_only_prefixes` | `ls::tests::run_only_prefixes_returns_0` | ✅ |
| `test_success_rc_has_only_objects` | `ls::tests::run_list_objects_non_recursive` (implicit) | ✅ |
| `test_success_rc_with_pagination` | `ls::tests::run_pagination_with_empty_second_page_returns_0` | ✅ |
| `test_success_rc_empty_bucket_no_key_given` | `ls::tests::run_empty_bucket_returns_0` | ✅ |
| `test_fail_rc_no_objects_nor_prefixes` | `ls::tests::run_no_match_returns_1` | ✅ |
| `test_human_readable_file_size` | `format::tests::human_readable_*` (11 tests) | ✅ |
| `test_summarize` | `ls::tests::run_summarize` | ✅ |
| `test_summarize_with_human_readable` | `ls::tests::run_summarize_human_readable` | ✅ |
| `test_requester_pays` | `ls::tests::run_list_objects_request_payer` | ✅ |
| `test_requester_pays_with_no_args` | `cli::tests::ls_request_payer_bare_flag_defaults_to_requester` | ✅ |
| `test_accesspoint_arn` | `uri::tests::arn_standard_access_point_*` (covers parse; SDK handles endpoint routing) | ✅ |
| `test_list_buckets_uses_bucket_name_prefix` | `ls::tests::run_list_buckets_with_prefix_filter` | ✅ |
| `test_list_buckets_uses_bucket_region` | `ls::tests::run_list_buckets_with_region_filter` | ✅ |
| `test_list_objects_ignores_bucket_name_prefix` | `ls::tests::run_list_objects_ignores_bucket_name_prefix` | ✅ |
| `test_list_objects_ignores_bucket_region` | `ls::tests::run_list_objects_ignores_bucket_region` | ✅ |

## unit/customizations/s3/test_subcommands.py

### ListCommand tests

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_ls_command_for_bucket` | `ls::tests::run_list_objects_non_recursive` | ✅ |
| `test_ls_command_with_no_args` | `ls::tests::run_list_buckets` | ✅ |
| `test_ls_with_bucket_name_prefix` | `ls::tests::run_list_buckets_with_prefix_filter` | ✅ |
| `test_ls_with_bucket_region` | `ls::tests::run_list_buckets_with_region_filter` | ✅ |
| `test_ls_with_verify_argument` | — | ➖ SSL verify handled by SDK config |
| `test_ls_with_requester_pays` | `ls::tests::run_list_objects_request_payer` | ✅ |

### RbCommand tests

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_rb_command_with_force_deletes_objects_in_bucket` | — | ⏳ rb not implemented |
| `test_rb_command_with_force_requires_strict_path` | — | ⏳ |

### CommandParameters (path validation)

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_check_path_type_pass` | `uri::tests::path_type_valid_*` | ✅ |
| `test_check_path_type_fail` | `uri::tests::path_type_*_rejects_invalid` | ✅ |
| `test_validate_streaming_paths_upload` | — | ⏳ Streaming not implemented |
| `test_validate_streaming_paths_download` | — | ⏳ |
| `test_validate_streaming_paths_with_no_overwrite` | — | ⏳ |
| `test_validate_no_streaming_paths` | — | ⏳ |
| `test_validate_streaming_paths_error` | — | ⏳ |
| `test_validate_checksum_algorithm_download_error` | — | ⏳ |
| `test_validate_checksum_algorithm_sync_download_error` | — | ⏳ |
| `test_validate_checksum_mode_upload_error` | — | ⏳ |
| `test_validate_checksum_mode_sync_upload_error` | — | ⏳ |
| `test_validate_checksum_mode_move_error` | — | ⏳ |
| `test_validate_non_existent_local_path_upload` | — | ⏳ |
| `test_add_path_for_non_existsent_local_path_download` | — | ⏳ |
| `test_validate_sse_c_args_missing_sse` | — | ⏳ |
| `test_validate_sse_c_args_missing_sse_c_key` | — | ⏳ |
| `test_validate_sse_c_args_missing_sse_c_copy_source` | — | ⏳ |
| `test_validate_sse_c_args_missing_sse_c_copy_source_key` | — | ⏳ |
| `test_validate_sse_c_args_wrong_path_type` | — | ⏳ |
| `test_adds_is_move` | — | ⏳ mv not implemented |

## unit/customizations/s3/test_utils.py

### Size formatting

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_human_readable_size` (11 cases) | `format::tests::human_readable_*` (12 tests) | ✅ |
| `test_convert_human_readable_to_int` (12 cases) | — | ⏳ Transfer config not implemented |

### URI / bucket-key parsing

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_unicode` | `uri::tests::key_with_unicode` | ✅ |
| `test_bucket` | `uri::tests::bucket_only` | ✅ |
| `test_bucket_with_slash` | `uri::tests::bucket_with_trailing_slash` | ✅ |
| `test_bucket_with_key` | `uri::tests::bucket_and_key` | ✅ |
| `test_bucket_with_key_and_prefix` | `uri::tests::bucket_and_key` | ✅ |
| `test_accesspoint_arn` | `uri::tests::arn_standard_access_point_no_key` | ✅ |
| `test_accesspoint_arn_with_slash` | `uri::tests::arn_standard_access_point_no_key` (trailing-slash equivalent covered by Python regex; our parser handles via `/` separator) | ✅ |
| `test_accesspoint_arn_with_key` | `uri::tests::arn_standard_access_point_with_key` | ✅ |
| `test_accesspoint_arn_with_key_and_prefix` | `uri::tests::arn_standard_access_point_deeply_nested_key` | ✅ |
| `test_outpost_arn_*` (8 tests) | `uri::tests::arn_outposts_access_point_*` (3 tests) + `arn_outposts_bucket_rejected` + `arn_outposts_all_colon_separators` | ✅ |
| `test_object_lambda_arn_*` (2 tests) | `uri::tests::arn_object_lambda_rejected` | ✅ |
| `test_outpost_bucket_arn_*` (2 tests) | `uri::tests::arn_outposts_bucket_rejected` | ✅ |

### Other utils

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_guess_content_type*` | — | ⏳ Content-type detection not implemented |
| `test_relpath_*` | — | ⏳ Path handling not implemented |
| `test_*_request_params_*` | — | ⏳ Request param mapping not implemented |
| `test_resolves_*` (ARN resolution) | — | ⏳ |

## functional/s3/test_mb_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_make_bucket` | `mb::tests::make_bucket_success` | ✅ |
| `test_adds_location_constraint` | `mb::tests::adds_location_constraint` | ✅ |
| `test_location_constraint_not_added_on_us_east_1` | `mb::tests::no_location_constraint_for_us_east_1` | ✅ |
| `test_nonzero_exit_if_invalid_path_provided` | `mb::tests::invalid_path_returns_252` | ✅ |
| `test_incompatible_with_express_directory_bucket` | `mb::tests::rejects_s3_express_directory_bucket` | ✅ |
| `test_make_bucket_with_single_tag` | `mb::tests::single_tag` | ✅ |
| `test_make_bucket_with_single_tag_us_east_1` | `mb::tests::tags_us_east_1_no_location_constraint` | ✅ |
| `test_make_bucket_with_multiple_tags` | `mb::tests::multiple_tags` | ✅ |
| `test_account_regional_namespace_bucket` | `mb::tests::account_regional_namespace_bucket` | ✅ |
| `test_account_regional_namespace_bucket_us_east_1` | `mb::tests::account_regional_namespace_us_east_1` | ✅ |
| `test_account_regional_namespace_short_bucket_name` | `mb::tests::short_an_bucket` | ✅ |
| `test_regular_bucket_no_namespace` | `mb::tests::regular_bucket_no_namespace` | ✅ |
| `test_tags_with_three_arguments_fails` | `cli::tests::mb_missing_path_is_error` | ✅ Clap rejects extra args |

## functional/s3/test_rb_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_rb` | `rb::tests::remove_bucket_success` | ✅ |
| `test_rb_force_empty_bucket` | `rb::tests::force_empty_bucket` | ✅ |
| `test_rb_force_non_empty_bucket` | `rb::tests::force_non_empty_bucket` | ✅ |
| `test_rb_failed_rc` | `rb::tests::delete_bucket_failure_returns_1` | ✅ |
| `test_rb_force_with_failed_rm` | `rb::tests::force_with_failed_list_returns_255` | ✅ |
| `test_nonzero_exit_if_uri_scheme_not_provided` | `rb::tests::invalid_path_returns_252` | ✅ |
| `test_nonzero_exit_if_key_provided` | `rb::tests::key_provided_returns_252`, `key_with_force_returns_252` | ✅ |

## functional/s3/test_rm_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_operations_used` | `rm::tests::single_delete` | ✅ |
| `test_dryrun_delete` | `rm::tests::dryrun_delete` | ✅ |
| `test_delete_with_request_payer` | `rm::tests::delete_with_request_payer` | ✅ |
| `test_recursive_delete_with_requests` | `rm::tests::recursive_delete_with_request_payer` | ✅ |
| `test_delete_using_crt_client` | — | ➖ CRT-specific |
| `test_recursive_delete_using_crt_client` | — | ➖ CRT-specific |

## functional/s3/test_presign_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_generates_a_url` | `presign::tests::generates_url` | ✅ |
| `test_handles_non_dns_compatible_buckets` | `presign::tests::non_dns_compatible_bucket_falls_back_to_path_style` | ✅ |
| `test_handles_expires_in` | `presign::tests::custom_expires_in` | ✅ |
| `test_handles_sigv4` | — | ➖ Rust SDK uses sigv4 by default; no opt-in |
| `test_s3_prefix_not_needed` | `presign::tests::s3_prefix_not_required` | ✅ |
| `test_can_support_addressing_mode_config` | — | ⏳ `--addressing-style` / `s3.addressing_style` config not wired |

## functional/s3/test_website_command.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_index_document` | `website::tests::index_document` | ✅ |
| `test_error_document` | `website::tests::error_document` | ✅ |

## functional/s3/ — Other commands

| File | Tests | Status |
|------|-------|--------|
| `test_cp_command.py` | ~50 tests | ⏳ cp not implemented |
| `test_mv_command.py` | ~20 tests | ⏳ mv not implemented |
| `test_sync_command.py` | ~30 tests | ⏳ sync not implemented |

## unit/customizations/test_s3errormsg.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_301_error_message` | — | ❌ Cross-region redirect not implemented |
| `test_kms_sigv4_error_message` | — | ➖ Handled differently in Rust SDK |
| `test_error_message_not_enhanced` | — | ❌ |

## unit/customizations/test_globalargs.py

Python test file is the authoritative reference for each global flag's
behavior (value parsing, handler registration, precedence). Our wiring
lives in `main.rs::build_context` and `config.rs`; parser tests live in
`cli::tests::globals_*`.

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_no_sign_request_if_option_specified` | `main.rs::build_context` → `ConfigLoader::no_credentials()` | ✅ (wiring; SDK effect smoke-tested against public bucket, see `smoke-testing.md`) |
| `test_request_signed_by_default` | Default `GlobalArgs` does not set `no_credentials()` | ✅ |
| `test_cli_read_timeout` | `config::tests::timeout_config_set_both` | ✅ |
| `test_cli_connect_timeout` | `config::tests::timeout_config_set_both` | ✅ |
| `test_cli_read_timeout_for_blocking` | `config::tests::timeout_config_zero_means_disabled` | ✅ |
| `test_cli_connect_timeout_for_blocking` | `config::tests::timeout_config_zero_means_disabled` | ✅ |
| `test_parse_verify_ssl_default_value` | `cli::tests::globals_default_values` | ✅ (parser only) |
| `test_parse_verify_ssl_verify_turned_off` | — | ❌ (flag rejected at arg-parse) |
| `test_cli_overrides_cert_bundle` | — | ❌ (flag rejected at arg-parse) |
| `test_cli_overrides_env_cert_bundle` | — | ❌ (flag rejected at arg-parse) |
| `test_no_verify_ssl_overrides_cli_cert_bundle` | — | ❌ (both flags rejected at arg-parse) |

## Rust-only tests (no Python equivalent)

These test Rust-specific concerns or expand coverage beyond the Python suite.

| Rust Test | What it tests |
|-----------|---------------|
| `cli::tests::global_args_cover_cli_json` | Every option in cli.json is accepted by our parser |
| `cli::tests::globals_*` (6 tests) | Global flag parsing (before/after subcommand, defaults) |
| `cli::tests::*_missing_*_is_error` (6 tests) | Missing required args for each command |
| `uri::tests::transfer_uri_*` (3 tests) | TransferUri enum parsing and Display |
| `uri::tests::arn_*` (16 tests) | Access point ARN parsing (standard, MRAP, Outposts, Object Lambda rejection, partition variants, colon-sep, deep keys, Display roundtrip) |
| `arn::tests::*` (7 tests) | `Arn::parse` unit tests (field extraction + 4 error variants) |
| `uri::tests::path_type_error_format_matches_cli` | Error message format matches Python CLI |
| `format::tests::format_datetime_*` (4 tests) | Date formatting correctness |
| `format::tests::format_size_*` (2 tests) | Size field alignment |
| `format::tests::human_readable_zero` | Edge case: 0 bytes |
| `term::test_util::tests::*` (9 tests) | InMemoryTerminal correctness |
| `ls::tests::display_page_*` (5 tests) | Output formatting with known SDK types |
| `paths::tests::*` (11 tests) | Relative-path formatting (matches `awscli/customizations/s3/utils.py::relative_path`) |
| `config::tests::http_client_builds_default` | `build_http_client` produces a usable HTTP client |
| `config::tests::timeout_config_*` (5 tests) | `--cli-read-timeout` / `--cli-connect-timeout` wiring, including Python's `0 = disabled` semantic |
| `config::tests::ca_bundle_*` (6 tests, `#[ignore]`) | `build_ca_bundle_tls_context` error paths + valid PEM — kept ignored while `--ca-bundle` rejects at arg-parse |

## Exit codes

Source: `awscli/constants.py`

| Code | Python Constant | Meaning | Rust Constant | Tested |
|------|-----------------|---------|---------------|--------|
| 0 | — | Success | — | ✅ |
| 1 | — | S3 transfer task failure | `exit_code::FAILURE` | ✅ (ls no-match) |
| 2 | — | S3 transfer task warning (glacier) | `exit_code::WARNING` | ⏳ |
| 252 | `PARAM_VALIDATION_ERROR_RC` | Argument validation error | `exit_code::PARAM_VALIDATION_ERROR` | ✅ (clap errors) |
| 253 | `CONFIGURATION_ERROR_RC` | Configuration error | `exit_code::CONFIGURATION_ERROR` | ⏳ |
| 254 | `CLIENT_ERROR_RC` | Service/client error | `exit_code::CLIENT_ERROR` | ✅ (SDK errors) |
| 255 | `GENERAL_ERROR_RC` | General error | `exit_code::GENERAL_ERROR` | ⏳ |

## Known behavioral gaps

These are differences between the Python CLI and our implementation that
need resolution. Each should have a corresponding test when fixed. See
`compat.md` for design-level detail and resolution paths.

| Gap | Impact | Python Behavior | Our Behavior |
|-----|--------|-----------------|--------------|
| Cross-region bucket redirect | All S3 operations | Auto-redirects via HeadBucket | Returns 301 error |
| Error message for 301 redirect | Error output | Enhanced with endpoint info | Raw error |
| `--no-verify-ssl` | SDK config | Disables TLS verification | Rejected at arg-parse (smithy-rs has no public toggle) |
| `--ca-bundle` | SDK config | Replaces system trust store | Rejected at arg-parse (TM per-thread HTTP clients have no TlsContext hook; rejecting is preferable to honoring on ls but not cp) |
| `--cli-read-timeout` semantics | Timeouts | Per-socket-read | Time-to-first-byte from request start |
| `--debug` output format | Log output | stdlib `logging` format | `tracing_subscriber::fmt` default (ANSI, ISO-8601) |
| `--cli-auto-prompt` / `--no-cli-auto-prompt` | Missing-arg handling | Interactive prompt loop before dispatch | Parsed but ignored (no interactive prompting) |
| S3-specific config keys | `~/.aws/config [s3]` + `AWS_S3_*` | botocore parses and applies | aws-config does not parse; we have not wired |
| User agent format | All requests | Custom CLI format with feature tags | SDK default (audit pending) |
