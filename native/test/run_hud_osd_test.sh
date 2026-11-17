#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/hud-osd-test}
mkdir -p "$OUT"
"$CXX" -I../src -std=c++23 -O2 -Wall -Wextra -I../../packages/sdk/include -o "$OUT/hud_osd_test" hud_osd_test.cpp
"$OUT/hud_osd_test"
