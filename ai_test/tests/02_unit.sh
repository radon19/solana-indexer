#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "02 unit tests"

unit_log="$ARTIFACTS/cargo-test.log"
if ! (cd "$ROOT" && cargo test --bin solana-indexer) >"$unit_log" 2>&1; then
  tail -n 60 "$unit_log" >&2 || true
  fail "cargo test --bin solana-indexer failed"
fi

grep -Fq "test result: ok." "$unit_log" || fail "no successful test result line in $unit_log"
assert_file_not_contains "$unit_log" "FAILED"

passed="$(grep -oE 'test result: ok\. [0-9]+ passed' "$unit_log" | awk '{print $4}' | tail -n 1 || true)"
if [[ -z "$passed" ]]; then
  fail "could not parse passed test count"
fi
assert_ge "unit tests passed" "$passed" "${AI_TEST_MIN_UNIT_TESTS:-24}"

pass "unit tests"
