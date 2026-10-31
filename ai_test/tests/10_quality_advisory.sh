#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"

section "10 quality and repository advisories"

# BUG-002 regression: the database DSN must come from the environment and no
# credentials may remain in source.
if grep -qE '(std::)?env::var\("DATABASE_URL"\)' "$ROOT/src/database/db.rs"; then
  pass "database URL is read from DATABASE_URL"
else
  fail "database URL is not read from DATABASE_URL (BUG-002 regression)"
fi
if grep -nE 'postgres://[^" ]+:[^" ]+@' "$ROOT/src/database/db.rs" >"$ARTIFACTS/hardcoded-dsn.log" 2>&1; then
  fail "hardcoded database DSN remains in src/database/db.rs (BUG-002 regression)"
else
  pass "no hardcoded database DSN in src/database/db.rs"
fi

if grep -nE 'panic!\(' "$ROOT/src/main.rs" "$ROOT/src/connection/client.rs" >"$ARTIFACTS/panics.log" 2>&1; then
  warn "panic! macros found in startup/connection paths; see $ARTIFACTS/panics.log"
else
  pass "no startup/connection panic! macros"
fi

if [[ -f "$ROOT/docs/limitations.md" ]]; then
  pass "consolidated scope and limitations documentation exists"
else
  warn "docs/limitations.md is missing"
fi

if [[ -d "$ROOT/.github/workflows" ]]; then
  pass "CI workflow directory exists"
else
  warn "no .github/workflows CI configuration"
fi

if git -C "$ROOT" status --porcelain 2>/dev/null | grep -Eq '^\?\? (capture\.bin|solana-indexer\.zip)$'; then
  warn "large artifacts are untracked and not ignored: capture.bin and/or solana-indexer.zip"
else
  pass "no known large untracked artifacts in the checked status"
fi


if [[ -f "$ARTIFACTS/build.log" ]]; then
  build_warnings="$(grep -c '^warning:' "$ARTIFACTS/build.log" || true)"
  if [[ "${build_warnings:-0}" -gt 0 ]]; then
    warn "cargo build emitted $build_warnings warning line(s); see $ARTIFACTS/build.log"
  else
    pass "cargo build has no warning lines"
  fi
fi

if grep -RInE '#\[test\]|#\[tokio::test\]' "$ROOT/src/parsers" >"$ARTIFACTS/parser-tests.log" 2>&1; then
  pass "parser test annotations found"
else
  warn "no parser tests under src/parsers (the largest parser surface is untested)"
fi

pass "quality advisories"
