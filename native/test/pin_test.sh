#!/usr/bin/env bash
# Tests swofty_pin_plan (native/tools/pin.sh) on given CI variables, taskset presence, affinities and CPU counts.
set -uo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
. tools/pin.sh
failed=0

expect() {
    local want_pin=$1 want_jobs=$2
    shift 2
    swofty_pin_plan "$@"
    if [ "${PIN[*]}" = "$want_pin" ] && [ "$SWOFTY_JOBS" = "$want_jobs" ]; then
        echo "ok   ($*): PIN='${PIN[*]}' jobs=$SWOFTY_JOBS"
    else
        echo "FAIL ($*): PIN='${PIN[*]}' jobs=$SWOFTY_JOBS, expected PIN='$want_pin' jobs=$want_jobs"
        failed=1
    fi
}

mask="taskset -c 2-7,10,11,14-31 nice -n 10"
expect "$mask" 26 "" 1 0-31 32
expect "$mask" 26 "" 1 0-11,14-31 30
expect "nice -n 10" 16 "" 1 16-31 16
expect "nice -n 10" 26 "" 1 2-7,10,11,14-31 26
expect "nice -n 10" 28 "" 1 0-27 28
expect "nice -n 10" 4 "" 1 0-3 4
expect "nice -n 10" 1 "" 1 5 1
expect "" 4 true 1 0-3 4
expect "" 32 true 1 0-31 32
expect "" 8 "" 0 "" 8
expect "" 2 1 0 "" 2

[ "$(swofty_cpus 0-2,5,7-8 | tr '\n' ' ')" = "0 1 2 5 7 8 " ] && echo "ok   swofty_cpus 0-2,5,7-8" || {
    echo "FAIL swofty_cpus 0-2,5,7-8: $(swofty_cpus 0-2,5,7-8 | tr '\n' ' ')"
    failed=1
}
exit $failed
