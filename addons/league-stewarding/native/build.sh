#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"

if command -v zig >/dev/null; then
    ZIG=(zig)
else
    ZIG=(nix run nixpkgs#zig --)
fi

export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache
mkdir -p build

REPO=../../..
SDK=$REPO/packages/sdk/include
GAME=${GAME_INCLUDE:-$REPO/native/include}

python3 tools/embed_web.py ../web build/web_assets.hpp

TELEMETRY=src/telemetry.cpp
[ -f "$TELEMETRY" ] || TELEMETRY=src/telemetry_stub.cpp

"${ZIG[@]}" c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -Wno-nullability-completeness \
    -I"$GAME" -I$SDK -Isrc -Ibuild -D__USE_MINGW_ANSI_STDIO=1 -shared -s -o steward.dll \
    src/steward.cpp src/server.cpp "$TELEMETRY" -lws2_32 -lbcrypt -lshell32

rm -f ./*.lib ./*.pdb
echo "built $(pwd)/steward.dll"
