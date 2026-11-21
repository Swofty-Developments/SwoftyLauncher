#!/usr/bin/env bash
set -euo pipefail
# The loader's session environment file (swofty\session_env.json, native/include/session_env.h) under Wine: a fake
# F1_25.exe that imports version.dll writes the variables it sees, and the loader's log says what the loader did.
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
NEEDS="zig wine xvfb-run"
. "$(dirname "$SELF")/lib/env.sh"
. ./game_lock.sh
OUT=${1:-$PWD/../build/wine-session-env}
export ZIG_GLOBAL_CACHE_DIR=$PWD/../build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/../build/zig-cache
GAME=$OUT/game
[[ -d $GAME/swofty ]] && chmod u+w "$GAME/swofty"
rm -rf "$GAME"
mkdir -p "$GAME/swofty"
"${PIN[@]}" zig cc -target x86_64-windows-gnu -O2 -o "$GAME/F1_25.exe" fake_session_env.c -lversion
cp ../dist/version.dll "$GAME/"
export WINEPREFIX="$OUT/prefix" WINEDLLOVERRIDES="version=n,b;mscoree,mshtml=;winemenubuilder.exe,winedbg.exe=d" WINEDEBUG=-all
LOCAL=$(winepath -u "$(wine cmd /c 'echo %LOCALAPPDATA%' | tr -d '\r')")
FALLBACK=$LOCAL/Swofty/state/loader-fallback.log
FILE=$GAME/swofty/session_env.json
LOG=$GAME/swofty/loader.log
SEEN=$GAME/seen.txt
TOKEN=tok-7Qx9Lm2Za8vR
status=0
pass() { echo "ok  : $1"; }
fail() { echo "FAIL: $1"; status=1; }
has() { if grep -qE "$3" "$2" 2>/dev/null; then pass "$1"; else fail "$1"; fi; }
lacks() { if grep -qE "$3" "$2" 2>/dev/null; then fail "$1"; else pass "$1"; fi; }
gone() { if [[ -e $2 ]]; then fail "$1"; else pass "$1"; fi; }
kept() { if [[ -e $2 ]]; then pass "$1"; else fail "$1"; fi; }
run() {
    rm -f "$SEEN"
    [[ -w $GAME/swofty ]] && rm -f "$LOG" "${LOG%.log}.prev.log"
    (cd "$GAME" && timeout 60 "${PIN[@]}" xvfb-run -a wine "$GAME/F1_25.exe" SWOFTY_T_A SWOFTY_T_B SWOFTY_T_C SWOFTY_LAUNCHER_SAVES SWOFTY_T_D) ||
        fail "the fake game did not exit cleanly"
}
# write WRITTEN EXPIRES [SCHEMA [ASCII]]: the file, with its window as seconds from now.
write() {
    python3 - "$FILE" "$1" "$2" "${3:-1}" "${4:-}" "$TOKEN" <<'EOF'
import json, sys, time
path, written, expires, schema, ascii_only, token = sys.argv[1:]
now = int(time.time())
doc = {'schema': int(schema), 'request': '5f3a9c1e', 'written': now + int(written), 'expires': now + int(expires),
       'env': {'SWOFTY_T_A': token, 'SWOFTY_T_B': 'quote " back \\ slash ü ✓ \U0001F600',
               'SWOFTY_T_C': '{"json":[1,2]}', 'SWOFTY_LAUNCHER_SAVES': 'off'}}
open(path, 'w', encoding='utf-8').write(json.dumps(doc, ensure_ascii=bool(ascii_only)))
EOF
}
want_values() {
    python3 - "$SEEN" "$TOKEN" "$1" <<'EOF'
import sys
seen, token, what = sys.argv[1:]
lines = open(seen, encoding='utf-8').read().splitlines() if __import__('os').path.exists(seen) else []
want = ['SWOFTY_T_A=' + token, 'SWOFTY_T_B=quote " back \\ slash ü ✓ \U0001F600', 'SWOFTY_T_C={"json":[1,2]}',
        'SWOFTY_LAUNCHER_SAVES=off', 'SWOFTY_T_D unset']
ok = lines == want
print(f'{"ok  " if ok else "FAIL"}: {what}')
if not ok:
    print('  saw: ' + repr(lines))
sys.exit(not ok)
EOF
}
want_unset() {
    if [[ $(cat "$SEEN" 2>/dev/null) == $'SWOFTY_T_A unset\nSWOFTY_T_B unset\nSWOFTY_T_C unset\nSWOFTY_LAUNCHER_SAVES unset\nSWOFTY_T_D unset' ]]; then
        pass "$1"
    else
        fail "$1 (saw: $(tr '\n' ' ' <"$SEEN" 2>/dev/null))"
    fi
}

echo "==== no file"
run
want_unset "a start without the file sees none of the variables"
has "the loader attached" "$LOG" '\(version\.dll proxy\) attached'
lacks "and logs nothing about a session environment" "$LOG" 'session env'

echo "==== a file written for this start"
write -5 1015
run
want_values "the game sees each variable as the file names it" || status=1
has "the loader names how many it set and the request" "$LOG" \
    'session env: set 4 of 4 variable\(s\) from swofty\\session_env\.json \(launch request 5f3a9c1e\); removed the file'
lacks "the loader never logs a value" "$LOG" "$TOKEN|\"json\":\\[1,2\\]|back \\\\ slash"
gone "the file is gone after the start" "$FILE"
run
want_unset "a second start sees none of the variables"

echo "==== a file with \\u escapes"
write -5 1015 1 ascii
run
want_values "escaped values decode to the same variables" || status=1
gone "the file is gone after the start" "$FILE"

echo "==== a file an earlier session left"
write -2000 -980
run
want_unset "an expired file arms nothing"
has "the loader says why it ignored it" "$LOG" \
    'session env: swofty\\session_env\.json ignored: it expired before this game started \(9[0-9]{2} s before\)'
lacks "and logs no value" "$LOG" "$TOKEN"
kept "the file stays for the launcher to remove" "$FILE"

echo "==== a file written for a later start"
write 600 1600
run
want_unset "a file written after this game started arms nothing"
has "the loader says why" "$LOG" 'ignored: it was written after this game started, for another start'

echo "==== a file in another layout"
write -5 1015 2
run
want_unset "another schema arms nothing"
has "the loader says why" "$LOG" 'ignored: it has a schema this loader does not read'
printf '{"schema":1,"request":"5f3a9c1e","written":1,' >"$FILE"
run
want_unset "a cut-off file arms nothing"
has "the loader says why" "$LOG" 'ignored: it is not JSON in the layout the launcher writes'
rm -f "$FILE"

echo "==== a game folder the game may not change"
# Windows refuses the delete with error 5. Wine reports it done and leaves the file, which the loader checks for.
write -5 1015
rm -f "$FALLBACK" "${FALLBACK%.log}.prev.log" "$LOG" "${LOG%.log}.prev.log"
chmod a-w "$GAME/swofty"
if [[ -w $GAME/swofty ]]; then
    echo "skip: this user writes in read-only folders (root?), so the refusal cannot be staged"
    chmod u+w "$GAME/swofty"
else
    run
    chmod u+w "$GAME/swofty"
    want_values "the game sees the variables" || status=1
    has "the loader says the file stays" "$FALLBACK" \
        'session env: set 4 of 4 variable\(s\) from swofty\\session_env\.json \(launch request 5f3a9c1e\); the file (could not be removed \(error 5\)|is still there after the delete), so the launcher removes it when the session ends'
    kept "the file is still there" "$FILE"
    lacks "and the fallback log has no value" "$FALLBACK" "$TOKEN"
fi
rm -f "$FILE"

[[ $status == 0 ]] && echo "session env test: ALL PASSED" || { echo "session env test: FAILED"; tail -20 "$LOG" 2>/dev/null; }
exit $status
