#!/usr/bin/env bash
# The physics non-finite log on the game's own code: builds phys_nonfinite_harness.cpp with safetyhook and runs it under
# Wine once per game image on disk (SWOFTY_KB_EXE, SWOFTY_STEAM_145_EXE, SWOFTY_EA_DUMP, SWOFTY_EA_145_DUMP; a flat EA
# dump has file offset = RVA). Each run finds the functions by their binding patterns, proves the log's lines and that
# it writes nothing, and times the walk with 500 and 5000 bodies. An image that is not on disk is skipped.
set -uo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
NEEDS="zig wine"
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/phys-nonfinite-test}
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
export WINEPREFIX=${WINEPREFIX:-$OUT/prefix} WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=;winemenubuilder.exe,winedbg.exe=d"
SH=../third_party/safetyhook
B=/mnt/gaming/f1-builds
D=/mnt/gaming/EAApp/ai-fix/dumps
IMAGES=(
    "kb|${SWOFTY_KB_EXE:-$B/1.0.141.7865-build25251167/F1_25.exe}|pe"
    "steam-1.0.145|${SWOFTY_STEAM_145_EXE:-$B/1.0.145.2472-build26566308/F1_25.exe}|pe"
    "ea-1.0.141|${SWOFTY_EA_DUMP:-$D/F1_25.1789128892.110548.mem}|flat"
    "ea-1.0.145|${SWOFTY_EA_145_DUMP:-$D/F1_25.1794315435.105156.mem}|flat"
)
mkdir -p "$OUT"
[[ $OUT/Zydis.o -nt $SH/Zydis.c ]] || "${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -I$SH -c $SH/Zydis.c -o "$OUT/Zydis.o" &
[[ $OUT/safetyhook.o -nt $SH/safetyhook.cpp ]] ||
    "${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -I$SH -c $SH/safetyhook.cpp -o "$OUT/safetyhook.o" &
wait
"${PIN[@]}" zig c++ -target x86_64-windows-gnu -std=c++23 -O2 -Wall -Wextra -Wno-nullability-completeness -I../src \
    -I../../packages/sdk/include -I../include -I$SH -Wl,--image-base=0x10000000 -o "$OUT/phys_nonfinite_harness.exe" \
    phys_nonfinite_harness.cpp "$OUT/safetyhook.o" "$OUT/Zydis.o" || exit 1
python3 - ../../re/bindings/core "$OUT/patterns.tsv" <<'EOF' || exit 1
import json, sys
names = ['Ne_PhysUpdateStage_FinishStepNotifyBodies', 'Ne_PhysBody_ApplyImpulse', 'Ne_PhysBody_SetLinearVelocity',
         'Ne_PhysBody_SetAngularVelocity']
with open(sys.argv[2], 'w') as out:
    for n in names:
        out.write(f"{n}\t{json.load(open(f'{sys.argv[1]}/{n}.json'))['signature']['pattern']}\n")
EOF
timeout 300 wineboot -i >/dev/null 2>&1
status=0 ran=0
for entry in "${IMAGES[@]}"; do
    IFS='|' read -r label image kind <<<"$entry"
    if [[ ! -f $image ]]; then
        echo "skip: $label: no image at $image"
        continue
    fi
    ran=$((ran + 1))
    timeout 900 "${PIN[@]}" wine "$OUT/phys_nonfinite_harness.exe" "$label" "Z:${image//\//\\}" "$kind" \
        "Z:${OUT//\//\\}\\patterns.tsv" 2>/dev/null
    r=$?
    [[ $r -eq 0 ]] || {
        printf 'phys non-finite harness (%s): exit %d (0x%08x)\n' "$label" "$r" "$((r & 0xffffffff))"
        status=1
    }
done
[[ $ran -gt 0 ]] || echo "skip: no game image on disk"
exit $status
