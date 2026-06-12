# exit_codes

Exit codes communicate success, failure, and warnings to calling processes. Transfer commands use 0=success, 1=partial-failure, 2=warning-only; error RC takes precedence over warning RC. Service errors produce 254, validation errors produce 252, and catch-all/unhandled errors produce 255. SIGINT produces 130.

## Coverage

39 scenarios — 7 spec, 11 covered, 14 unspeccable, 7 n/a
(1 spec is prod-only)

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / single upload success exit 0 | commands/cp/upload_single.toml | covered |
| cp / single upload transfer failure exit 1 | cp_upload_transfer_failure.toml | spec, prod-only |
| cp / single upload nonexistent source exit 255 | dimensions/error_format/local_source_missing.toml | covered |
| cp / single download nonexistent key exit 1 | dimensions/error_format/download_nonexistent_key.toml | covered |
| cp / single download nonexistent local dir exit 0 | — | n/a: v2 creates the directory and succeeds |
| cp / recursive nonexistent S3 prefix exit 0 | cp_recursive_nonexistent_prefix.toml | spec |
| cp / recursive partial failure exit 1 | — | unspeccable: fault injection for partial transfer failure |
| cp / recursive broken symlink exit 1 | — | unspeccable: symlink creation in harness setup |
| cp / recursive directory marker failure exit 1 | — | unspeccable: zero-byte directory marker keys |
| cp / dryrun upload exit 0 | commands/cp/upload_single_dryrun.toml | covered |
| cp / streaming to stdout exit codes | — | unspeccable: streaming pipe capture |
| ls / all buckets exit 0 | commands/ls/basic.toml | covered |
| ls / nonexistent bucket exit 254 | commands/ls/nonexistent.toml | covered |
| ls / prefix no match exit 1 | ls_prefix_no_match.toml | spec |
| mb / success exit 0 | commands/mb/basic.toml | covered |
| mb / existing bucket exit 1 (non-us-east-1) | — | unspeccable: setup timing dependency for BucketAlreadyOwnedByYou |
| mv / single success exit 0 | commands/mv/upload_single.toml | covered |
| mv / multipart delete failure exit 1 | — | unspeccable: local filesystem permission control |
| rb / nonexistent bucket exit 1 | rb_nonexistent_bucket.toml | spec |
| rb / path with key exit 252 | rb_path_with_key.toml | spec |
| rb / force gated on object removal | — | unspeccable: fault injection for rm --recursive partial failure |
| rm / single nonexistent key exit 0 | commands/rm/nonexistent.toml | covered |
| rm / recursive empty prefix exit 0 | rm_recursive_empty_prefix.toml | spec |
| rm / recursive partial failure exit 1 | — | unspeccable: fault injection for delete-objects partial failure |
| sync / success exit 0 | commands/sync/local_to_s3.toml | covered |
| sync / partial failure exit 1 | — | unspeccable: fault injection |
| sync / nonexistent bucket exit non-zero | sync_nonexistent_bucket.toml | spec |
| sync / delete from nonexistent source | error_format/sync_delete_nonexistent_source.toml | covered |
| glacier warning exit 2 | — | unspeccable: glacier storage class objects in mock |
| error RC precedence over warning RC | — | unspeccable: glacier storage class objects in mock |
| SIGINT exit 130 | — | unspeccable: signal delivery to child process |
| SIGPIPE silent exit | — | unspeccable: pipe/signal delivery to child process |
| transfer exit 0/1/2 scheme (v2) | — | n/a: meta-contract covered by individual command specs |
| v2 exit code scheme (252/253/254/255) | — | n/a: meta-contract covered by individual error specs |
| Windows %ERRORLEVEL% / PowerShell $LASTEXITCODE | — | n/a: platform-specific, same numeric values |
| help topic named 'return-codes' | — | n/a: documentation system, not runtime behavior |
| cp exits 137 on OOM-kill | — | n/a: OS-level SIGKILL behavior |
| closed stdout fd (1>&-) | — | n/a: edge case, v2 bug per #10257 |
| HTTP 200 with per-key delete errors exit 0 | — | unspeccable: delete-objects partial failure in mock |

## Notes

- Mock cannot reproduce BucketAlreadyOwnedByYou for mb existing bucket — S3 CreateBucket idempotency window prevents reliable reproduction. Classified unspeccable.
- Signal-related specs (SIGINT, SIGPIPE) require delivering signals to child processes during execution, which the harness does not support.
- Glacier/warning exit code 2 requires glacier-class objects which the mock does not support.
- Fault injection (permission denied, ENOSPC, network timeout, partial failure) requires capabilities the harness does not have.
- Corpus correction: reconciliation claimed rb nonexistent bucket would exit 254 (service error path); actual v2 behavior is exit 1 via RbCommand's own exception handler.
