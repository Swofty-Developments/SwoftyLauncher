#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"

if ! command -v zig >/dev/null; then
    exec nix shell nixpkgs#zig -c "$SELF" "$@"
fi
mkdir -p build
export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache

NATIVE=../../native
SH=$NATIVE/third_party/safetyhook
. $NATIVE/tools/pin.sh
CC=(zig cc -target x86_64-windows-gnu)
CXX=(zig c++ -target x86_64-windows-gnu -std=c++23)

[[ build/Zydis.o -nt $SH/Zydis.c ]] || "${PIN[@]}" "${CC[@]}" -O2 -I$SH -c $SH/Zydis.c -o build/Zydis.o &
[[ build/safetyhook.o -nt $SH/safetyhook.cpp ]] || "${PIN[@]}" "${CXX[@]}" -O2 -I$SH -c $SH/safetyhook.cpp -o build/safetyhook.o &
wait

"${PIN[@]}" "${CXX[@]}" -O2 -Wall -Wextra -Wno-missing-field-initializers -I../../packages/sdk/include -I$SH -shared \
    -o build/swofty_trace.dll src/swofty_trace.cpp build/safetyhook.o build/Zydis.o -lbcrypt -s

objdump -p build/swofty_trace.dll | grep -E 'DLL Name|SwoftyModInit' || true
md5sum build/swofty_trace.dll
