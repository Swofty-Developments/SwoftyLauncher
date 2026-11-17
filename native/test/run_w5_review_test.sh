#!/usr/bin/env bash
# Host build of w5_review_test.cpp, the same way run_hooks_test.sh builds hooks_test.cpp.
# Install: w5_review_test.cpp -> native/test/hooks/, this script -> native/test/.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
. "$(dirname "$SELF")/lib/env.sh"
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
OUT=${1:-$PWD/../build/w5-review-test}
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
mkdir -p "$OUT"
"${PIN[@]}" zig c++ -target x86_64-linux-musl -I../src -std=c++23 -O1 -Wall -Wextra -Wno-nullability-completeness \
    -I../../packages/sdk/include -o "$OUT/w5_review_test" hooks/w5_review_test.cpp
"$OUT/w5_review_test"
