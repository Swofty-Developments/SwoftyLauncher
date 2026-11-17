#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/garage-placement-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -I../../packages/sdk/include \
    -o "$OUT/garage_placement_test" garage_placement_test.cpp
"$OUT/garage_placement_test"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -o "$OUT/garage_crew_pose_test" garage_crew_pose_test.cpp
"$OUT/garage_crew_pose_test"
