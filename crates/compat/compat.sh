#!/usr/bin/env bash
#
# Compat test runner — convenience wrapper around cargo test.
# Run ./compat.sh help for usage.
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

: "${COMPAT_CLI_BINARY:=aws}"
# Resolve relative paths to absolute so tests find the binary regardless of cwd
if [[ "$COMPAT_CLI_BINARY" == */* && ! "$COMPAT_CLI_BINARY" == /* ]]; then
  COMPAT_CLI_BINARY="$(cd "$(dirname "$COMPAT_CLI_BINARY")" && pwd)/$(basename "$COMPAT_CLI_BINARY")"
fi
export COMPAT_CLI_BINARY

cmd="${1:-test}"
shift || true
filter="${1:-}"
shift || true

cargo_args=(-p s3-compat-tests)
test_args=(--nocapture)
if [[ -n "$filter" ]]; then
    test_args+=("$filter")
fi

case "$cmd" in
    test)
        cargo test "${cargo_args[@]}" -- "${test_args[@]}"
        ;;
    probe)
        COMPAT_MODE=probe cargo test "${cargo_args[@]}" -- "${test_args[@]}"
        ;;
    capture)
        COMPAT_MODE=capture cargo test "${cargo_args[@]}" -- "${test_args[@]}"
        ;;
    validate)
        COMPAT_TARGET=prod cargo test "${cargo_args[@]}" -- "${test_args[@]}"
        ;;
    *)
        cat >&2 <<'EOF'
Usage: ./compat.sh <command> [filter]

Commands:
  test [filter]       Assert specs against golden files (mock)
  probe [filter]      Show CLI output, no assertions (mock)
  capture [filter]    Write/update golden files (mock)
  validate [filter]   Assert specs against golden files (prod S3, requires AWS creds)

Examples:
  ./compat.sh test                              # run all specs against mock
  ./compat.sh test basic_object_listing         # run one spec
  ./compat.sh probe nonexistent_bucket          # see what the CLI outputs
  ./compat.sh capture prefix_listing            # write golden files from output
  AWS_PROFILE=dev ./compat.sh validate          # validate all specs against prod

  RUST_LOG=s3_compat_spec=debug ./compat.sh test basic_object_listing   # debug logging
  RUST_LOG=s3_compat_spec=trace ./compat.sh test basic_object_listing   # full trace

  COMPAT_TARGET=prod ./compat.sh probe nonexistent_bucket   # probe against prod

Environment:
  COMPAT_CLI_BINARY   CLI binary to test (default: aws from PATH)
  COMPAT_TARGET       mock (default) or prod
  COMPAT_KEEP_TEMPDIR Keep temp working dirs for inspection (set to any value)
  RUST_LOG            tracing filter (debug shows setup/exec, trace adds config/env/output)
EOF
        exit 1
        ;;
esac
