#!/usr/bin/env bash
# Runs the swofty::json test on the programs native/test/manifest.py builds for it into the folder given as $1. Run it
# as native/test/run_tests.sh json, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh json
OUT=$1
"$OUT/json_test" "${JSON_FUZZ_ROUNDS:-20000}"
# A sanitizer build runs the same cases and a longer fuzz loop where the host compiler supports it.
if [ -x "$OUT/json_test_san" ]; then
    "$OUT/json_test_san" "${JSON_SAN_FUZZ_ROUNDS:-100000}"
else
    echo "json: no host compiler with -fsanitize=address,undefined; skipped the sanitizer build"
fi
