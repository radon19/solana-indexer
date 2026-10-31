#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "04 full capture replay"

export REPLAY="$CAPTURE"
unset REPLAY_DLQ
unset REPLAY_LOOP

run_app "$ARTIFACTS/replay.log"
assert_app_ok "capture replay" "$ARTIFACTS/replay.log"
assert_file_contains "$ARTIFACTS/replay.log" "REPLAY pass 1: 6207 updates from"
assert_file_contains "$ARTIFACTS/replay.log" "REPLAY pass 1 done: 6206 updates"
assert_file_contains "$ARTIFACTS/replay.log" "REPLAY complete"
assert_file_not_contains "$ARTIFACTS/replay.log" "moved to DLQ"
assert_file_not_contains "$ARTIFACTS/replay.log" "flush failed"
assert_file_not_contains "$ARTIFACTS/replay.log" "panicked"

assert_expected_counts
assert_sql_eq "DLQ must be empty after a clean replay" \
  "SELECT count(*) FROM dead_letter_batches" "0"
assert_sql_eq "migration success after replay" \
  "SELECT count(*) FROM _sqlx_migrations WHERE success" "1"

printf '\nStored rows:\n'
table_counts | sed 's/^/  /'

pass "full capture replay"

assert_sql_eq "offline replay never advances resume" "SELECT count(*) FROM resume" "0"
