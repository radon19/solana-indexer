#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "08 replay performance smoke test"

export REPLAY="$CAPTURE"
unset REPLAY_DLQ
unset REPLAY_LOOP

start_ns="$(date +%s%N)"
run_app "$ARTIFACTS/perf-replay.log"
end_ns="$(date +%s%N)"
assert_app_ok "performance replay" "$ARTIFACTS/perf-replay.log"
assert_file_not_contains "$ARTIFACTS/perf-replay.log" "moved to DLQ"

elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
max_ms="${AI_TEST_MAX_REPLAY_MS:-60000}"
if (( elapsed_ms > max_ms )); then
  fail "replay took ${elapsed_ms} ms, above the ${max_ms} ms guard"
fi
pass "replay completed in ${elapsed_ms} ms (guard ${max_ms} ms)"

assert_expected_counts
assert_sql_eq "DLQ empty after performance replay" \
  "SELECT count(*) FROM dead_letter_batches" "0"

pass "performance smoke test"
