#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/heap-sizes-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -o "$OUT/heap_sizes_test" heap_sizes_test.cpp
"$OUT/heap_sizes_test"
