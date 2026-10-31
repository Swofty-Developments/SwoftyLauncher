#!/usr/bin/env bash
# Host tests for the box track lists (native/src/core/apply/tracks/{facts_file,fits,list,per_package,fuel_plan}.hpp):
# fits_test, list_test and per_package_test, plus a Windows compile of the runtime files that use them.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
if ! command -v zig >/dev/null; then
    exec nix shell nixpkgs#zig -c "$SELF" "$@"
fi
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/track-lists-test}
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
mkdir -p "$OUT"
FLAGS=(-std=c++23 -O1 -Wall -Wextra -Wno-nullability-completeness -I../../packages/sdk/include)
for t in fits list per_package; do
    "${PIN[@]}" zig c++ -target x86_64-linux-musl "${FLAGS[@]}" -o "$OUT/${t}_test" "tracks/${t}_test.cpp"
done
for f in list per_package data_session; do
    "${PIN[@]}" zig c++ -target x86_64-windows-gnu "${FLAGS[@]}" -I../src/core -I../include \
        -c "../src/core/apply/tracks/$f.cpp" -o "$OUT/$f.windows.o"
done
for t in fits list per_package; do
    "$OUT/${t}_test"
done
