#!/usr/bin/env bash
# Builds and runs the native tests (native/test/manifest.py) through ninja. With no argument it runs every host
# test; otherwise it runs the targets named: a test's name, `test` (every host test), `test-wine` (every Wine
# test) or any other ninja target. NATIVE_TEST_JOBS caps how many run at once.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")/.."
NEEDS="zig ninja c++"
. test/lib/env.sh
export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache SWOFTY_TEST_GRAPH=1
mkdir -p build/test
python3 tools/test_graph.py --builddir build/test --out build/test/build.ninja
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
command -v taskset >/dev/null || PIN=()
[ $# -gt 0 ] || set -- test
exec "${PIN[@]}" ninja -f build/test/build.ninja -k 0 ${NATIVE_TEST_JOBS:+-j "$NATIVE_TEST_JOBS"} "$@"
