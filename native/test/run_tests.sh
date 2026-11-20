#!/usr/bin/env bash
# Builds and runs the native tests (native/test/manifest.py) through ninja. With no argument it runs every host
# test; otherwise it runs the targets named: a test's name, `test` (every host test), `test-wine` (every Wine
# test) or any other ninja target. NATIVE_TEST_JOBS caps how many compiles, links and host tests run at once
# (default: the CPUs left after the pin, tools/pin.sh), and NATIVE_WINE_JOBS how many Wine tests run beside them
# (default: half that, at least 2 and at most 8).
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")/.."
NEEDS="zig ninja c++"
. test/lib/env.sh
export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache SWOFTY_TEST_GRAPH=1
mkdir -p build/test
if command -v flock >/dev/null; then
    exec 9>build/test/.build.lock
    flock -n 9 || { echo "run_tests.sh: another test run holds build/test; waiting for it" >&2 && flock 9; }
fi
python3 tools/ninja_deps.py build/test/.ninja_deps run_tests.sh
JOBS=${NATIVE_TEST_JOBS:-$SWOFTY_JOBS}
WINE_JOBS=${NATIVE_WINE_JOBS:-$((JOBS / 2 < 2 ? 2 : JOBS / 2 > 8 ? 8 : JOBS / 2))}
python3 tools/test_graph.py --builddir build/test --out build/test/build.ninja --jobs "$JOBS" --wine-jobs "$WINE_JOBS"
[ $# -gt 0 ] || set -- test
exec "${PIN[@]}" ninja -f build/test/build.ninja -k 0 -j "$((JOBS + WINE_JOBS))" "$@"
