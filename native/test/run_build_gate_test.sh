#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
. "$(dirname "$SELF")/lib/env.sh"
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/build-gate-test}
mkdir -p "$OUT"
python3 ../tools/supported_builds.py ../../packages/core/src/games/f1_25/build_support.rs "$OUT/supported_builds.h"
"${PIN[@]}" zig cc -target x86_64-linux-musl -O2 -Wall -Wextra -Werror -I"$OUT" -I../include -o "$OUT/build_gate_test" build_gate_test.c
"${PIN[@]}" "$OUT/build_gate_test"
