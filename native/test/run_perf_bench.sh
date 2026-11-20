#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
. "$(dirname "$SELF")/lib/env.sh"
OUT=${PERF_OUT:-$PWD/../build/perf-bench}
WINE=${WINE:-wine}
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
SH=../third_party/safetyhook
mkdir -p "$OUT"
[[ $OUT/Zydis.o -nt $SH/Zydis.c ]] || "${PIN[@]}" zig cc -target x86_64-windows-gnu -I../src -O2 -I$SH -c $SH/Zydis.c -o "$OUT/Zydis.o" &
[[ $OUT/safetyhook.o -nt $SH/safetyhook.cpp ]] ||
    "${PIN[@]}" zig c++ -target x86_64-windows-gnu -I../src -std=c++23 -O2 -I$SH -c $SH/safetyhook.cpp -o "$OUT/safetyhook.o" &
wait
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -I../src -std=c++23 -O2 -Wno-nullability-completeness -I../../packages/sdk/include \
    -DSWOFTY_MEM_SHARED -I../include -I$SH -o "$OUT/perf_bench.exe" perf_bench.cpp ../src/core/diagnostics/perf.cpp ../src/core/diagnostics/perf/*.cpp ../src/core/runtime/mem_pages.cpp "$OUT/safetyhook.o" "$OUT/Zydis.o" -s
cd "$OUT"
export WINEPREFIX=${WINEPREFIX:-$OUT/prefix} WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=;winemenubuilder.exe=d"
"${PIN[@]}" $WINE ./perf_bench.exe "$@"
