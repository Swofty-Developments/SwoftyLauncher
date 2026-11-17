#!/usr/bin/env bash
# run_one.sh <name> <dir> <cwd> <command...>: runs one test from <cwd> with its output in <dir>/log. A pass
# writes <dir>/ok, which is the ninja output of the test, and the seconds it took to <dir>/seconds.
name=$1 dir=$2 cwd=$3
shift 3
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
