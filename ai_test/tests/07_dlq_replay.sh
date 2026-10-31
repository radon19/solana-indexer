#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "07 DLQ replay and failure handling"

valid_slot=900000001
invalid_slot=900000002

clear_dlq
cleanup_test_slot "$valid_slot"
cleanup_test_slot "$invalid_slot"

valid_id="$(insert_dlq_payload "$AI_TEST_DIR/fixtures/dlq_valid.json" "$valid_slot")"
[[ -n "$valid_id" ]] || fail "could not insert synthetic valid DLQ row"

export REPLAY_DLQ=1
unset REPLAY
run_app "$ARTIFACTS/dlq-valid.log"
assert_app_ok "valid DLQ replay" "$ARTIFACTS/dlq-valid.log"
assert_file_contains "$ARTIFACTS/dlq-valid.log" "DLQ replay complete"
assert_file_contains "$ARTIFACTS/dlq-valid.log" "1 batch(es)"

assert_sql_eq "replayed synthetic slot exists" \
  "SELECT count(*) FROM slots WHERE slot = $valid_slot" "1"
assert_sql_eq "valid DLQ row marked replayed" \
  "SELECT count(*) FROM dead_letter_batches WHERE id = $valid_id AND replayed_at IS NOT NULL AND replay_error IS NULL" "1"

clear_dlq
cleanup_test_slot "$valid_slot"

invalid_id="$(insert_dlq_payload "$AI_TEST_DIR/fixtures/dlq_invalid_version.json" "$invalid_slot")"
[[ -n "$invalid_id" ]] || fail "could not insert synthetic invalid DLQ row"

# The app must fail rather than silently discard an undecodable payload.
run_app "$ARTIFACTS/dlq-invalid.log"
assert_app_failed "invalid-version DLQ replay fails" "$ARTIFACTS/dlq-invalid.log"
assert_file_contains "$ARTIFACTS/dlq-invalid.log" "unsupported DLQ payload version"

assert_sql_eq "invalid DLQ row remains pending" \
  "SELECT count(*) FROM dead_letter_batches WHERE id = $invalid_id AND replayed_at IS NULL" "1"
replay_error="$(psql_val "SELECT replay_error FROM dead_letter_batches WHERE id = $invalid_id")"
[[ "$replay_error" == *"unsupported DLQ payload version"* ]] || fail "invalid row missing replay_error: [$replay_error]"
pass "invalid DLQ row records replay_error"

if grep -Fq "panicked" "$ARTIFACTS/dlq-invalid.log"; then
  warn "DLQ replay failure exits via panic rather than a clean CLI error; see docs/limitations.md"
fi

clear_dlq
cleanup_test_slot "$invalid_slot"

# Nothing pending: replay should be a successful no-op.
run_app "$ARTIFACTS/dlq-empty.log"
assert_app_ok "empty DLQ replay" "$ARTIFACTS/dlq-empty.log"
assert_file_contains "$ARTIFACTS/dlq-empty.log" "0 batch(es)"

# REPLAY_DLQ is deliberately exact: "true" must not enter DLQ mode.
export REPLAY_DLQ=true
export REPLAY="$AI_TEST_DIR/fixtures/empty.bin"
run_app "$ARTIFACTS/replay-dlq-not-exact.log"
assert_app_ok "REPLAY_DLQ=true does not enter DLQ mode" "$ARTIFACTS/replay-dlq-not-exact.log"
assert_file_not_contains "$ARTIFACTS/replay-dlq-not-exact.log" "DLQ replay complete"

unset REPLAY_DLQ
unset REPLAY

pass "DLQ replay and failure handling"
