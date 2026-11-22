#!/usr/bin/env bash
# Host test of lib/game_run.sh with stand-in wine, wineserver and xvfb-run scripts first on PATH. The stand-in wine
# makes a prefix for wineboot and, for `wine cmd /c EXE`, takes the next line of a plan: start-fail exits 1 and
# writes nothing, as Wine does when it fails to start; exit N writes EXE's swofty\loader.log and the start marker
# (when the prefix has its folder), as the loader does, and exits N; marker N writes only the marker; quiet N exits
# N and writes nothing; sleep N waits N s and exits 0. Every call lands in a calls file.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir "$T/bin"
cat >"$T/bin/wine" <<'EOF'
#!/usr/bin/env bash
[ "${1:-}" = --version ] && { echo wine-10.0; exit 0; }
if [ "${1:-}" = wineboot ]; then
    mkdir -p "$WINEPREFIX/drive_c/users/Public" "$WINEPREFIX/drive_c/users/tester/AppData/Local"
    touch "$WINEPREFIX/system.reg"
    echo wineboot >>"$GR_CALLS"
    exit 0
fi
[ "${1:-} ${2:-}" = "cmd /c" ] || { echo "stand-in wine: want cmd /c, got $*" >>"$GR_CALLS"; exit 99; }
exe=$3
shift 3
action=$(head -n 1 "$GR_PLAN")
sed -i 1d "$GR_PLAN"
echo "run $action: $* FOO=${FOO:-}" >>"$GR_CALLS"
marker=$WINEPREFIX/drive_c/users/tester/AppData/Local/Swofty/state
case $action in
start-fail)
    echo "wine: could not start the program"
    exit 1
    ;;
exit*)
    mkdir -p "$(dirname "$exe")/swofty"
    echo attached >>"$(dirname "$exe")/swofty/loader.log"
    [ -d "$marker" ] && echo $$ >"$marker/loader-start.marker"
    echo "game output"
    exit "${action#exit }"
    ;;
marker*)
    echo $$ >"$marker/loader-start.marker"
    exit "${action#marker }"
    ;;
quiet*) exit "${action#quiet }" ;;
sleep*)
    sleep "${action#sleep }"
    exit 0
    ;;
esac
exit 98
EOF
cat >"$T/bin/wineserver" <<'EOF'
#!/bin/sh
[ "$1" = --version ] && { echo "Wine 10.0" >&2; exit 0; }
echo "server $1" >>"$GR_CALLS"
EOF
cat >"$T/bin/xvfb-run" <<'EOF'
#!/bin/sh
while [ "$1" = -a ]; do shift; done
exec "$@"
EOF
chmod +x "$T/bin/wine" "$T/bin/wineserver" "$T/bin/xvfb-run"
export GR_CALLS=$T/calls GR_PLAN=$T/plan
fails=0
ok() { echo "ok    $1"; }
fail() {
    echo "FAIL  $1"
    fails=$((fails + 1))
}

# scenario NAME PLAN... : a fresh game folder with version.dll and a prefix made by the stand-in wineboot.
scenario() {
    echo "==== $1"
    shift
    rm -rf "$T/game" "$T/prefix" "$T/calls" "$T/out" "$T/err"
    mkdir -p "$T/game/swofty"
    touch "$T/game/version.dll" "$T/game/F1_25.exe"
    mkdir -p "$T/prefix/drive_c/users/Public" "$T/prefix/drive_c/users/tester/AppData/Local"
    touch "$T/prefix/system.reg"
    printf '%s\n' "$@" >"$T/plan"
}

# play CODE [ARGS...]: game_run in a shell of its own; stdout to out, stderr to err, its exit code in rc.
play() {
    rc=0
    PATH="$T/bin:$PATH" WINEPREFIX=$T/prefix "$BASH" -c 'set -euo pipefail; PIN=(); . lib/game_run.sh; '"$1" \
        >"$T/out" 2>"$T/err" || rc=$?
}

want_rc() { [[ $rc == "$2" ]] && ok "$1" || fail "$1 (exit $rc, want $2)"; }
want_calls() {
    local got
    got=$(paste -sd '|' "$T/calls")
    [[ $got == "$2" ]] && ok "$1" || fail "$1 (calls '$got', want '$2')"
}
has() { grep -qE "$3" "$T/$2" && ok "$1" || fail "$1 ($2 has no '$3')"; }
lacks() { grep -qE "$3" "$T/$2" && fail "$1 ($2 has '$3')" || ok "$1"; }

SETTLE='server -k|server -w'

scenario "a clean run" "exit 0"
play 'game_run "'"$T"'/game/F1_25.exe" a "b c"'
want_rc "a clean run returns 0" 0
want_calls "the game runs once and the wineserver is left running" "run exit 0: a b c FOO="
has "the game's output reaches stdout" out '^game output$'
lacks "nothing is retried" err 'retrying'

scenario "an empty prefix" "exit 0"
rm -rf "$T/prefix"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "a run in an empty prefix returns 0" 0
want_calls "wineboot makes the prefix before the game runs" "wineboot|run exit 0:  FOO="

scenario "a Wine start failure" start-fail "exit 0"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "the second run's result is returned" 0
want_calls "the wineserver is stopped and waited for before the game runs once more" "run start-fail:  FOO=|$SETTLE|run exit 0:  FOO="
has "the retry is announced" err '^game_run: retrying: wine start failure: .*F1_25\.exe exited 1 and the loader wrote nothing; Wine printed:$'
has "Wine's output of the failed run is printed" err '^  wine: could not start the program$'
lacks "stdout has only the run that counts" out 'could not start'

scenario "two Wine start failures" start-fail start-fail
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "the second failure's exit code is returned" 1
want_calls "there is one retry, no more" "run start-fail:  FOO=|$SETTLE|run start-fail:  FOO=|$SETTLE"
has "the second failure is named" err '^game_run: wine start failure again: '

scenario "a game exit after the loader attached" "exit 5" "exit 0"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "the game's exit code is returned" 5
want_calls "a game result is never retried" "run exit 5:  FOO="

scenario "the start marker alone" "marker 7" "exit 0"
mkdir -p "$T/prefix/drive_c/users/tester/AppData/Local/Swofty/state"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "a run whose loader wrote only the start marker returns its exit code" 7
want_calls "the marker counts as the loader attached" "run marker 7:  FOO="

scenario "a stale start marker" start-fail "exit 0"
mkdir -p "$T/prefix/drive_c/users/tester/AppData/Local/Swofty/state"
echo 1 >"$T/prefix/drive_c/users/tester/AppData/Local/Swofty/state/loader-start.marker"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_calls "a marker left by an earlier run does not count" "run start-fail:  FOO=|$SETTLE|run exit 0:  FOO="

scenario "no loader beside the exe" start-fail "exit 0"
rm "$T/game/version.dll"
play 'game_run "'"$T"'/game/F1_25.exe"'
want_rc "an exe without the loader returns its exit code" 1
want_calls "an exe without the loader is never retried" "run start-fail:  FOO="

for code in 124 137; do
    scenario "a timeout ($code)" "quiet $code" "exit 0"
    play 'game_run "'"$T"'/game/F1_25.exe"'
    want_rc "a timeout returns $code" $code
    want_calls "a timeout is never retried" "run quiet $code:  FOO="
done

scenario "variables set for the call" "exit 0"
play 'FOO=bar game_run "'"$T"'/game/F1_25.exe"; [[ -z ${FOO:-} ]]'
want_rc "the variable is gone after the call" 0
want_calls "a variable set before game_run reaches the game" "run exit 0:  FOO=bar"

scenario "a redirected run" start-fail "exit 0"
play 'game_run "'"$T"'/game/F1_25.exe" >"'"$T"'/redirected" 2>&1'
has "the retry line reaches the script's stderr past the caller's redirect" err 'retrying: wine start failure'
grep -q '^game output$' "$T/redirected" && ok "the game's output follows the caller's redirect" ||
    fail "the game's output follows the caller's redirect"

scenario "no prefix" "exit 0"
rc=0
PATH="$T/bin:$PATH" "$BASH" -c 'set -euo pipefail; unset WINEPREFIX; PIN=(); . lib/game_run.sh; game_run "'"$T"'/game/F1_25.exe"' \
    >"$T/out" 2>"$T/err" || rc=$?
want_rc "game_run refuses to run without WINEPREFIX" 2
[[ ! -e $T/calls ]] && ok "nothing runs without WINEPREFIX" || fail "nothing runs without WINEPREFIX"

scenario "a run near its timeout" "sleep 7"
echo "a loader line" >"$T/game/swofty/loader.log"
play 'game_run -t 11 "'"$T"'/game/F1_25.exe"'
want_rc "a run that ends before its timeout returns 0" 0
has "6 s in, the run is reported" err "^game_run: .*F1_25\\.exe still runs 6 s after it started \\(its timeout is 11 s\\); the processes of $T/prefix:\$"
has "the report lists the prefix's processes" err '^  pid [0-9]+: .*bin/wine cmd /c '
has "the report lists their threads" err "^    thread [0-9]+ '[^']*' state [A-Z] in "
has "the report ends with the loader log" err '^game_run: the last lines of .*/game/swofty/loader\.log:$'

if ((fails)); then
    echo "game_run test: $fails FAILED"
    exit 1
fi
echo "game_run test: ALL PASSED"
