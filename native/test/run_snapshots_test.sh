#!/usr/bin/env bash
# Runs the career snapshots test on the programs native/test/manifest.py builds for it into the folder given as $1.
# Run it as native/test/run_tests.sh snapshots, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh snapshots
OUT=$1
"$OUT/snapshots_test" --fixtures ../../packages/sdk/std/fixtures
