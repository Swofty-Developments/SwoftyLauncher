#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"

if ! command -v zig >/dev/null || ! command -v ninja >/dev/null; then
    exec nix shell nixpkgs#zig nixpkgs#ninja -c "$SELF" "$@"
fi
export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache

VARIANT=${VARIANT:-}
case $VARIANT in
'') DEFS=() ;;
nomodsettings) DEFS=(-DSWOFTY_NO_MOD_SETTINGS) ;;
hooknoop) DEFS=(-DSWOFTY_MOD_SETTINGS_NOOP) ;;
*)
    echo "build.sh: unknown VARIANT '$VARIANT' (known: nomodsettings, hooknoop)" >&2
    exit 2
    ;;
esac
CORE_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' ../Cargo.toml | head -1)
DEFS+=(-DSWOFTY_CORE_VERSION="\"$CORE_VERSION\"")
DEFS+=(-DSWOFTY_MEM_SHARED)
DIST=dist${VARIANT:+-$VARIANT}
RELEASE=release${VARIANT:+-$VARIANT}
OUT=build${VARIANT:+/$VARIANT}

SDK=../packages/sdk/include
. tools/pin.sh
mkdir -p build "$OUT"
# One build at a time per build directory: two ninjas writing one deps log damage it (tools/ninja_deps.py).
if command -v flock >/dev/null; then
    exec 9>"$OUT/.build.lock"
    flock -n 9 || { echo "build.sh: another build holds $OUT; waiting for it" >&2 && flock 9; }
fi
rm -rf "$DIST"
mkdir -p "$DIST/swofty/addons/core"
python3 tools/supported_builds.py ../packages/core/src/games/f1_25/build_support.rs "$OUT/supported_builds.h.new"
cmp -s "$OUT/supported_builds.h.new" "$OUT/supported_builds.h" || mv "$OUT/supported_builds.h.new" "$OUT/supported_builds.h"
rm -f "$OUT/supported_builds.h.new"
# bindgen writes bindings.json for the KB build and bindings.<build>.json for every other overlay in
# re/db/builds; core picks the set whose build.pe_timestamp matches the running exe.
python3 ../re/tools/bindgen.py --json "$DIST/swofty/addons/core/bindings.json" --symbols "$DIST/swofty/addons/core/symbols.tsv"
# With a game image at hand, resolve the emitted sets against it the way core does; a binding that fails stops the build.
if [[ -n ${SWOFTY_STEAM_EXE:-} ]]; then
    python3 ../re/tools/bindverify.py --set "$DIST/swofty/addons/core/bindings.json" --image "$SWOFTY_STEAM_EXE"
fi
if [[ -n ${SWOFTY_EA_DUMP:-} ]]; then
    python3 ../re/tools/bindverify.py --set "$DIST/swofty/addons/core/bindings.ea.json" --image "$SWOFTY_EA_DUMP" \
        ${SWOFTY_STEAM_EXE:+--compare "$SWOFTY_STEAM_EXE"}
fi
# swofty-core-<version>-patches.json lists every byte core writes in the exe on each supported build
# (tools/patch_sites.txt); the server's code challenges skip them. It is a release asset in $RELEASE, outside the game
# folder. With a Steam exe at hand, the sites of its build are resolved on it and each hook's instructions decoded, with
# re/tools/py's capstone.
rm -rf "$RELEASE"
mkdir -p "$RELEASE"
PATCH_PY=python3
if [[ -n ${SWOFTY_STEAM_EXE:-} ]]; then
    PATCH_PY=../re/tools/py
fi
"$PATCH_PY" tools/patch_sites.py --sets "$DIST/swofty/addons/core" --out "$RELEASE/swofty-core-$CORE_VERSION-patches.json" \
    --core-version "$CORE_VERSION" ${SWOFTY_STEAM_EXE:+--image "$SWOFTY_STEAM_EXE"}
python3 - src/core/runtime/stage.cpp "$DIST/swofty/addons/core/features.json" <<'EOF'
import json, re, sys
table = open(sys.argv[1]).read().split('FEATURES[] = ', 1)[1].split(';', 1)[0]
features = {name: 'stable' if stable == 'true' else 'experimental' for name, stable in re.findall(r'\{"(\w+)", (true|false)\}', table)}
json.dump({'schema': 1, 'features': features}, open(sys.argv[2], 'w'))
EOF

# GAME_INCLUDE points the core at other generated game headers (bindgen --dev output) ahead of the
# shipped ones in include/. The career library and the gamedb string decode compile out, and report
# unavailable at runtime, when the headers lack the KB structs they read (src/core/runtime/layout.hpp).
ORDER=()
[ -n "$VARIANT" ] || ORDER=(--order-snapshot tools/core_init_order.txt)
python3 tools/build_graph.py --builddir "$OUT" --out "$OUT/build.ninja" --sdk "$SDK" \
    --cc "zig cc -target x86_64-windows-gnu" --cxx "zig c++ -target x86_64-windows-gnu -std=c++23" \
    "${DEFS[@]/#/--define=}" --game-include "${GAME_INCLUDE:-}" "${ORDER[@]}"
python3 tools/ninja_deps.py "$OUT/.ninja_deps" build.sh
"${PIN[@]}" ninja -f "$OUT/build.ninja" -j "${NINJA_JOBS:-$SWOFTY_JOBS}"

cp "$OUT/version.dll" "$DIST/version.dll"
cp "$OUT/swofty_core.dll" "$DIST/swofty/addons/core/swofty_core.dll"
cp "$OUT/swofty_gamelog.dll" "$DIST/swofty/addons/core/swofty_gamelog.dll"
cp src/gamelog/log_channels.tsv "$DIST/swofty/addons/core/log_channels.tsv"

for f in "$DIST/version.dll" "$DIST/swofty/addons/core/swofty_core.dll" "$DIST/swofty/addons/core/swofty_gamelog.dll"; do
    echo "== $f"
    command -v objdump >/dev/null && objdump -p "$f" | grep -E '^(Magic|Characteristics)|file format|DLL Name|^\s+\[ *[0-9]+\] \+base' || true
    command -v objdump >/dev/null && objdump -f "$f" | grep -E 'architecture'
    md5sum "$f"
done
md5sum "$DIST"/swofty/addons/core/bindings*.json "$DIST/swofty/addons/core/symbols.tsv" "$DIST/swofty/addons/core/log_channels.tsv" \
    "$DIST/swofty/addons/core/features.json" "$RELEASE/swofty-core-$CORE_VERSION-patches.json"
