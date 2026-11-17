#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/ui-registry-walk-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../../packages/sdk/include -o "$OUT/ui_registry_walk_test" ui_registry_walk_test.cpp
"$OUT/ui_registry_walk_test"
