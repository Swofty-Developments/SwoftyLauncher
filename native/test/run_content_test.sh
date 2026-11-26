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
"$OUT/slices_test" ../../packages/sdk/std/registries
"$OUT/garage_test"
"$OUT/box_ui_test"
python3 content/check_logo_build.py content/bc7 "$OUT/logo_out"
"$OUT/rows_test" ../../packages/sdk/std/registries
"$OUT/vehicle_tower_test"
"$OUT/package_id_test"
"$OUT/id_pool_test"
"$OUT/box_world_test"
"$OUT/option_tags_test"
"$OUT/gvdb_file_test"
"$OUT/wear_test" ../../packages/sdk/std/registries
python3 content/check_lowering.py "$OUT/lower_tool"
python3 content/check_wide_default.py "$OUT/lower_tool"
# The steps on the game database read a stock copy (content/stock_gamedb.py), never the game folder's.
if GAMEDB=$(python3 content/stock_gamedb.py 2>"$OUT/stock_gamedb.why"); then
    echo "game database steps on $GAMEDB"
    "$OUT/gvdb_file_test" "$GAMEDB"
    "$OUT/option_tags_test" "$GAMEDB"
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --out "$OUT/f2012_edits.json" --summary "$OUT/f2012_summary.json" \
        swofty.f2012 content/wear/f2012.json
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --out "$OUT/drs_edits.json" --summary "$OUT/drs_summary.json" \
        test.drs content/drs/drs.json
    CS=()
    for y in 2016 2017 2018 2019 2020 2021 2022 2023 2024; do
        CS+=("swofty.classic-seasons-$y" "content/classic-seasons/classic-seasons-$y/registry/content.json")
    done
    for how in here worker; do
        "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --plan "$how" \
            --out "$OUT/plan_${how}_edits.json" --summary "$OUT/plan_${how}_summary.json" "${CS[@]}" >/dev/null 2>&1 || true
    done
    cmp "$OUT/plan_here_edits.json" "$OUT/plan_worker_edits.json"
    python3 - "$OUT/plan_here_summary.json" "$OUT/plan_worker_summary.json" "$OUT/plan_here_edits.json" <<'EOF'
import json, sys
here, worker = (json.load(open(p)) for p in sys.argv[1:3])
edits = len(json.load(open(sys.argv[3]))["db_edits"])
assert not here["plan_off_thread"] and worker["plan_off_thread"], (here["plan_off_thread"], worker["plan_off_thread"])
assert here["plan_fresh"] == worker["plan_fresh"] and here["staged_records"] == worker["staged_records"], "plans differ"
assert edits > 50000, edits
print("Classic Seasons on the game database: the plan counted on a worker and on this thread give the same %d database edit(s)" % edits)
EOF
    python3 - "$OUT/drs_summary.json" <<'EOF'
import json, sys
s = json.load(open(sys.argv[1]))
lines = s["lines"]["rules.drs"]
assert s["records"].get("rules.drs") == 2 and s["boot_fresh"].get("rules.drs") == 16, s["records"]
assert any("test.drs/never: group swofty_drs_never_" in l and "race start lap 200" in l for l in lines), lines
print("rules.drs on the game database: 2 groups, 16 new keys")
EOF
    python3 - content/classic-seasons/classic-seasons-2016/registry/content.json "$OUT/box_world_box.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
for pkg in d["content.package"].values():
    pkg.pop("calendar", None)
    for t in pkg.get("teams", []):
        t.pop("drivers", None)
json.dump(d, open(sys.argv[2], "w"))
EOF
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --edits content/box_world/swofty_f1_edits.json \
        --out "$OUT/box_world_edits.json" --summary "$OUT/box_world_summary.json" --box-worlds \
        swofty.classic-seasons-2016 "$OUT/box_world_box.json"
    python3 - "$OUT/box_world_summary.json" <<'EOF'
import json, sys
s = json.load(open(sys.argv[1]))
lines = s["lines"]["content.package"]
assert s["records"].get("content.package") == 1 and not s["refused"], (s["records"], s["refused"])
assert any("11 teams, its world holds 10 garages: season swofty_box_cs_f1_2016" in l and
           "layer swofty_box_cs_f1_2016_world (f1_26, 11 garages" in l for l in lines), lines
print("box worlds on the game database: an 11-team swofty_f1 box gets its own season and f1_26 layer")
EOF
    python3 - content/classic-seasons/classic-seasons-2016/registry/content.json "$OUT/box_world_tt.json" <<'EOF'
import copy, json, sys
d = json.load(open(sys.argv[1]))
extra = copy.deepcopy(d["vehicle"]["mercedes"])
extra.pop("stat_id", None)
extra["label"] = "cs16_mercedes_tt"
d["vehicle"]["mercedes_tt"] = extra
pkg = d["content.package"].pop(next(iter(d["content.package"])))
pkg.pop("calendar", None)
pkg["entrant_list"] = "cs_f1_2016_tt"
pkg["database"] = {"entrant_list_from": "f1_2025", "standings_from": "f1_teams_2024",
                   "entrant_list_set": {"previous_season_driver_standings": ""}}
pkg["teams"].append(dict(copy.deepcopy(pkg["teams"][0]), key="mercedes_tt", vehicle="swofty.classic-seasons-2016/mercedes_tt"))
for t in pkg["teams"]:
    t.pop("drivers", None)
d["content.package"] = {"f1_2016_tt": pkg}
json.dump(d, open(sys.argv[2], "w"))
EOF
    "$OUT/rows_tool" --registries ../../packages/sdk/std/registries --db "$GAMEDB" --edits content/box_world/swofty_f1_edits.json \
        --out "$OUT/box_world_tt_edits.json" --summary "$OUT/box_world_tt_summary.json" --box-worlds \
        swofty.classic-seasons-2016 "$OUT/box_world_tt.json"
    python3 - "$OUT/box_world_tt_summary.json" <<'EOF'
import json, sys
s = json.load(open(sys.argv[1]))
lines = s["lines"]["content.package"]
assert s["records"].get("content.package") == 1 and not s["refused"], (s["records"], s["refused"])
assert any("12 teams, its world holds 10 garages: season swofty_box_cs_f1_2016_tt" in l and
           "layer swofty_box_cs_f1_2016_tt_world (f1_26_myteam, 12 garages" in l for l in lines), lines
print("box worlds on the game database: a 12-team Time Trial list with no season of its own gets a My Team layer")
EOF
    python3 content/check_boot_ahead.py "$OUT/rows_tool" "$GAMEDB" "$OUT" "${CS[@]}" -- \
        swofty.classic-seasons-2016 "$OUT/box_world_box.json"
else
    echo "game database steps skipped: $(cat "$OUT/stock_gamedb.why")"
fi
