#!/usr/bin/env bash
# Runs the live apply paths test on the programs native/test/manifest.py builds for it into the folder given as $1.
# Run it as native/test/run_tests.sh apply, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh apply
OUT=$1
mkdir -p "$OUT/reports"
"$OUT/apply_test" "$OUT/reports"
"$OUT/files_test"
