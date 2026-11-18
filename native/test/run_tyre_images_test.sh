#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
if ! command -v zig >/dev/null || ! command -v wine >/dev/null || ! command -v xvfb-run >/dev/null; then
    exec nix shell nixpkgs#zig nixpkgs#wineWow64Packages.stable nixpkgs#xvfb-run -c "$SELF" "$@"
fi
OUT=${1:-$PWD/../build/tyre-image-test}
mkdir -p "$OUT"
ZS=../third_party/zstd
OBJECTS=()
for c in "$ZS"/common/*.c "$ZS"/decompress/*.c; do
    o="$OUT/$(basename "${c%.c}").o"
    zig cc -target x86_64-windows-gnu -O2 -DZSTD_LEGACY_SUPPORT=0 -DZSTD_DISABLE_ASM -I"$ZS" -c "$c" -o "$o"
    OBJECTS+=("$o")
done
FLAGS=(-target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -fms-extensions -I../../packages/sdk/include -I../include -I"$ZS")
zig c++ "${FLAGS[@]}" tyre_images_test.cpp "${OBJECTS[@]}" -o "$OUT/tyre_images_test.exe"
zig c++ "${FLAGS[@]}" ../src/core/badge_images.cpp badge_images_test.cpp "${OBJECTS[@]}" -ld3d12 -ldxgi -o "$OUT/badge_images_test.exe"
xvfb-run -a wine "$OUT/tyre_images_test.exe"
xvfb-run -a wine "$OUT/badge_images_test.exe" --tyres
