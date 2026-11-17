#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
. "$(dirname "$SELF")/lib/env.sh"
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/t145}
mkdir -p "$OUT"
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -Wno-nullability-completeness \
    -I../../packages/sdk/include -I../include -shared -s -o "$OUT/t145_probe.dll" ingame/t145_probe.cpp
echo "built $OUT/t145_probe.dll"
