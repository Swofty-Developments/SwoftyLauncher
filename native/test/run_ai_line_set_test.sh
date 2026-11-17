#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/ai-line-set-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -o "$OUT/ai_line_set_test" ai_line_set_test.cpp
"$OUT/ai_line_set_test"
