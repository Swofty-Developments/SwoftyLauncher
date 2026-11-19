#!/usr/bin/env bash
# Builds pe_imports_test.cpp for the host and runs it, compiles native/include/pe_imports.h as C the way the loader
# does, then builds the test for Windows with the core's shared page table (src/core/mem_pages.cpp,
# SWOFTY_MEM_SHARED) as readable() and runs it under Wine on Linux, natively on Windows (Git Bash).
set -uo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) NATIVE=1 ;;
*) NATIVE=0 ;;
esac
if ! command -v zig >/dev/null || { [[ $NATIVE == 0 ]] && ! command -v wine >/dev/null; }; then
    exec nix shell nixpkgs#zig nixpkgs#wineWow64Packages.stable -c "$SELF" "$@"
fi
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/pe-imports-test}
SDK=../../packages/sdk/include
mkdir -p "$OUT"
if [[ $NATIVE == 1 ]]; then
    PIN=()
    RUN=()
else
    PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
    RUN=(wine)
    export WINEPREFIX=${WINEPREFIX:-$OUT/prefix} WINEDEBUG=-all
fi
r=0
if [[ $NATIVE == 0 ]]; then
    "${PIN[@]}" zig c++ -target x86_64-linux-musl -std=c++23 -O2 -Wall -Wextra -Werror -I../include \
        -o "$OUT/pe_imports_test" pe_imports_test.cpp || exit 1
    "${PIN[@]}" "$OUT/pe_imports_test"
    r=$?
    [[ $r -eq 0 ]] || printf 'pe imports host test: exit %d\n' "$r"
fi
printf '#include "pe_imports.h"\n' | "${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -Wall -Wextra -Werror \
    -I../include -x c -c -o "$OUT/pe_imports_c.o" - || exit 1
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -I../include -I$SDK -DSWOFTY_MEM_SHARED \
    -o "$OUT/pe_imports_test.exe" pe_imports_test.cpp ../src/core/mem_pages.cpp || exit 1
[[ $NATIVE == 1 ]] || timeout 300 wineboot -i >/dev/null 2>&1
timeout 120 "${PIN[@]}" "${RUN[@]}" "$OUT/pe_imports_test.exe" 2>/dev/null
w=$?
[[ $w -eq 0 ]] || printf 'pe imports Windows test: exit %d (0x%08x)\n' "$w" "$((w & 0xffffffff))"
[[ $r -ne 0 ]] || r=$w
exit $r
