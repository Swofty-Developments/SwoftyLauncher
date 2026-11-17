#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/myteam-vehicle-keep-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -I../src -o "$OUT/myteam_vehicle_keep_test" myteam_vehicle_keep_test.cpp
"$OUT/myteam_vehicle_keep_test"
