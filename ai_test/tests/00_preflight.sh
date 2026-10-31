#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "00 preflight"

assert_file_exists "$ROOT/Cargo.toml"
assert_file_exists "$ROOT/Cargo.lock"
assert_file_exists "$ROOT/src/main.rs"
assert_file_exists "$ROOT/src/store/writer.rs"
assert_file_exists "$ROOT/migrations/0001_init.sql"
assert_file_exists "$AI_TEST_DIR/fixtures/empty.bin"
assert_file_exists "$AI_TEST_DIR/fixtures/corrupt_frame.bin"

command -v cargo >/dev/null || fail "cargo is required"
command -v timeout >/dev/null || fail "timeout is required"

if [[ "${AI_TEST_RUNTIME:-1}" == "1" ]]; then
  command -v docker >/dev/null || fail "docker is required for runtime tests"
  command -v psql >/dev/null || fail "psql is required for runtime tests"
  command -v python3 >/dev/null || fail "python3 is required for shutdown signal tests"

  docker info >/dev/null 2>&1 || fail "docker daemon is not reachable"
  [[ -s "$CAPTURE" ]] || fail "capture file missing or empty: $CAPTURE"

  capture_bytes="$(stat -c%s "$CAPTURE")"
  assert_ge "capture size bytes" "$capture_bytes" 1048576
else
  printf 'Runtime prerequisites skipped (AI_TEST_SKIP_RUNTIME=1).\n'
fi

pass "preflight"
