# filters

Angles swept: issues

The `--include`/`--exclude` pattern-matching semantics that control which files/objects are selected for multi-file operations. Patterns use Python `fnmatch` (Unix shell-style wildcards: `*`, `?`, `[seq]`), are evaluated left-to-right with last-match-wins, and match against the full relative path from the source root. `*` crosses `/` path separators (unlike shell globbing).

## Coverage

19 scenarios — 13 spec, 2 spec (prod-only), 2 covered, 2 n/a

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / recursive — exclude-all then include-txt (allowlist idiom) | cp_recursive_exclude_all_include_txt.toml | spec |
| cp / recursive — last-match-wins (include overrides exclude) | cp_recursive_last_match_wins.toml | spec |
| cp / recursive — `*` glob crosses `/` path separators | cp_recursive_glob_star_crosses_slash.toml | spec |
| cp / recursive — `?` glob matches single character | cp_recursive_glob_question_mark.toml | spec |
| cp / recursive — `[seq]` character class | cp_recursive_glob_char_class.toml | spec |
| cp / recursive — exclude-only (no include) | cp_recursive_exclude_only.toml | spec |
| cp / recursive — include-only has no effect (default=included) | cp_recursive_include_only_noop.toml | spec |
| cp / recursive — exclude all, no re-include → empty match silent exit 0 | cp_recursive_exclude_all_empty_match.toml | spec |
| cp / recursive — multiple interleaved excludes | cp_recursive_interleaved_filters.toml | spec |
| cp / single — exclude matching source file → silent no-op | cp_single_file_exclude_matches.toml | spec |
| mv / recursive (s3→s3) — exclude preserves source objects | mv_recursive_s3_exclude.toml | spec, prod-only |
| rm / recursive — exclude preserves matching keys | rm_recursive_exclude_preserves.toml | spec |
| rm / recursive / dryrun — exclude omits from dryrun output | ../dryrun/rm_dryrun_exclude_filter.toml | covered |
| sync / upload — exclude skips matching files | sync_upload_exclude.toml | spec |
| sync / download — exclude-all include-txt | sync_download_exclude_all_include_txt.toml | spec |
| sync / --delete — excluded files at destination protected | ../sync_strategy/sync_delete_excludes_protected.toml | covered |
| sync / s3→s3 — exclude filter | sync_s3_to_s3_exclude.toml | spec, prod-only |
| --query JMESPath / ls filtering | — | n/a: --query has no effect on `aws s3`; accepted no-op |
| ls --bucket-name-prefix / --bucket-region | — | n/a: not include/exclude filter semantics; distinct ls feature |

## Notes

- `--include` alone (without a preceding `--exclude`) has no effect because all files start with include=True; the include just re-affirms the default.
- Patterns are joined with the source root path via `os.path.join`; they match against the absolute source path (for local) or `bucket/key` (for S3). Leading `./` or `/` in patterns silently fail to match (no normalization).
- The `[seq]` character class means literal `[` and `]` in S3 keys collide with fnmatch syntax; there is no escaping mechanism (IC-024).

**Mock/harness fidelity gaps:**

- **HG-001 (CopyObject):** s3-to-s3 mv/cp/sync with filters requires CopyObject, which the mock lacks. Forces `mv_recursive_s3_exclude` and `sync_s3_to_s3_exclude` to `prod_only`. s3→s3 dryrun with filters runs on the mock (no copy issued).
