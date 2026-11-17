#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
CXX=${CXX:-c++}
NEEDS=c++
. "$(dirname "$SELF")/lib/env.sh"
OUT=${1:-$PWD/../build/session-job-probe-test}
mkdir -p "$OUT"
"$CXX" -std=c++23 -O2 -Wall -Wextra -pthread -I../src -o "$OUT/session_job_probe_test" session_job_probe_test.cpp
"$OUT/session_job_probe_test"
