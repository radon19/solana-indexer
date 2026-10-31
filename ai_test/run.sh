#!/usr/bin/env bash
# AI-generated black-box test runner for the Solana indexer.
# The application reads DATABASE_URL; the harness exports it for the
# disposable test database. Runtime tests build the debug `replay` feature;
# the default production build streams live.

set -uo pipefail

AI_TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$AI_TEST_DIR/.." && pwd)"
export AI_TEST_DIR ROOT

# shellcheck disable=SC1091
source "$AI_TEST_DIR/lib.sh"

ARTIFACTS="$AI_TEST_DIR/artifacts/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$ARTIFACTS"
export ARTIFACTS

CAPTURE="${AI_TEST_CAPTURE:-$ROOT/capture.bin}"
export CAPTURE

RUNTIME=1
if [[ "${AI_TEST_SKIP_RUNTIME:-0}" == "1" ]]; then
  RUNTIME=0
fi
export AI_TEST_RUNTIME="$RUNTIME"

# Do not let a caller's replay loop turn a finite test run into a hang.
unset REPLAY_LOOP
unset REPLAY_DLQ
unset REPLAY
unset PRINT_EVENTS

PGHOST="${AI_TEST_PGHOST:-127.0.0.1}"
PGUSER="${AI_TEST_PGUSER:-indexer}"
PGPASSWORD="${AI_TEST_PGPASSWORD:-indexer}"
PGDATABASE="${AI_TEST_PGDATABASE:-solana_indexer}"
export PGHOST PGUSER PGPASSWORD PGDATABASE

port_is_free() {
  local port="$1"
  if timeout 1 bash -c "exec 3<>/dev/tcp/127.0.0.1/$port" 2>/dev/null; then
    return 1
  fi
  return 0
}

choose_port() {
  local candidate
  if [[ -n "${AI_TEST_DB_PORT:-}" ]]; then
    printf '%s\n' "$AI_TEST_DB_PORT"
    return 0
  fi
  if port_is_free 5432; then
    printf '5432\n'
    return 0
  fi
  for candidate in $(seq 55432 55448); do
    if port_is_free "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  fail "no free test port found (tried 5432 and 55432..55448)"
}

build_repo() {
  local target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
  if ! (cd "$ROOT" && CARGO_TARGET_DIR="$target_dir" cargo build --locked --features replay --bin solana-indexer) >"$ARTIFACTS/build.log" 2>&1; then
    tail -n 40 "$ARTIFACTS/build.log" >&2 || true
    fail "cargo build failed (log: $ARTIFACTS/build.log)"
  fi
  APP_BIN="$target_dir/debug/solana-indexer"
  export APP_BIN
  pass "built active tree: $APP_BIN"
}

wait_for_db() {
  local i
  for i in $(seq 1 60); do
    if PGPASSWORD="$PGPASSWORD" psql -X -q -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -c 'SELECT 1' >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

CONTAINER=""
CONTAINER="ai-test-postgres-$$"
export CONTAINER
cleanup() {
  if [[ -n "${CONTAINER:-}" ]]; then
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

FAILED=0
run_test() {
  local path="$1"
  local name log
  name="$(basename "$path" .sh)"
  log="$ARTIFACTS/$name.log"
  printf '\n--- %s ---\n' "$name"
  if bash "$path" 2>&1 | tee "$log"; then
    pass "$name"
  else
    printf 'FAIL: %s (log: %s)\n' "$name" "$log" >&2
    FAILED=$((FAILED + 1))
  fi
}

printf 'AI test suite — %s\n' "$(date -Is)"
printf 'repo:      %s\n' "$ROOT"
printf 'artifacts: %s\n' "$ARTIFACTS"
printf 'runtime:   %s\n' "$RUNTIME"

# Static preflight and fast tests.
run_test "$AI_TEST_DIR/tests/00_preflight.sh"
run_test "$AI_TEST_DIR/tests/01_static.sh"
run_test "$AI_TEST_DIR/tests/02_unit.sh"

if [[ "$RUNTIME" == "1" ]]; then
  if [[ ! -s "$CAPTURE" ]]; then
    fail "capture file missing or empty: $CAPTURE (set AI_TEST_CAPTURE or use AI_TEST_SKIP_RUNTIME=1)"
  fi

  PGPORT="$(choose_port)"
  export PGPORT
  if ! port_is_free "$PGPORT"; then
    fail "test port $PGPORT is already in use; refusing to touch an existing database"
  fi
  printf 'test db:   postgres:18 on 127.0.0.1:%s\n' "$PGPORT"

  DATABASE_URL="postgres://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/${PGDATABASE}"
  export DATABASE_URL
  printf 'database:  %s:%s/%s\n' "$PGHOST" "$PGPORT" "$PGDATABASE"

  build_repo

  if ! docker image inspect postgres:18 >/dev/null 2>&1; then
    if ! docker pull postgres:18 >"$ARTIFACTS/docker-pull.log" 2>&1; then
      tail -n 30 "$ARTIFACTS/docker-pull.log" >&2 || true
      fail "cannot pull postgres:18"
    fi
  fi

  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  if ! docker run -d --rm --name "$CONTAINER" \
    -e POSTGRES_USER="$PGUSER" \
    -e POSTGRES_PASSWORD="$PGPASSWORD" \
    -e POSTGRES_DB="$PGDATABASE" \
    -p "127.0.0.1:${PGPORT}:5432" \
    postgres:18 >"$ARTIFACTS/docker-run.log" 2>&1; then
    cat "$ARTIFACTS/docker-run.log" >&2 || true
    fail "cannot start test postgres"
  fi

  if ! wait_for_db; then
    docker logs "$CONTAINER" >"$ARTIFACTS/docker.log" 2>&1 || true
    tail -n 40 "$ARTIFACTS/docker.log" >&2 || true
    fail "test postgres did not become ready on port $PGPORT"
  fi
  pass "test postgres ready on 127.0.0.1:$PGPORT"

  run_test "$AI_TEST_DIR/tests/03_schema.sh"
  run_test "$AI_TEST_DIR/tests/04_replay.sh"
  run_test "$AI_TEST_DIR/tests/05_idempotency.sh"
  run_test "$AI_TEST_DIR/tests/06_invariants.sh"
  run_test "$AI_TEST_DIR/tests/07_dlq_replay.sh"
  run_test "$AI_TEST_DIR/tests/08_performance.sh"
  run_test "$AI_TEST_DIR/tests/10_quality_advisory.sh"
  run_test "$AI_TEST_DIR/tests/11_graceful_shutdown.sh"
  run_test "$AI_TEST_DIR/tests/09_failure_modes.sh"
else
  printf '\nRuntime tests skipped (AI_TEST_SKIP_RUNTIME=1).\n'
  run_test "$AI_TEST_DIR/tests/10_quality_advisory.sh"
fi

printf '\n==============================\n'
if [[ -s "$ARTIFACTS/known_bugs.txt" ]]; then
  printf 'Known bugs reported: %s\n' "$(wc -l <"$ARTIFACTS/known_bugs.txt")"
  sed 's/^/  /' "$ARTIFACTS/known_bugs.txt"
fi
if [[ -s "$ARTIFACTS/advisories.txt" ]]; then
  printf 'Advisories: %s\n' "$(wc -l <"$ARTIFACTS/advisories.txt")"
  sed 's/^/  /' "$ARTIFACTS/advisories.txt"
fi
if (( FAILED > 0 )); then
  printf 'Unexpected failures: %d\n' "$FAILED"
  printf 'Artifacts: %s\n' "$ARTIFACTS"
  exit 1
fi
if [[ "${AI_TEST_STRICT:-0}" == "1" && -s "$ARTIFACTS/known_bugs.txt" ]]; then
  printf 'Strict mode: known bugs are fatal.\n'
  printf 'Artifacts: %s\n' "$ARTIFACTS"
  exit 2
fi
printf 'PASS: all executable tests succeeded.\n'
printf 'Artifacts: %s\n' "$ARTIFACTS"
