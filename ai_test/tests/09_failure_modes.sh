#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "09 failure modes"

# Missing replay file: the app reports the error and exits non-zero.
export REPLAY="$AI_TEST_DIR/fixtures/does-not-exist.bin"
unset REPLAY_DLQ
run_app "$ARTIFACTS/missing-capture.log"
assert_file_contains "$ARTIFACTS/missing-capture.log" "REPLAY FAILED"
assert_app_failed "missing REPLAY file fails the process" "$ARTIFACTS/missing-capture.log"

# Corrupt frame: load() logs the decode error and returns Ok(partial), so a
# partial replay can look successful. Report the exit-status bug if present.
export REPLAY="$AI_TEST_DIR/fixtures/corrupt_frame.bin"
run_app "$ARTIFACTS/corrupt-capture.log"
assert_file_contains "$ARTIFACTS/corrupt-capture.log" "decode failed"
if [[ "${APP_RC:-1}" -eq 0 ]]; then
  known_bug "BUG-006" "corrupt capture exits 0 after a decode failure/partial replay"
else
  pass "corrupt capture exits non-zero"
fi

# Database unavailable. This test stops the disposable test container and must
# be the last runtime test.
if [[ -n "${CONTAINER:-}" ]]; then
  docker stop "$CONTAINER" >/dev/null 2>&1 || true
fi
for _ in $(seq 1 15); do
  if ! PGPASSWORD="$PGPASSWORD" psql -X -q -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -c 'SELECT 1' >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

export REPLAY="$AI_TEST_DIR/fixtures/empty.bin"
AI_TEST_APP_TIMEOUT=45
export AI_TEST_APP_TIMEOUT
run_app "$ARTIFACTS/db-down.log"
assert_app_failed "DB unavailable fails the process" "$ARTIFACTS/db-down.log"
if grep -Fq "panicked" "$ARTIFACTS/db-down.log"; then
  known_bug "BUG-004" "database connection failure waits for the pool timeout and then panics instead of failing fast/cleanly"
elif [[ "${APP_RC:-0}" -eq 124 ]]; then
  known_bug "BUG-004" "database connection failure did not exit within 45s"
else
  pass "DB unavailable exits non-zero without a panic"
fi

pass "failure modes"
