#!/usr/bin/env bash
# The career list (career_saves_logic.hpp apply_list: added, deleted-in-game, missing, unchanged, back) built for Windows and run under
# Wine, the way the core runs it. CI runs this; it sources game_lock.sh.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
if ! command -v wine >/dev/null || ! command -v xvfb-run >/dev/null || ! command -v zig >/dev/null; then
    exec nix shell nixpkgs#zig nixpkgs#wineWow64Packages.stable nixpkgs#xvfb-run -c "$SELF" "$@"
fi
. ./game_lock.sh
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=$(realpath -m "${1:-$PWD/../build/career-list-test}")
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
mkdir -p "$OUT"
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -Wno-nullability-completeness \
    -Wno-missing-designated-field-initializers -I../../packages/sdk/include -o "$OUT/career_saves_test.exe" career_saves_test.cpp
export WINEPREFIX="$OUT/prefix" WINEDLLOVERRIDES="mscoree,mshtml=;winemenubuilder.exe,winedbg.exe=d" WINEDEBUG=-all
timeout 300 wineboot -i >/dev/null 2>&1 || true
timeout 120 "${PIN[@]}" wine "$OUT/career_saves_test.exe" 2>/dev/null | tee "$OUT/career-saves.txt"
grep -q "^career saves: 0 failed" "$OUT/career-saves.txt"
