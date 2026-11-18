#!/usr/bin/env bash
# run_one.sh <name> <kind> <dir> <cwd> <command...>: runs one test from <cwd> with its output in <dir>/log, with
# SWOFTY_TEST_KIND set to its kind (host or wine; game_lock.sh refuses a host test). A pass writes <dir>/ok, which is
# the ninja output of the test, and the seconds it took to <dir>/seconds. A failure prints the log's FAIL lines, with
# their line numbers, before its last lines, so a check that failed early is not cut off.
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
    fails=$(grep -n '^FAIL' "$dir/log" | head -n 20)
    if [ -n "$fails" ]; then
        echo "FAIL $((SECONDS - start))s $name (exit $rc); the FAIL lines of $dir/log:"
        echo "$fails"
        echo "and its last lines:"
    else
        echo "FAIL $((SECONDS - start))s $name (exit $rc), with no FAIL line; the last lines of $dir/log:"
    fi
    tail -n 30 "$dir/log"
fi
exit $rc
