# Sourced by the Wine test scripts after env.sh. game_run [-t SECONDS] EXE [ARGS...] runs a fake F1_25.exe once
# under Wine, on an Xvfb display of its own (xvfb-run -a), and returns the game's own exit code; the game's output
# goes to the caller's stdout once it ends. SECONDS (default 120) is the timeout, which returns 124 (137 when the
# game outlives the 5 s after it).
#
# Every run starts and ends with no wineserver for WINEPREFIX: game_settle stops it and waits until it is gone, so a
# run never meets the server of the run before while that server shuts down (3 s after its last process ends) or
# while the processes it started on the display xvfb-run just closed still run. An empty WINEPREFIX is made first,
# outside the game's timeout.
#
# The run goes through `wine cmd /c`, whose exit code is the game process's own. A bare `wine EXE` exits with the
# status of the Unix process instead: the process tells the wineserver it ended, then runs the Unix exit handlers,
# and the wineserver kills it if it still runs a second later (137); a thread that faults in that teardown gives 139.
#
# A run that exits nonzero while the loader wrote nothing (version.dll beside EXE, and neither EXE's
# swofty\loader.log nor the loader's start marker in the prefix changed) is a Wine start failure, not a game result:
# it prints "retrying: wine start failure" with Wine's output and runs once more, and the second result is final.
# Every other result returns at once. A run still going 5 s before its timeout prints what the prefix's processes
# and their threads are doing and the end of EXE's loader.log. These lines go to the stderr the script was started
# with, so a caller's redirect of the game's output does not hide them. native/test/game_run_test.sh tests it.
. "$(dirname "${BASH_SOURCE[0]}")/wineserver.sh"
GAME_RUN_SERVER=$(swofty_wineserver) || GAME_RUN_SERVER=
exec {GAME_RUN_ERR}>&2

game_settle() {
    [ -n "$GAME_RUN_SERVER" ] && [ -n "${WINEPREFIX:-}" ] || return 0
    timeout 30 "$GAME_RUN_SERVER" -k >/dev/null 2>&1 || :
    timeout 30 "$GAME_RUN_SERVER" -w >/dev/null 2>&1 || :
}

game_run_marker() {
    local user
    for user in "$WINEPREFIX"/drive_c/users/*; do
        if [[ ${user##*/} != Public && -d $user/AppData/Local ]]; then
            echo "$user/AppData/Local/Swofty/state/loader-start.marker"
            return 0
        fi
    done
    return 0
}

game_run_traces() {
    local file
    for file in "$@"; do
        stat -c '%i %s %y' "$file" 2>/dev/null || echo -
    done
}

game_run_prefix() {
    [ -f "$WINEPREFIX/system.reg" ] && return 0
    local rc=0
    timeout 300 "${PIN[@]}" xvfb-run -a wine wineboot -i >/dev/null 2>&1 || rc=$?
    ((rc == 0)) || echo "game_run: making the Wine prefix $WINEPREFIX failed (exit $rc)" >&$GAME_RUN_ERR
    game_settle
}

game_run_report() {
    local exe=$1 after=$2 secs=$3 procs proc cmd task stat log
    echo "game_run: $exe still runs $after s after it started (its timeout is $secs s); the processes of $WINEPREFIX:"
    procs=$(grep -slzxF "WINEPREFIX=$WINEPREFIX" /proc/[0-9]*/environ) || :
    for proc in $procs; do
        proc=${proc%/environ}
        cmd=$(tr '\0' ' ' <"$proc/cmdline" 2>/dev/null | sed 's/[[:space:]]*$//') || :
        [ -n "$cmd" ] || continue
        echo "  pid ${proc#/proc/}: $cmd"
        for task in "$proc"/task/*; do
            stat=$(cat "$task/stat" 2>/dev/null) || continue
            stat=${stat##*) }
            echo "    thread ${task##*/} '$(cat "$task/comm" 2>/dev/null)' state ${stat%% *} in $(cat "$task/wchan" 2>/dev/null || echo '?')"
        done
    done
    log=$(dirname "$exe")/swofty/loader.log
    if [ -f "$log" ]; then
        echo "game_run: the last lines of $log:"
        tail -n 25 "$log" | sed 's/^/  /'
    fi
}

game_run() {
    local secs=120
    if [[ ${1:-} == -t ]]; then
        secs=$2
        shift 2
    fi
    local exe=$1
    shift
    if [ -z "${WINEPREFIX:-}" ]; then
        echo "game_run: WINEPREFIX is not set; the fake game runs only in a prefix of its test" >&2
        return 2
    fi
    local log loader= out marker before rc attempt watch after=$((secs > 10 ? secs - 5 : secs))
    log=$(dirname "$exe")/swofty/loader.log
    [ -f "$(dirname "$exe")/version.dll" ] && loader=1
    out=$(mktemp)
    for attempt in 1 2; do
        game_settle
        game_run_prefix
        marker=$(game_run_marker)
        [ -z "$marker" ] || rm -f "$marker"
        before=$(game_run_traces "$log" "$marker")
        (
            nap=
            trap 'kill ${nap:-} 2>/dev/null; exit 0' TERM
            sleep "$after" &
            nap=$!
            wait $nap
            trap '' TERM
            game_run_report "$exe" "$after" "$secs" >&$GAME_RUN_ERR
        ) >/dev/null 2>&1 &
        watch=$!
        rc=0
        timeout -k 5 "$secs" "${PIN[@]}" xvfb-run -a wine cmd /c "$exe" "$@" >"$out" 2>&1 || rc=$?
        kill $watch 2>/dev/null || :
        wait $watch 2>/dev/null || :
        game_settle
        if [[ $rc == 0 || $rc == 124 || $rc == 137 || -z $loader || $(game_run_traces "$log" "$marker") != "$before" ]]; then
            break
        fi
        if [[ $attempt == 1 ]]; then
            echo "game_run: retrying: wine start failure: $exe exited $rc and the loader wrote nothing; Wine printed:" >&$GAME_RUN_ERR
        else
            echo "game_run: wine start failure again: $exe exited $rc and the loader wrote nothing; Wine printed:" >&$GAME_RUN_ERR
        fi
        tail -n 20 "$out" | sed 's/^/  /' >&$GAME_RUN_ERR
    done
    cat "$out"
    rm -f "$out"
    return $rc
}
