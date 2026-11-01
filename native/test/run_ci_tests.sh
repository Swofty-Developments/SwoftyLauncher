#!/usr/bin/env bash
# Runs CI's native tests side by side, NATIVE_TEST_JOBS at a time (default: one per core). A line reports each
# test as it ends; then each test's output follows in a log group, in the order below, longest first.
# The Wine tests need Wine and an xvfb-run that gives every call its own display, as ci.yml sets up; the
# stock xvfb-run -a can hand two calls started together the same one. Tests given the same lock name run
# one at a time.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$PWD
export ZIG_GLOBAL_CACHE_DIR=$ROOT/native/build/zig-cache ZIG_LOCAL_CACHE_DIR=$ROOT/native/build/zig-cache
export AT_TEST_DIR=${RUNNER_TEMP:-/tmp}/any-team-test

NAMES=()
CMDS=()
LOCKS=()
t() { NAMES+=("$1"); CMDS+=("$2"); LOCKS+=("${3:-}"); }

zig_test() { echo "zig c++ -target x86_64-linux-musl -std=c++23 -O2 $1 -o $2 && ./$2"; }
cme=addons/career-editor/native

# Both bind 127.0.0.1:21414, the game's LAN port. The Wine test skips its case when the port is taken; the
# socket hooks test fails, so it goes first.
t "Socket hooks Wine test" native/test/run_netplay_test.sh port-21414
# A synthetic F1_25.exe carries the signatures; no game files are used.
t "Wine test" native/test/run_wine_test.sh port-21414
t "Registry core (shared corpus, Python reference, random stacks, read benchmark)" native/test/run_registry_test.sh
t "Standard library 1.6 headers compile test" native/test/run_sdk_headers_test.sh
t "Schema check unit and agreement test" native/test/run_schema_test.sh
t "Test agent Wine test" native/test/run_testagent_test.sh
# Builds every templates/std addon with its native/build.sh and validates its registry data files.
t "Standard library templates" "python3 tools/std/templates.py"
t "Events dispatcher unit test" native/test/run_events_test.sh
t "Progressive registration test" addons/progressive/native/test/register/run.sh
t "Content registries (row engine, Classic Seasons lowering, variants, stat-id pools)" native/test/run_content_test.sh
# Without work/exe/F1_25.exe it runs the synthetic packed image only.
t "Binding resolver Wine test" native/test/run_bind_test.sh
t "Career operations planner and SDK calls host test" native/test/run_career_ops_test.sh
t "Any Team Transfers unit test and build" "addons/any-team-transfers/native/test/run.sh && addons/any-team-transfers/native/build.sh"
t "swofty::json unit test" native/test/run_json_test.sh
t "Live apply paths (scheduler, put-back, context, budgets, sizing, stat ids, reports)" native/test/run_apply_test.sh
t "Track registries (seeds, record lowering, parity with the launcher's track activation)" native/test/run_tracks_test.sh
t "Save sandbox host and Wine test" native/test/run_save_sandbox_test.sh
t "RE-Optimization unit and Wine hook tests" addons/re-optimization/native/test/run.sh
t "Progressive effective records test" addons/progressive/native/test/records/run.sh
t "Runtime assets Wine test" native/test/run_runtime_assets_test.sh
t "Box track lists (fit rules, calendar and More tracks, per-box entries, fuel plan, guard decision)" native/test/run_track_lists_test.sh
t "Career snapshots (swofty_registry entry, freezes, budgets, migration, fixtures)" native/test/run_snapshots_test.sh
t "Socket trace Wine test" native/test/run_net_trace_test.sh
t "Career UI Wine test" native/test/run_career_ui_test.sh
t "Loader log fallback Wine test" native/test/run_log_fallback_test.sh
t "String override Wine test" native/test/run_lang_override_test.sh
t "Overlay toolkit Wine test" native/test/run_toolkit_test.sh
t "Overlay draw-list unit test" native/test/run_hud_test.sh
# The 1.6 handler guard, built as build.sh builds it, against faults and SDK handlers that throw.
t "Handler guard Wine test" native/test/run_guard_test.sh
t "Career list Wine test" native/test/run_career_list_test.sh
t "Hook points, events and services host test" native/test/run_hooks_test.sh
t "Game hook points host test" native/test/run_game_points_test.sh
t "Session control, AI controller and spectator points host test" native/test/run_session_points_test.sh
t "Live car operations host test" native/test/run_car_live_test.sh
t "Hook points review test" native/test/run_w5_review_test.sh
t "Shader look unit test" native/test/run_grade_test.sh
t "ERS new-lap site unit test" native/test/run_ers_newlap_test.sh
t "Round track record guard unit test" native/test/run_track_record_test.sh
t "Tyre slot guard unit test" native/test/run_tyre_slots_test.sh
t "Pit slot guard unit test" native/test/run_pit_slots_test.sh
t "Pit box guard unit test" native/test/run_pit_box_test.sh
t "Direct start unit test" native/test/run_direct_start_test.sh
t "Thrustmaster LCD guard unit test" native/test/run_thrustmaster_lcd_test.sh
t "Garage setup guard unit test" native/test/run_garage_setup_test.sh
t "Garage crew guard unit test" native/test/run_garage_crew_test.sh
t "Session enter guard unit test" native/test/run_session_enter_test.sh
t "Audio job queue fix unit test" native/test/run_audio_queue_test.sh
t "Boot watchdog grace unit test" native/test/run_watchdog_grace_test.sh
t "Pattern scan and scan cache unit test" native/test/run_scan_test.sh
t "Telemetry feed unit test" "$(zig_test "-I packages/sdk/include native/test/telemetry_feed_test.cpp" telemetry_feed_test)"
t "Career Mode Editor race number unit test" "$(zig_test "-I $cme/src $cme/test/race_numbers_test.cpp $cme/src/race_numbers.cpp" race_numbers_test)"
t "Career Mode Editor weather plan unit test" "$(zig_test "-I $cme/src $cme/test/weather_plan_test.cpp $cme/src/weather_plan.cpp" weather_plan_test)"
t "Career Mode Editor line-up unit test" "$(zig_test "-I $cme/src $cme/test/lineup_test.cpp $cme/src/lineup.cpp" lineup_test)"
t "Career Mode Editor entrant bound unit test" "$(zig_test "-I $cme/src $cme/test/entrants_cap_test.cpp" entrants_cap_test)"
t "Career Mode Editor rating search unit test" "$(zig_test "-I $cme/src $cme/test/rating_solve_test.cpp $cme/src/rating_solve.cpp" rating_solve_test)"
t "Career Mode Editor calendar rules unit test" "$(zig_test "-I $cme/src $cme/test/calendar_rules_test.cpp $cme/src/calendar_rules.cpp" calendar_rules_test)"

JOBS=${NATIVE_TEST_JOBS:-$(nproc)}
LOGS=$(mktemp -d)
echo "${#CMDS[@]} tests, $JOBS at a time"
for i in "${!CMDS[@]}"; do
    while [ "$(jobs -rp | wc -l)" -ge "$JOBS" ]; do
        wait -n
    done
    (
        start=$SECONDS
        if [ -n "${LOCKS[$i]}" ]; then
            flock "$LOGS/${LOCKS[$i]}.lock" bash -c "${CMDS[$i]}"
        else
            bash -c "${CMDS[$i]}"
        fi >"$LOGS/$i.log" 2>&1
        rc=$?
        echo "$rc $((SECONDS - start))" >"$LOGS/$i.rc"
        echo "$([ $rc = 0 ] && echo "ok  " || echo FAIL) $((SECONDS - start))s ${NAMES[$i]}"
    ) &
done
wait

failed=()
for i in "${!CMDS[@]}"; do
    read -r rc secs <"$LOGS/$i.rc" 2>/dev/null || { rc=1 secs=?; }
    echo "::group::$([ "$rc" = 0 ] && echo ok || echo FAILED) ${secs}s ${NAMES[$i]}"
    cat "$LOGS/$i.log"
    echo "::endgroup::"
    [ "$rc" = 0 ] || failed+=("${NAMES[$i]}")
done
for name in "${failed[@]}"; do
    echo "::error title=Native test failed::$name"
done
[ ${#failed[@]} -eq 0 ]
