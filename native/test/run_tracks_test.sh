#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
if ! command -v zig >/dev/null; then
    exec nix shell nixpkgs#zig -c "$SELF" "$@"
fi
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/tracks-test}
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
mkdir -p "$OUT"
python3 ../src/core/apply/tracks/track_keys.py --check
python3 ../src/core/apply/tracks/venues.py --check
SRC=(../src/core/registry/store.cpp ../src/core/registry/table.cpp ../src/core/registry/validate.cpp ../src/core/registry/schema.cpp
     ../src/core/registry/engine.cpp ../src/core/apply/ids.cpp)
pids=()
"${PIN[@]}" zig c++ -target x86_64-linux-musl -std=c++23 -O1 -Wall -Wextra -Wno-nullability-completeness -I../../packages/sdk/include \
    -o "$OUT/lower_test" tracks/lower_test.cpp "${SRC[@]}" & pids+=($!)
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -I../../packages/sdk/include -I../include \
    -Wno-nullability-completeness -c ../src/core/apply/tracks/boot.cpp -o "$OUT/boot.windows.o" & pids+=($!)
for p in "${pids[@]}"; do wait "$p"; done
GAMEDB="/mnt/gaming/SteamLibrary/steamapps/common/F1 25/game_package/gamedatabases/gamedatabases.2025.bin"
if [[ ${MAKE_FIXTURE:-} == 1 && -f $GAMEDB ]]; then
    "$OUT/lower_test" --make-fixture "$GAMEDB" tracks/fixture/tracks.tsv
fi
"$OUT/lower_test" tracks ../../packages/sdk/std/registries
