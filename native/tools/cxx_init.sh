#!/bin/sh
# cxx_init.sh REPORT COMPILE...: runs a core compile with -Wglobal-constructors. Its diagnostics, one per object with
# a dynamic initializer or a destructor that runs at exit, go to REPORT as "<diagnostic><TAB><declaration>" lines for
# tools/init_check.py; every other diagnostic reaches stderr as the compiler printed it.
report=$1
shift
"$@" -Wglobal-constructors 2>"$report.log"
rc=$?
awk -v report="$report" '
function flush(   i) { for (i = 0; i < held; i++) print hold[i] > "/dev/stderr"; held = 0 }
function close_pending() { if (pending != "") { print pending "\t" > report; pending = "" } }
BEGIN { printf "" > report }
/^In file included from / { hold[held++] = $0; next }
/: warning: declaration requires a global (constructor|destructor) \[-Wglobal-constructors\]$/ {
    close_pending(); held = 0; pending = $0; skipping = 1; next
}
skipping && /^ *[0-9]+ \| / {
    if (pending != "") { line = $0; sub(/^ *[0-9]+ \| /, "", line); print pending "\t" line > report; pending = "" }
    next
}
skipping && /^ *\| / { next }
{ close_pending(); skipping = 0 }
/^[0-9]+ warnings? generated\.$/ { held = 0; next }
{ flush(); print > "/dev/stderr" }
END { close_pending(); flush() }
' "$report.log"
rm -f "$report.log"
exit $rc
