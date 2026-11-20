# Sourced by run_wine_test.sh. swofty_wineserver prints the wineserver of the Wine build that `wine` on PATH
# runs, or returns 1 when there is none. Distributions keep it off PATH (Debian and Ubuntu under /usr/lib/wine as
# wineserver or wineserver64), so it looks beside the binary `wine` resolves to through symlinks, beside `wine`
# itself, in SWOFTY_WINE_LIB_DIRS and on PATH, and takes the first whose version is the one `wine --version`
# reports, so a wineserver of another Wine build is never used. wineserver prints its version on stderr and wine
# on stdout, so both are read. native/test/wineserver_test.sh tests it.
SWOFTY_WINE_LIB_DIRS=${SWOFTY_WINE_LIB_DIRS:-/usr/lib/wine /usr/lib/x86_64-linux-gnu/wine /usr/lib64/wine /usr/local/lib/wine}

swofty_wine_version() {
    { "$@" --version 2>&1 || :; } | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?(-rc[0-9]+)?' | head -n 1 || :
}

swofty_wineserver() {
    local wine real want dir name candidate
    wine=$(command -v wine) || return 1
    real=$(readlink -f "$wine") || return 1
    want=$(swofty_wine_version "$wine")
    [ -n "$want" ] || return 1
    local candidates=()
    for dir in "$(dirname "$real")" "$(dirname "$wine")" $SWOFTY_WINE_LIB_DIRS; do
        for name in wineserver wineserver64; do
            candidates+=("$dir/$name")
        done
    done
    candidate=$(command -v wineserver) && candidates+=("$candidate")
    for candidate in "${candidates[@]}"; do
        if [ -f "$candidate" ] && [ -x "$candidate" ] && [ "$(swofty_wine_version "$candidate")" = "$want" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}
