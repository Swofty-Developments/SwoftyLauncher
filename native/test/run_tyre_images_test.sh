#!/usr/bin/env bash
# The Live image service's tyre catalogues, mod routes and archive worker: tyre_images_test.cpp, then the tyre cases of
# badge_images_test.cpp against src/core/render/badges/badge_images.cpp, both built for Windows and run under Wine.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
NEEDS="zig wine xvfb-run"
. "$(dirname "$SELF")/lib/env.sh"
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/tyre-image-test}
mkdir -p "$OUT"
export WINEPREFIX=${WINEPREFIX:-$OUT/prefix} WINEDEBUG=-all
ZS=../third_party/zstd
OBJECTS=()
for c in "$ZS"/common/*.c "$ZS"/decompress/*.c; do
    o="$OUT/$(basename "${c%.c}").o"
    "${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -DZSTD_LEGACY_SUPPORT=0 -DZSTD_DISABLE_ASM -I"$ZS" -c "$c" -o "$o"
    OBJECTS+=("$o")
done
FLAGS=(-target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -fms-extensions -I../src -I../../packages/sdk/include
    -I../include -I"$ZS")
"${PIN[@]}" zig c++ "${FLAGS[@]}" tyre_images_test.cpp "${OBJECTS[@]}" -o "$OUT/tyre_images_test.exe"
"${PIN[@]}" zig c++ "${FLAGS[@]}" ../src/core/render/badges/badge_images.cpp badge_images_test.cpp "${OBJECTS[@]}" \
    -ld3d12 -ldxgi -o "$OUT/badge_images_test.exe"
timeout 300 "${PIN[@]}" xvfb-run -a wine "$OUT/tyre_images_test.exe"
timeout 300 "${PIN[@]}" xvfb-run -a wine "$OUT/badge_images_test.exe" --tyres
