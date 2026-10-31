#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "03 schema and migrations"

export REPLAY="$AI_TEST_DIR/fixtures/empty.bin"
unset REPLAY_DLQ
run_app "$ARTIFACTS/migrate.log"
assert_app_ok "empty replay applies migrations" "$ARTIFACTS/migrate.log"
assert_file_contains "$ARTIFACTS/migrate.log" "REPLAY complete"

assert_sql_eq "successful migrations" \
  "SELECT count(*) FROM _sqlx_migrations WHERE success" "1"
assert_sql_eq "migration version" \
  "SELECT version FROM _sqlx_migrations ORDER BY version LIMIT 1" "1"

tables=(
  slots
  block_meta
  transactions
  accounts
  swap_events
  route_hops
  trade_events
  launch_events
  liquidity_events
  pool_events
  dead_letter_batches
  resume
)
for table in "${tables[@]}"; do
  assert_sql_eq "table $table exists" \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='$table'" "1"
done

pk_columns() {
  psql_val "
    SELECT string_agg(a.attname, ',' ORDER BY k.ord)
    FROM pg_index i
    JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord) ON true
    JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
    WHERE i.indrelid = 'public.$1'::regclass AND i.indisprimary"
}

assert_eq "slots primary key" "$(pk_columns slots)" "slot"
assert_eq "block_meta primary key" "$(pk_columns block_meta)" "slot"
assert_eq "transactions primary key" "$(pk_columns transactions)" "signature"
assert_eq "accounts primary key" "$(pk_columns accounts)" "pubkey"
assert_eq "swap_events primary key" "$(pk_columns swap_events)" "slot,signature,ix_index,parent_ix_index,is_inner"
assert_eq "route_hops primary key" "$(pk_columns route_hops)" "slot,signature,ix_index,parent_ix_index,is_inner,hop_index"
assert_eq "trade_events primary key" "$(pk_columns trade_events)" "slot,signature,ix_index,parent_ix_index,is_inner"
assert_eq "launch_events primary key" "$(pk_columns launch_events)" "slot,signature,ix_index,parent_ix_index,is_inner"
assert_eq "liquidity_events primary key" "$(pk_columns liquidity_events)" "slot,signature,ix_index,parent_ix_index,is_inner,kind"
assert_eq "pool_events primary key" "$(pk_columns pool_events)" "slot,signature,ix_index,parent_ix_index,is_inner"

assert_sql_eq "route_hops -> swap_events FK with cascade" \
  "SELECT count(*) FROM pg_constraint WHERE conrelid='public.route_hops'::regclass AND contype='f' AND confrelid='public.swap_events'::regclass AND confdeltype='c'" "1"

assert_sql_eq "trade side check constraint" \
  "SELECT count(*) FROM pg_constraint WHERE conrelid='public.trade_events'::regclass AND contype='c' AND conname='trade_events_side_check'" "1"
assert_sql_eq "liquidity kind check constraint" \
  "SELECT count(*) FROM pg_constraint WHERE conrelid='public.liquidity_events'::regclass AND contype='c' AND conname='liquidity_events_kind_check'" "1"

assert_sql_eq "unsigned monetary columns use numeric(20,0)" \
  "SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND data_type='numeric' AND numeric_precision=20 AND numeric_scale=0 AND (table_name,column_name) IN (('accounts','lamports'),('transactions','fee'),('swap_events','input_amount'),('swap_events','output_amount'),('route_hops','in_amount'),('route_hops','out_amount'),('trade_events','token_amount'),('trade_events','quote_amount'),('liquidity_events','token_a_amount'),('liquidity_events','token_b_amount'))" "10"

assert_sql_eq "dead_letter_batches expected columns" \
  "SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='dead_letter_batches' AND column_name IN ('id','created_at','attempts','row_count','sqlstate','error','payload_format','payload','replayed_at','replay_error')" "10"

assert_sql_eq "amount range constraints are validated" \
  "SELECT count(*) FROM pg_constraint WHERE connamespace='public'::regnamespace AND conname IN ('accounts_lamports_u64','transactions_fee_u64','swap_events_amounts_u64','route_hops_amounts_u64','trade_events_amounts_u64','liquidity_events_amounts_u64') AND convalidated" "6"

assert_sql_eq "DLQ format defaults to json-v2" \
  "SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='dead_letter_batches' AND column_name='payload_format' AND column_default LIKE '%json-v2%'" "1"

assert_sql_eq "empty schema has no slots" "SELECT count(*) FROM slots" "0"
assert_sql_eq "empty schema has no DLQ rows" "SELECT count(*) FROM dead_letter_batches" "0"

pass "schema and migrations"

assert_sql_eq "offline replay never advances resume" "SELECT count(*) FROM resume" "0"
