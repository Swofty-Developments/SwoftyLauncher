#!/bin/sh
# cxx_init.sh REPORT SOURCE COMPILE...: runs a core compile of SOURCE with -Wglobal-constructors. REPORT starts with
# "# sha256 <hash>", the hash of SOURCE as the compile starts, so tools/init_check.py can tell a source edited during
# the build. The diagnostics, one per object with a dynamic initializer or a destructor that runs at exit, follow as
# "<diagnostic><TAB><declaration><TAB><macro>" lines, where <macro> is the innermost macro the declaration is spelled in
# (the last "expanded from macro" note) or empty, and their notes are dropped; every other diagnostic reaches stderr
# as the compiler printed it.
report=$1
source=$2
shift 2
hash=$(sha256sum < "$source") || exit 1
"$@" -Wglobal-constructors 2>"$report.log"
rc=$?
awk -v report="$report" -v hash="${hash%% *}" '
function flush(   i) { for (i = 0; i < held; i++) print hold[i] > "/dev/stderr"; held = 0 }
function close_pending() { if (pending != "") { print pending "\t" decl "\t" macro > report; pending = "" } }
BEGIN { print "# sha256 " hash > report }
/^In file included from / { hold[held++] = $0; next }
/: warning: declaration requires a global (constructor|destructor) \[-Wglobal-constructors\]$/ {
    close_pending(); held = 0; pending = $0; decl = ""; macro = ""; in_note = 0; skipping = 1; next
}
skipping && /^ *[0-9]+ \| / {
    if (decl == "" && !in_note) { decl = $0; sub(/^ *[0-9]+ \| /, "", decl) }
    next
}
skipping && /^ *\| / { next }
skipping && /: note: / {
    held = 0; in_note = 1
    if (match($0, /expanded from macro '\''[^'\'']+'\''/)) macro = substr($0, RSTART + 21, RLENGTH - 22)
    next
}
{ close_pending(); skipping = 0 }
/^[0-9]+ warnings? generated\.$/ { held = 0; next }
{ flush(); print > "/dev/stderr" }
END { close_pending(); flush() }
' "$report.log"
rm -f "$report.log"
exit $rc
