#!/usr/bin/env bash
# run_one.sh <name> <kind> <dir> <cwd> <command...>: runs one test from <cwd> with its output in <dir>/log, with
# SWOFTY_TEST_KIND set to its kind (host or wine; game_lock.sh refuses a host test). A pass writes <dir>/ok, which is
# the ninja output of the test, and the seconds it took to <dir>/seconds.
name=$1 dir=$3 cwd=$4
export SWOFTY_TEST_KIND=$2
shift 4
mkdir -p "$dir"
rm -f "$dir/ok" "$dir/seconds"
start=$SECONDS
(cd "$cwd" && "$@") >"$dir/log" 2>&1
rc=$?
echo $((SECONDS - start)) >"$dir/seconds"
if [ $rc -eq 0 ]; then
    touch "$dir/ok"
    echo "ok   $((SECONDS - start))s $name"
else
    echo "FAIL $((SECONDS - start))s $name (exit $rc); the last lines of $dir/log:"
    tail -n 30 "$dir/log"
fi
exit $rc
