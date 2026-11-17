#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/hud-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../../packages/sdk/include -o "$OUT/hud_test" hud_test.cpp
"$OUT/hud_test" "$OUT/capture-test.png" "$OUT"
if command -v python3 >/dev/null; then
    python3 check_png.py "$OUT/capture-test.png"
fi
