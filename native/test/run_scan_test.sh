#!/usr/bin/env bash
# Runs the pattern scan test on the program native/test/manifest.py builds for it into the folder given as $1: with
# work/exe/F1_25.exe and a built native/dist/ it also checks every binding against that exe, which it only reads. Run
# it as native/test/run_tests.sh scan, which builds it first. It is a host test and takes no game lock.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh scan
OUT=$1
EXE=${SWOFTY_TEST_EXE:-$PWD/../../work/exe/F1_25.exe}
BINDINGS=${SWOFTY_TEST_BINDINGS:-$PWD/../dist/swofty/addons/core/bindings.json}
if [[ -f $EXE && -f $BINDINGS ]]; then
    "$OUT/scan_test" "$EXE" "$BINDINGS"
else
    echo "scan_test: no $EXE or $BINDINGS; the checks against the game exe are skipped"
    "$OUT/scan_test"
fi
