#!/usr/bin/env bash
# Compiles native/include/pe_imports.h as C the way the loader does, then builds pe_imports_test.cpp for Windows with
# the core's shared page table (src/core/runtime/mem_pages.cpp, SWOFTY_MEM_SHARED) as readable() and runs it under
# Wine on Linux, natively on Windows (Git Bash). The host build of the test is the pe_imports unit (manifest.py).
set -uo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) NATIVE=1 ;;
*) NATIVE=0 ;;
esac
NEEDS=zig
[[ $NATIVE == 0 ]] && NEEDS="zig wine"
. "$(dirname "$SELF")/lib/env.sh"
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/pe-imports-test}
SDK=../../packages/sdk/include
mkdir -p "$OUT"
if [[ $NATIVE == 1 ]]; then
    PIN=()
    RUN=()
else
    RUN=(wine)
    export WINEPREFIX=${WINEPREFIX:-$OUT/prefix} WINEDEBUG=-all
fi
printf '#include "pe_imports.h"\n' | "${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -Wall -Wextra -Werror \
    -I../include -x c -c -o "$OUT/pe_imports_c.o" - || exit 1
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -I../src -std=c++23 -O2 -Wall -Wextra -I../include -I$SDK -DSWOFTY_MEM_SHARED \
    -o "$OUT/pe_imports_test.exe" pe_imports_test.cpp ../src/core/runtime/mem_pages.cpp || exit 1
[[ $NATIVE == 1 ]] || timeout 300 wineboot -i >/dev/null 2>&1
timeout 120 "${PIN[@]}" "${RUN[@]}" "$OUT/pe_imports_test.exe" 2>/dev/null
r=$?
[[ $r -eq 0 ]] || printf 'pe imports Windows test: exit %d (0x%08x)\n' "$r" "$((r & 0xffffffff))"
exit $r
