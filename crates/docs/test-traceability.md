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
| `test_requester_pays_with_no_args` | — | ❌ Blocked: clap requires explicit value |
| `test_accesspoint_arn` | — | ⏳ ARN parsing not implemented |
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
| `test_accesspoint_arn` | — | ⏳ ARN parsing not implemented |
| `test_accesspoint_arn_with_slash` | — | ⏳ |
| `test_accesspoint_arn_with_key` | — | ⏳ |
| `test_accesspoint_arn_with_key_and_prefix` | — | ⏳ |
| `test_outpost_arn_*` (8 tests) | — | ⏳ |
| `test_object_lambda_arn_*` (2 tests) | — | ⏳ |
| `test_outpost_bucket_arn_*` (2 tests) | — | ⏳ |

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

## functional/s3/ — Other commands

| File | Tests | Status |
|------|-------|--------|
| `test_cp_command.py` | ~50 tests | ⏳ cp not implemented |
| `test_mv_command.py` | ~20 tests | ⏳ mv not implemented |
| `test_sync_command.py` | ~30 tests | ⏳ sync not implemented |
| `test_presign_command.py` | ~5 tests | ⏳ presign not implemented |
| `test_website_command.py` | ~5 tests | ⏳ website not implemented |

## unit/customizations/test_s3errormsg.py

| Python Test | Rust Equivalent | Status |
|-------------|-----------------|--------|
| `test_301_error_message` | — | ❌ Cross-region redirect not implemented |
| `test_kms_sigv4_error_message` | — | ➖ Handled differently in Rust SDK |
| `test_error_message_not_enhanced` | — | ❌ |

## Rust-only tests (no Python equivalent)

These test Rust-specific concerns or expand coverage beyond the Python suite.

| Rust Test | What it tests |
|-----------|---------------|
| `cli::tests::global_args_cover_cli_json` | Every option in cli.json is accepted by our parser |
| `cli::tests::globals_*` (6 tests) | Global flag parsing (before/after subcommand, defaults) |
| `cli::tests::*_missing_*_is_error` (6 tests) | Missing required args for each command |
| `uri::tests::transfer_uri_*` (3 tests) | TransferUri enum parsing and Display |
| `uri::tests::path_type_error_format_matches_cli` | Error message format matches Python CLI |
| `format::tests::format_datetime_*` (4 tests) | Date formatting correctness |
| `format::tests::format_size_*` (2 tests) | Size field alignment |
| `format::tests::human_readable_zero` | Edge case: 0 bytes |
| `term::test_support::tests::*` (9 tests) | InMemoryTerminal correctness |
| `ls::tests::display_page_*` (5 tests) | Output formatting with known SDK types |

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
need resolution. Each should have a corresponding test when fixed.

| Gap | Impact | Python Behavior | Our Behavior |
|-----|--------|-----------------|--------------|
| Cross-region bucket redirect | All S3 operations | Auto-redirects via HeadBucket | Returns 301 error |
| `--request-payer` without value | ls | Defaults to "requester" | Requires explicit value |
| Access point ARN parsing | URI parsing | Handles ARN formats | Only handles s3://bucket/key |
| Error message for 301 redirect | Error output | Enhanced with endpoint info | Raw error |
| `--no-verify-ssl` wiring | SDK config | Applied to SDK client | Parsed but not applied |
