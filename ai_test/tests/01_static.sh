#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "01 static checks"

check_log="$ARTIFACTS/cargo-check.log"
if ! (cd "$ROOT" && cargo check --locked) >"$check_log" 2>&1; then
  tail -n 50 "$check_log" >&2 || true
  fail "cargo check --locked failed"
fi
pass "cargo check --locked"

warning_lines="$(grep -c '^warning:' "$check_log" || true)"
if [[ "${warning_lines:-0}" -gt 0 ]]; then
  warn "cargo check emitted $warning_lines warning line(s); see $check_log"
else
  pass "cargo check has no warning lines"
fi

fmt_log="$ARTIFACTS/cargo-fmt.log"
if (cd "$ROOT" && cargo fmt --all -- --check) >"$fmt_log" 2>&1; then
  pass "cargo fmt --check"
else
  warn "cargo fmt --all -- --check reports formatting differences; see $fmt_log"
fi

if grep -RInE 'unimplemented!|todo!\(' "$ROOT/src" >"$ARTIFACTS/unimplemented.log" 2>&1; then
  warn "unimplemented!/todo! markers found in src; see $ARTIFACTS/unimplemented.log"
else
  pass "no unimplemented!/todo! markers in src"
fi

pass "static checks"
