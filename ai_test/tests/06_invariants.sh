#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "06 data invariants"

# Chain state.
assert_sql_zero "transactions with empty signature" \
  "SELECT count(*) FROM transactions WHERE signature = ''"
assert_sql_zero "negative transaction fees" \
  "SELECT count(*) FROM transactions WHERE fee < 0"
assert_sql_zero "non-positive transaction slots" \
  "SELECT count(*) FROM transactions WHERE slot <= 0"
assert_sql_zero "transactions without a fee payer" \
  "SELECT count(*) FROM transactions WHERE fee_payer IS NULL OR fee_payer = ''"
assert_sql_zero "failed transactions in the successful-only capture" \
  "SELECT count(*) FROM transactions WHERE success = false"

assert_sql_zero "slots with invalid status" \
  "SELECT count(*) FROM slots WHERE status NOT IN ('processed','confirmed','finalized','dead','other')"
assert_sql_zero "non-positive slot numbers" \
  "SELECT count(*) FROM slots WHERE slot <= 0"
assert_sql_zero "slot parent not below child" \
  "SELECT count(*) FROM slots WHERE parent IS NOT NULL AND parent >= slot"

assert_sql_zero "block_meta with empty hashes" \
  "SELECT count(*) FROM block_meta WHERE blockhash = '' OR parent_blockhash = ''"
assert_sql_zero "block_meta parent not below child" \
  "SELECT count(*) FROM block_meta WHERE parent_slot >= slot"
assert_sql_zero "negative block transaction counts" \
  "SELECT count(*) FROM block_meta WHERE tx_count < 0"

# Swap fields.
assert_sql_zero "swap rows with empty user/protocol" \
  "SELECT count(*) FROM swap_events WHERE user_address = '' OR protocol = ''"
assert_sql_zero "swap rows with negative amounts" \
  "SELECT count(*) FROM swap_events WHERE input_amount < 0 OR output_amount < 0"
assert_sql_zero "duplicate swap primary keys" \
  "SELECT count(*) - count(DISTINCT (slot, signature, ix_index, parent_ix_index, is_inner)) FROM swap_events"

# Known parser data-quality defect: V1 parsers use mint_of(), which returns an
# empty string when no matching token balance exists. Collect the counts here
# and report one known bug after the liquidity checks.
empty_swap_mints="$(psql_val "SELECT count(*) FROM swap_events WHERE input_mint = '' OR output_mint = ''")"
empty_swap_input="$(psql_val "SELECT count(*) FROM swap_events WHERE input_mint = ''")"
empty_swap_output="$(psql_val "SELECT count(*) FROM swap_events WHERE output_mint = ''")"
assert_sql_zero "empty-mint swap rows with zero amount" \
  "SELECT count(*) FROM swap_events WHERE (input_mint = '' AND input_amount = 0) OR (output_mint = '' AND output_amount = 0)"

# Route hops.
assert_sql_zero "route hops with percent outside 0..10000" \
  "SELECT count(*) FROM route_hops WHERE percent_bps < 0 OR percent_bps > 10000"
assert_sql_zero "route hops with negative hop index" \
  "SELECT count(*) FROM route_hops WHERE hop_index < 0"
assert_sql_zero "route hops with empty protocol" \
  "SELECT count(*) FROM route_hops WHERE protocol = ''"
assert_sql_zero "route hops with negative amounts" \
  "SELECT count(*) FROM route_hops WHERE in_amount < 0 OR out_amount < 0"
assert_sql_zero "orphan route hops" \
  "SELECT count(*) FROM route_hops h LEFT JOIN swap_events s USING (slot, signature, ix_index, parent_ix_index, is_inner) WHERE s.signature IS NULL"

# Trades.
assert_sql_zero "trades with invalid side" \
  "SELECT count(*) FROM trade_events WHERE side NOT IN ('buy','sell')"
assert_sql_zero "trades with empty required strings" \
  "SELECT count(*) FROM trade_events WHERE user_address = '' OR protocol = '' OR mint = ''"
assert_sql_zero "trades with negative token amounts" \
  "SELECT count(*) FROM trade_events WHERE token_amount < 0"
negative_quotes="$(psql_val "SELECT count(*) FROM trade_events WHERE quote_amount < 0")"
if [[ "$negative_quotes" == "0" ]]; then
  pass "trades with negative quote amounts = 0"
else
  known_bug "BUG-005" "$negative_quotes trade row(s) have a negative quote_amount (u64::MAX cast to i64); see docs/limitations.md"
fi

# Launches.
assert_sql_zero "launches with empty required strings" \
  "SELECT count(*) FROM launch_events WHERE protocol = '' OR mint = '' OR creator = '' OR user_address = '' OR name = '' OR symbol = '' OR uri = '' OR bonding_curve = ''"
assert_sql_zero "launches with non-positive timestamp" \
  "SELECT count(*) FROM launch_events WHERE timestamp_unix_ms <= 0"

# Liquidity and pools.
assert_sql_zero "liquidity rows with invalid kind" \
  "SELECT count(*) FROM liquidity_events WHERE kind NOT IN ('add','remove')"
assert_sql_zero "liquidity rows with empty protocol/pool/user" \
  "SELECT count(*) FROM liquidity_events WHERE protocol = '' OR pool = '' OR user_address = ''"
assert_sql_zero "liquidity rows with negative amounts" \
  "SELECT count(*) FROM liquidity_events WHERE token_a_amount < 0 OR token_b_amount < 0"
empty_liq_mints="$(psql_val "SELECT count(*) FROM liquidity_events WHERE token_a_mint = '' OR token_b_mint = ''")"

assert_sql_zero "pool rows with empty required strings" \
  "SELECT count(*) FROM pool_events WHERE protocol = '' OR pool = '' OR token_a_mint = '' OR token_b_mint = ''"

# Report the shared mint_of() defect once, with per-table evidence. The default
# run keeps going; AI_TEST_STRICT=1 makes this fatal.
empty_mint_total=$((empty_swap_mints + empty_liq_mints))
if (( empty_mint_total == 0 )); then
  pass "no empty input/output mint fields"
else
  known_bug "BUG-003" "empty mints: swap rows=$empty_swap_mints (input=$empty_swap_input, output=$empty_swap_output), liquidity rows=$empty_liq_mints; see docs/limitations.md"
fi

# Logical referential integrity: every event signature must exist in transactions.
assert_sql_zero "orphan swap transactions" \
  "SELECT count(*) FROM swap_events e LEFT JOIN transactions t USING (signature) WHERE t.signature IS NULL"
assert_sql_zero "orphan trade transactions" \
  "SELECT count(*) FROM trade_events e LEFT JOIN transactions t USING (signature) WHERE t.signature IS NULL"
assert_sql_zero "orphan launch transactions" \
  "SELECT count(*) FROM launch_events e LEFT JOIN transactions t USING (signature) WHERE t.signature IS NULL"
assert_sql_zero "orphan liquidity transactions" \
  "SELECT count(*) FROM liquidity_events e LEFT JOIN transactions t USING (signature) WHERE t.signature IS NULL"
assert_sql_zero "orphan pool transactions" \
  "SELECT count(*) FROM pool_events e LEFT JOIN transactions t USING (signature) WHERE t.signature IS NULL"

# Event slot must match the parent transaction slot.
assert_sql_zero "swap slot mismatches" \
  "SELECT count(*) FROM swap_events e JOIN transactions t USING (signature) WHERE e.slot <> t.slot"
assert_sql_zero "trade slot mismatches" \
  "SELECT count(*) FROM trade_events e JOIN transactions t USING (signature) WHERE e.slot <> t.slot"
assert_sql_zero "launch slot mismatches" \
  "SELECT count(*) FROM launch_events e JOIN transactions t USING (signature) WHERE e.slot <> t.slot"
assert_sql_zero "liquidity slot mismatches" \
  "SELECT count(*) FROM liquidity_events e JOIN transactions t USING (signature) WHERE e.slot <> t.slot"
assert_sql_zero "pool slot mismatches" \
  "SELECT count(*) FROM pool_events e JOIN transactions t USING (signature) WHERE e.slot <> t.slot"

# Every stored row gets chain time when metadata for its slot supplies it.
for table in transactions swap_events trade_events launch_events liquidity_events pool_events; do
  assert_sql_zero "$table block times match available block metadata" \
    "SELECT count(*) FROM $table e JOIN block_meta b USING (slot) WHERE b.block_time IS NOT NULL AND e.block_time IS DISTINCT FROM b.block_time"
done

pass "data invariants"
