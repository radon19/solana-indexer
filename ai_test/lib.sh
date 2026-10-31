#!/usr/bin/env bash
# Shared helpers for the AI test suite. This file is sourced by the test
# scripts and by run.sh. It deliberately does not enable `set -e`: each test
# owns its failure policy.

AI_TEST_DIR="${AI_TEST_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
ROOT="${ROOT:-$(cd "$AI_TEST_DIR/.." && pwd)}"
CAPTURE="${AI_TEST_CAPTURE:-$ROOT/capture.bin}"
export AI_TEST_DIR ROOT CAPTURE

if [[ -f "$AI_TEST_DIR/config.env" ]]; then
  # shellcheck disable=SC1091
  source "$AI_TEST_DIR/config.env"
fi

ARTIFACTS="${ARTIFACTS:-$AI_TEST_DIR/artifacts/manual}"
mkdir -p "$ARTIFACTS"
export ARTIFACTS

section() {
  printf '\n== %s ==\n' "$*"
}

pass() {
  printf 'PASS: %s\n' "$*"
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'WARN: %s\n' "$*"
  printf '%s\n' "$*" >>"$ARTIFACTS/advisories.txt"
}

known_bug() {
  local id="$1"
  local message="$2"
  printf 'KNOWN BUG [%s] %s\n' "$id" "$message"
  printf '[%s] %s\n' "$id" "$message" >>"$ARTIFACTS/known_bugs.txt"
}

_psql() {
  PGPASSWORD="${PGPASSWORD:-indexer}" psql -X -q \
    -h "${PGHOST:-127.0.0.1}" \
    -p "${PGPORT:?PGPORT is not set}" \
    -U "${PGUSER:-indexer}" \
    -d "${PGDATABASE:-solana_indexer}" \
    -v ON_ERROR_STOP=1 "$@"
}

psql_val() {
  _psql -At -c "$1" | tr -d '\r'
}

psql_cmd() {
  _psql -c "$1"
}

assert_eq() {
  local label="$1"
  local actual="$2"
  local expected="$3"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label: expected [$expected], got [$actual]"
  fi
  pass "$label = $actual"
}

assert_ge() {
  local label="$1"
  local actual="$2"
  local minimum="$3"
  if [[ ! "$actual" =~ ^[0-9]+$ ]] || (( actual < minimum )); then
    fail "$label: expected >= $minimum, got [$actual]"
  fi
  pass "$label = $actual (>= $minimum)"
}

assert_sql_eq() {
  local label="$1"
  local query="$2"
  local expected="$3"
  local actual
  actual="$(psql_val "$query")"
  assert_eq "$label" "$actual" "$expected"
}

assert_sql_zero() {
  local label="$1"
  local query="$2"
  assert_sql_eq "$label" "$query" "0"
}

assert_file_exists() {
  local path="$1"
  [[ -f "$path" ]] || fail "missing file: $path"
  pass "file exists: $path"
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  if ! grep -Fq -- "$needle" "$path"; then
    fail "expected '$needle' in $path"
  fi
  pass "found '$needle'"
}

assert_file_not_contains() {
  local path="$1"
  local needle="$2"
  if grep -Fq -- "$needle" "$path"; then
    fail "unexpected '$needle' in $path"
  fi
  pass "did not find '$needle'"
}

run_app() {
  local log="$1"
  shift
  if [[ -z "${DATABASE_URL:-}" ]]; then
    DATABASE_URL="postgres://${PGUSER:-indexer}:${PGPASSWORD:-indexer}@${PGHOST:-127.0.0.1}:${PGPORT:?PGPORT is not set}/${PGDATABASE:-solana_indexer}"
    export DATABASE_URL
  fi
  local rc=0
  timeout "${AI_TEST_APP_TIMEOUT:-180}" "$APP_BIN" "$@" >"$log" 2>&1 || rc=$?
  APP_RC="$rc"
  export APP_RC
}

assert_app_ok() {
  local label="$1"
  local log="$2"
  if [[ "${APP_RC:-1}" -ne 0 ]]; then
    tail -n 30 "$log" >&2 || true
    fail "$label: app exited ${APP_RC:-?} (log: $log)"
  fi
  pass "$label: exit 0"
}

assert_app_failed() {
  local label="$1"
  local log="$2"
  if [[ "${APP_RC:-0}" -eq 0 ]]; then
    fail "$label: expected non-zero exit (log: $log)"
  fi
  pass "$label: exit ${APP_RC}"
}

table_counts() {
  psql_val "
    SELECT 'transactions', count(*) FROM transactions
    UNION ALL SELECT 'slots', count(*) FROM slots
    UNION ALL SELECT 'block_meta', count(*) FROM block_meta
    UNION ALL SELECT 'accounts', count(*) FROM accounts
    UNION ALL SELECT 'swap_events', count(*) FROM swap_events
    UNION ALL SELECT 'route_hops', count(*) FROM route_hops
    UNION ALL SELECT 'trade_events', count(*) FROM trade_events
    UNION ALL SELECT 'launch_events', count(*) FROM launch_events
    UNION ALL SELECT 'liquidity_events', count(*) FROM liquidity_events
    UNION ALL SELECT 'pool_events', count(*) FROM pool_events
    ORDER BY 1"
}

assert_expected_counts() {
  assert_sql_eq "transactions count" "SELECT count(*) FROM transactions" "$EXPECTED_TRANSACTIONS"
  assert_sql_eq "slots count" "SELECT count(*) FROM slots" "$EXPECTED_SLOTS"
  assert_sql_eq "block_meta count" "SELECT count(*) FROM block_meta" "$EXPECTED_BLOCK_META"
  assert_sql_eq "accounts count" "SELECT count(*) FROM accounts" "$EXPECTED_ACCOUNTS"
  assert_sql_eq "swap_events count" "SELECT count(*) FROM swap_events" "$EXPECTED_SWAPS"
  assert_sql_eq "route_hops count" "SELECT count(*) FROM route_hops" "$EXPECTED_HOPS"
  assert_sql_eq "trade_events count" "SELECT count(*) FROM trade_events" "$EXPECTED_TRADES"
  assert_sql_eq "launch_events count" "SELECT count(*) FROM launch_events" "$EXPECTED_LAUNCHES"
  assert_sql_eq "liquidity_events count" "SELECT count(*) FROM liquidity_events" "$EXPECTED_LIQUIDITY"
  assert_sql_eq "pool_events count" "SELECT count(*) FROM pool_events" "$EXPECTED_POOLS"
}

clear_dlq() {
  psql_cmd "DELETE FROM dead_letter_batches;" >/dev/null
}

insert_dlq_payload() {
  local json_file="$1"
  local slot="$2"
  local attempts="${3:-5}"
  local json
  json="$(cat "$json_file")"
  local escaped="${json//\'/\'\'}"
  psql_val "INSERT INTO dead_letter_batches
    (attempts, row_count, sqlstate, error, payload_format, payload)
    VALUES ($attempts, 1, NULL, 'ai-test synthetic DLQ row', 'json-v2', convert_to('$escaped', 'UTF8'))
    RETURNING id"
}

cleanup_test_slot() {
  local slot="$1"
  psql_cmd "DELETE FROM slots WHERE slot = $slot;" >/dev/null
}
