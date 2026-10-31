#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lib.sh"
section "11 graceful shutdown"
python3 "$AI_TEST_DIR/tests/graceful_shutdown.py"
pass "graceful shutdown"
