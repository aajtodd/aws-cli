# credential_resolution

How the CLI discovers, validates, and orders AWS credentials across the provider
chain: environment variables, the shared credentials file, named-profile config,
`credential_process`, assume-role/STS, container/IMDS, and SSO. Given a set of
credential sources, which one wins, what forms a complete credential, and what
happens when resolution fails.

Angles swept: issues

## Coverage

This dimension spans three evidence tiers, because not all of it is S3-harness
-speccable:

- **Tier A — S3-harness specs** (local/deterministic credential resolution observable
  through an `aws s3` invocation): the specs in this directory.
- **Tier B — `covered: aws-config <test>`** (networked providers — assume-role/STS,
  web-identity, ECS, IMDS, SSO — not reachable through the S3 mock): the compat evidence
  is the `aws-config` provider test suite, the same code path the Rust CLI uses. Cited,
  not re-authored here.
- **Tier C — `unspeccable:<reason>`**: behaviors no backend the harness can drive can
  observe.

Classification legend: `spec` (authored here, mock+prod) · `covered: config/<spec>`
(already specced in the config dimension) · `covered: aws-config <test>` (Tier-B SDK
evidence) · `unspeccable:<reason>`.

### Tier A — specs (this directory)

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| env: secret without access key → ignored, no creds | env_secret_without_access_key_ignored.toml | spec |
| env: `AWS_SECRET_KEY` alias not honored → partial creds | env_aws_secret_key_alias_not_honored.toml | spec |
| profile: incomplete creds (no secret) → partial creds, no fall-through | profile_incomplete_credentials.toml | spec |
| credential_process: success → creds resolved + sent | credential_process_success.toml | spec |
| credential_process: program exits non-zero → retrieval error | credential_process_failure.toml | spec |
| legacy `AWS_CREDENTIAL_FILE` env not honored | aws_credential_file_env_not_honored.toml | spec |

All six pass mock+prod. The two that reach a service auth error
(`credential_process_success`) use the `(InvalidAccessKeyId|NotSignedUp)` regex
alternation (mock returns `NotSignedUp`, prod `InvalidAccessKeyId` for fake creds).

### Covered by the config dimension (provider-independent file/profile mechanics)

These credential-adjacent scenarios are profile SELECTION / config-FILE parsing /
file-LOCATION, owned by `config`. Not re-authored here.

| Scenario | Classification |
|---|---|
| `AWS_SHARED_CREDENTIALS_FILE` relocates the creds file | covered: config/credentials_file_env_override.toml |
| credentials file section naming (`[X]` vs `[profile X]`) | covered: config/credentials_file_section_naming.toml |
| non-existent profile → clear error | covered: config/profile_not_found.toml |
| `--profile` flag vs `AWS_PROFILE`/`AWS_DEFAULT_PROFILE` selection | covered: config/profile_flag_overrides_default_profile_env.toml |
| `AWS_CONFIG_FILE` relocates the config file | covered: config/config_file_env_override.toml |

### Tier B — networked providers (`covered: aws-config <test>`)

Evidence lives in `aws-config` (`/Users/todaaron/sandbox/rs/smithy-rs/aws/rust-runtime/aws-config`):
`src/default_provider/credentials.rs` registers `make_test!(name)` cases, each
replaying recorded traffic from `test-data/default-credential-provider-chain/<name>/`
(`test-case.json` + `env.json` + `fs/` + `http-traffic.json`). Profile-chain
*resolution* (not live calls) is additionally pinned by `test-data/assume-role-tests.json`
(run by `src/profile/credentials/repr.rs`). This is the same credential-resolution
code path the Rust CLI uses. Each row was read to confirm it pins the Python-parity
behavior — `covered` only where it does; `weak`/divergent flagged below.

| Corpus contract | Evidence | Classification |
|---|---|---|
| role_arn + source_profile → AssumeRole chain resolution (#990) | assume-role-tests.json "basic test case" | covered: aws-config assume-role-tests (resolution); e2e STS-call via source_profile is weak (only `credential_source` has a replay test) |
| assume-role REQUIRES source_profile, no fall-through (#2938) | assume-role-tests.json "role_arn without source_profile" → error | covered: aws-config assume-role-tests |
| external_id / credential_source config (#1182, partial) | assume-role-tests.json (external_id, credential_source); imds_assume_role, ecs_assume_role | covered: aws-config (external_id, credential_source) |
| chained / self-referential source_profile + loop detection (#3039) | assume-role-tests.json "multiple chained...", "self referential", loop guards | covered: aws-config assume-role-tests |
| web-identity token file + role_arn → AssumeRoleWithWebIdentity, env form (#80-adjacent) | web_identity_token_env | covered: aws-config web_identity_token_env |
| web-identity token from profile (direct key + via source_profile chain) | web_identity_token_profile, web_identity_token_source_profile | covered: aws-config web_identity_token_profile |
| IMDS can be disabled; no IAM role → error | imds_disabled (`AWS_EC2_METADATA_DISABLED`), imds_no_iam_role | covered: aws-config imds_disabled |
| IMDS instance-profile temp creds (incl. session token) | imds_default_chain_success | covered: aws-config imds_default_chain_success (provider output; S3-request `x-amz-security-token` attachment not exercised here) |
| ECS container credential URI provides creds | ecs_credentials | covered: aws-config ecs_credentials |
| ECS + assume-role chaining | ecs_assume_role | covered: aws-config ecs_assume_role |
| SSO cached-token → GetRoleCredentials (#990-adjacent) | sso_assume_role (+ e2e_fips_and_dual_stack_sso) | covered: aws-config sso_assume_role (verified botocore-compatible SHA1 cache addressing + sso_region endpoint) |
| SSO missing token file → error | sso_no_token_file | covered: aws-config sso_no_token_file (file-missing path only; expired-token sub-case untested) |
| SSO server error → load failure | sso_server_error | covered: aws-config sso_server_error (behavior only; error string is generic, not botocore-typed) |

**Parity caveats (cite as behavior evidence, NOT message-level parity):** several
aws-config tests assert Rust-internal error strings (e.g. ``"`web_identity_token_file`
was specified but `role_arn` was missing"``, `"an error occurred while loading
credentials"`, OS-level `"No such file or directory"`), not botocore's wording — they
prove the path errors out, not that the message matches Python. The `#3334`
"source_profile inherits creds-not-region" aspect and a positive web-identity
precedence ordering have NO test (candidates to add a test-data case).

### Tier C — unspeccable

| Scenario | Classification |
|---|---|
| MFA prompting during assume-role (interactive TTY) | unspeccable: interactive-tty (also a ship-gate open question) |
| credential refresh DURING a long multipart transfer (#635/#3586/#6709) | unspeccable: timing/duration (aws-config covers the refresh mechanism in isolation; the mid-transfer integration is timing-bound) |
| assume-role cache-file path on Windows (#1062/#1063/#2978) | unspeccable: platform (Windows path semantics) |
| `aws s3 sync` continues per-file when SSO token expires mid-sync (#4863/#5796) | unspeccable: CLI sync-loop integration (no aws-config unit test models it; needs a CLI-level integration test, not credential-provider evidence) |
| env-vs-profile / credential-source PRECEDENCE by request inspection | unspeccable: request-log (HG-010 — which source signed is only observable via the request; see note) |

### Known Rust-vs-Python divergences surfaced by the Tier-B audit

These are NOT `covered` — the aws-config evidence shows the Rust path behaves
*differently* from the Python baseline. They are compatibility risks to track, not
contracts we can claim parity on:

| Behavior | Divergence | Status |
|---|---|---|
| `mfa_serial` in a profile (#991/#1235/#3038) | The aws-config provider chain has **zero** `mfa_serial` support (no parse node, no test). Python prompts for an MFA code and sends `SerialNumber`/`TokenCode` to AssumeRole. An `mfa_serial` profile resolves differently (no prompt) on the Rust path. | Behavioral gap — cross-cutting open question (MFA ship-gate). Do NOT classify `covered`. |
| IMDSv1 fallback | Confirmed IMDSv2-only: every recorded IMDS exchange starts with `PUT /latest/api/token`; a token 403 is terminal (`imds_token_fail`), no token-less GET. Python falls back to IMDSv1. | Real divergence; IMDSv1-dependent scenarios are uncovered. |
| IMDS credential fetch retry | `imds_default_chain_retries` pins Rust retrying 503 on every IMDS call. Per #2289, botocore lacks retry-with-backoff for cred fetch. | Divergence (favorable direction), but the test pins non-Python behavior — not parity evidence. |

## Notes

- **The `--profile` flag drops the env credential provider.** botocore sets
  `disable_env_vars` when an explicit `--profile` (or session-explicit profile) is
  given, removing `EnvProvider` from the chain
  (`awscli/botocore/credentials.py:92,167`). So a `--profile`-flag spec is NOT masked
  by the harness's baseline env creds — which is why `profile_incomplete_credentials`
  and the `credential_process` specs need no `default_env` opt-out. `AWS_PROFILE` (env)
  does NOT trigger this: env creds keep precedence. This resolves the corpus's apparent
  #113-vs-#861 contradiction — both are true, for the flag and the env var respectively.
- **`default_env.credentials = false` (HG-012)** is required only for the two specs that
  test the env layer itself with the default profile (`env_secret_without_access_key
  _ignored`, `env_aws_secret_key_alias_not_honored`) — otherwise the harness's injected
  key pair would mask the scenario.
- **#97 reaches the wrong conclusion, but is good provenance.** It requests
  `AWS_SECRET_KEY` be accepted as an alias; the v2 baseline does NOT honor it
  (`EnvProvider` maps secret only to `AWS_SECRET_ACCESS_KEY`). The spec locks the
  baseline (alias ignored) and cites #97 as the contested request.
- **Exit-code vocabulary observed:** 253 = no credentials found anywhere; 255 = partial
  /malformed credentials or a credential_process error (botocore error, not a service
  response); 254 = credentials resolved and a request was signed but the service
  rejected them.
- The env-vs-profile/credential-source PRECEDENCE-by-inspection scenario is request-log
  -blocked (HG-010): when both an env cred and a profile resolve, *which one signed* is
  only visible in the request. The narrower flag-vs-env *selection* behavior is captured
  above via success-vs-failure, but ranking N simultaneously-valid sources is not.
