#!/usr/bin/env bash
# Host test of lib/wineserver.sh with stand-in wine and wineserver scripts in a temporary folder: a wineserver
# beside wine on PATH, one beside the binary a wine symlink points to, one under a distribution's lib folder, none,
# and ones of another Wine version, which are never taken. The stand-in wineservers print their version on stderr,
# as Wine's does (server/main.c), and wine on stdout.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir "$T/tools"
for tool in grep head readlink dirname; do
    ln -s "$(command -v "$tool")" "$T/tools/$tool"
done
fails=0

fake() {
    mkdir -p "$(dirname "$1")"
    printf '#!/bin/sh\n[ "$1" = --version ] && echo "%s"\n' "$2" >"$1"
    chmod +x "$1"
}

fake_server() {
    mkdir -p "$(dirname "$1")"
    printf '#!/bin/sh\n[ "$1" = --version ] && echo "%s" >&2\n' "$2" >"$1"
    chmod +x "$1"
}

expect() {
    local what=$1 want=$2 got status=0
    got=$(PATH="$3:$T/tools" SWOFTY_WINE_LIB_DIRS="${4:-$T/none}" "$BASH" -c 'set -euo pipefail; . lib/wineserver.sh; swofty_wineserver') || status=$?
    if [[ $want == none && $status == 1 && -z $got ]] || [[ $want != none && $status == 0 && $got == "$want" ]]; then
        echo "ok    $what"
    else
        echo "FAIL  $what (got '$got', status $status, want $want)"
        fails=$((fails + 1))
    fi
}

fake "$T/path/bin/wine" "wine-10.0"
fake_server "$T/path/bin/wineserver" "Wine 10.0"
expect "a wineserver beside wine on PATH" "$T/path/bin/wineserver" "$T/path/bin"

fake "$T/nix/lib/wine/wine" "wine-10.0 (Staging)"
fake_server "$T/nix/lib/wine/wineserver" "Wine 10.0"
mkdir -p "$T/nix/bin"
ln -s ../lib/wine/wine "$T/nix/bin/wine"
expect "a wineserver beside the binary a wine symlink points to" "$T/nix/lib/wine/wineserver" "$T/nix/bin"

fake "$T/deb/bin/wine" "wine-10.0 (Ubuntu 10.0~repack-12ubuntu1)"
fake_server "$T/deb/lib/wine/wineserver64" "Wine 10.0"
expect "wineserver64 under a distribution's lib folder" "$T/deb/lib/wine/wineserver64" "$T/deb/bin" "$T/nowhere $T/deb/lib/wine"

fake "$T/bare/bin/wine" "wine-10.0"
expect "none when no wineserver is found" none "$T/bare/bin"

fake_server "$T/old/bin/wineserver" "Wine 9.0"
expect "none when the only wineserver is another version" none "$T/bare/bin:$T/old/bin"
fake_server "$T/new/lib/wine/wineserver" "Wine 10.0"
expect "the wineserver of wine's version, past one of another version on PATH" "$T/new/lib/wine/wineserver" \
    "$T/bare/bin:$T/old/bin" "$T/new/lib/wine"

fake "$T/mixed/bin/wine" "wine-10.0"
fake_server "$T/mixed/bin/wineserver" "Wine 9.22"
expect "none when the wineserver beside wine is another version" none "$T/mixed/bin"

mkdir -p "$T/silent/bin"
printf '#!/bin/sh\nexit 1\n' >"$T/silent/bin/wine"
chmod +x "$T/silent/bin/wine"
fake_server "$T/silent/bin/wineserver" "Wine 10.0"
expect "none when wine reports no version" none "$T/silent/bin"

expect "none without wine" none "$T/path/nothing"

[ $fails -eq 0 ] && echo "wineserver: all passed" || { echo "wineserver: $fails failed"; exit 1; }
