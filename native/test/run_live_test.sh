#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/live-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../include -I../../packages/sdk/include \
    -o "$OUT/live_test" live_test.cpp
"$OUT/live_test"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../include -I../../packages/sdk/include \
    -o "$OUT/live_gates_test" live_gates_test.cpp
"$OUT/live_gates_test"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../../packages/sdk/include -o "$OUT/live_sdk_test" live_sdk_test.cpp
"$OUT/live_sdk_test"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../../packages/sdk/include -o "$OUT/live_tyre_mapping_test" \
    live_tyre_mapping_test.cpp
"$OUT/live_tyre_mapping_test"
