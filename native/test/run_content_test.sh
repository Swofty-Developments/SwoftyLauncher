#!/usr/bin/env bash
# Runs the content registries test on the programs native/test/manifest.py builds for it into the folder given as
# $1. Run it as native/test/run_tests.sh content, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh content
OUT=$1
"$OUT/bc7_test" content/bc7
"$OUT/logo_cache_test"
"$OUT/logo_build_test" content/bc7 "$OUT/logo_out"
rm -rf "$OUT/logo_slices" "$OUT/logo_out_cold" "$OUT/logo_out_warm"
LOGO_SLICE_DIR="$OUT/logo_slices" "$OUT/logo_build_test" content/bc7 "$OUT/logo_out_cold" | grep 'slice cache in LOGO_SLICE_DIR'
LOGO_SLICE_DIR="$OUT/logo_slices" LOGO_SLICE_WARM=1 "$OUT/logo_build_test" content/bc7 "$OUT/logo_out_warm" | grep 'slice cache\(:\| in\) .*\(LOGO_SLICE_DIR\|second run\)'
python3 content/check_logo_slices.py content/bc7/expect/logo_out.md5 "$OUT/logo_out" "$OUT/logo_out_cold" "$OUT/logo_out_warm"
"$OUT/logo_deliver_test" content/bc7
"$OUT/garage_test"
python3 content/check_logo_build.py content/bc7 "$OUT/logo_out"
"$OUT/rows_test" ../../packages/sdk/std/registries
"$OUT/vehicle_tower_test"
"$OUT/package_id_test"
"$OUT/id_pool_test"
"$OUT/wear_test" ../../packages/sdk/std/registries
python3 content/check_lowering.py "$OUT/lower_tool"
python3 content/check_wide_default.py "$OUT/lower_tool"
GAMEDB="/mnt/gaming/SteamLibrary/steamapps/common/F1 25/game_package/gamedatabases/gamedatabases.2025.bin"
if [[ -f $GAMEDB ]]; then
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --out "$OUT/f2012_edits.json" --summary "$OUT/f2012_summary.json" \
        swofty.f2012 content/wear/f2012.json
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --out "$OUT/drs_edits.json" --summary "$OUT/drs_summary.json" \
        test.drs content/drs/drs.json
    python3 - "$OUT/drs_summary.json" <<'EOF'
import json, sys
s = json.load(open(sys.argv[1]))
lines = s["lines"]["rules.drs"]
assert s["records"].get("rules.drs") == 2 and s["boot_fresh"].get("rules.drs") == 16, s["records"]
assert any("test.drs/never: group swofty_drs_never_" in l and "race start lap 200" in l for l in lines), lines
print("rules.drs on the game database: 2 groups, 16 new keys")
EOF
fi
