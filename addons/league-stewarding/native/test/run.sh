#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"

if command -v zig >/dev/null; then
    ZIG=(zig)
else
    ZIG=(nix run nixpkgs#zig --)
fi

OUT=${STEWARD_TEST_DIR:-${TMPDIR:-/tmp}/steward-test}
mkdir -p "$OUT"
export ZIG_GLOBAL_CACHE_DIR=$OUT/zig-cache ZIG_LOCAL_CACHE_DIR=$OUT/zig-cache

"${ZIG[@]}" c++ -target x86_64-linux-musl -std=c++23 -O1 -Wall -Wextra -Wno-nullability-completeness -I../src \
    -o "$OUT/steward_logic_test" steward_logic_test.cpp
"$OUT/steward_logic_test" "$@"

"${ZIG[@]}" c++ -target x86_64-linux-musl -std=c++23 -O1 -Wall -Wextra -Wno-nullability-completeness -I../src \
    -o "$OUT/telemetry_parse_test" telemetry_parse_test.cpp
"$OUT/telemetry_parse_test" "$@"
