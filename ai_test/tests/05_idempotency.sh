#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "05 replay idempotency"

tx_checksum() {
  psql_val "SELECT md5(string_agg(signature, '|' ORDER BY signature)) FROM transactions"
}

slot_checksum() {
  psql_val "SELECT md5(string_agg(slot::text || ':' || coalesce(parent::text, '') || ':' || status, '|' ORDER BY slot)) FROM slots"
}

before_counts="$(table_counts)"
before_tx="$(tx_checksum)"
before_slots="$(slot_checksum)"

export REPLAY="$CAPTURE"
unset REPLAY_DLQ
unset REPLAY_LOOP
run_app "$ARTIFACTS/replay-second.log"
assert_app_ok "second capture replay" "$ARTIFACTS/replay-second.log"
assert_file_not_contains "$ARTIFACTS/replay-second.log" "moved to DLQ"
assert_file_not_contains "$ARTIFACTS/replay-second.log" "flush failed"

after_counts="$(table_counts)"
after_tx="$(tx_checksum)"
after_slots="$(slot_checksum)"

if [[ "$before_counts" != "$after_counts" ]]; then
  printf 'Before:\n%s\nAfter:\n%s\n' "$before_counts" "$after_counts" >&2
  fail "table counts changed on second replay"
fi
pass "table counts are stable across replay"

assert_eq "transaction signature checksum" "$after_tx" "$before_tx"
assert_eq "slot table checksum" "$after_slots" "$before_slots"
assert_sql_eq "DLQ still empty after second replay" \
  "SELECT count(*) FROM dead_letter_batches" "0"

pass "replay idempotency"
