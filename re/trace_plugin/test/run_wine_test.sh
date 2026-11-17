#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
if ! command -v wine >/dev/null || ! command -v xvfb-run >/dev/null || ! command -v zig >/dev/null; then
    exec nix shell nixpkgs#zig nixpkgs#wineWow64Packages.stable nixpkgs#xvfb-run -c "$SELF" "$@"
fi
OUT=${1:-$PWD/../build/wine-test}
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
. ../../../native/tools/pin.sh
rm -rf "$OUT/game"
mkdir -p "$OUT/game/swofty/mods"
"${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -o "$OUT/game/F1_25.exe" fake_game.c -lversion -luser32
cp ../../../native/dist/version.dll "$OUT/game/"
cp ../build/swofty_trace.dll "$OUT/game/swofty/mods/"
python3 mksigs.py "$OUT/game/F1_25.exe" > "$OUT/signatures.json"
python3 ../../tools/trace_build.py --signatures "$OUT/signatures.json" --build-from-exe "$OUT/game/F1_25.exe" \
    -o "$OUT/game/swofty/trace.json" T900.trace.json
export WINEPREFIX="$OUT/prefix" WINEDLLOVERRIDES="version=n,b;mscoree,mshtml=;winemenubuilder.exe=d" WINEDEBUG=-all
status=0
"${PIN[@]}" xvfb-run -a wine "$OUT/game/F1_25.exe" || status=$?
echo "---- loader.log"
cat "$OUT/game/swofty/loader.log"
echo "---- trace.log"
cat "$OUT/game/swofty/trace.log"
echo "---- check"
python3 check_log.py "$OUT/game/swofty/trace.log"
[ "$status" = 0 ] || exit "$status"

echo "---- phase 2: build mismatch must install nothing"
cp "$OUT/game/swofty/trace.log" "$OUT/trace.phase1.log"
wineserver -w || true
python3 -c "import json,sys; p=sys.argv[1]; d=json.load(open(p)); d['build']['size']+=1; json.dump(d,open(p,'w'),indent=1)" "$OUT/game/swofty/trace.json"
status2=0
"${PIN[@]}" xvfb-run -a wine "$OUT/game/F1_25.exe" >/dev/null || status2=$?
echo "phase 2 exit status $status2"
cat "$OUT/game/swofty/trace.log"
grep -q 'result=MISMATCH allowed=0' "$OUT/game/swofty/trace.log" &&
    grep -q 'status=idle reason=build_mismatch installed=0' "$OUT/game/swofty/trace.log" &&
    ! grep -q '^[RE] ' "$OUT/game/swofty/trace.log" &&
    echo "ok   build mismatch: nothing installed, no events" || { echo "FAIL build mismatch phase"; exit 1; }
exit $status
