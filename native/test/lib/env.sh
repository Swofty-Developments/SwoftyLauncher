# Sourced by the native test scripts: the one check that the tools a script needs are on PATH. NEEDS names them
# (default: zig). When one is missing, the script reruns itself under nix shell with it. Without nix, when the rerun
# still lacks it, or when it is sourced from a shell rather than a script (which nix shell cannot rerun), it stops
# with a message naming what is missing. It also sets PIN (tools/pin.sh).
declare -A _swofty_package=([zig]=nixpkgs#zig [c++]=nixpkgs#gcc [ninja]=nixpkgs#ninja
    [wine]=nixpkgs#wineWow64Packages.stable [xvfb-run]=nixpkgs#xvfb-run)
_swofty_missing=() _swofty_packages=()
for _swofty_tool in ${NEEDS:-zig}; do
    if ! command -v "$_swofty_tool" >/dev/null; then
        _swofty_missing+=("$_swofty_tool")
        _swofty_packages+=("${_swofty_package[$_swofty_tool]:-nixpkgs#$_swofty_tool}")
    fi
done
if [ ${#BASH_SOURCE[@]} -lt 2 ] || [[ $- == *i* ]]; then
    if [ ${#_swofty_missing[@]} -gt 0 ]; then
        echo "native/test/lib/env.sh: ${_swofty_missing[*]} not on PATH. It reruns a test script under nix shell, which" \
            "it cannot do for a shell that sources it: run the script, or enter nix shell ${_swofty_packages[*]} first." >&2
        unset _swofty_package _swofty_missing _swofty_packages _swofty_tool
        return 1
    fi
else
    SELF=${SELF:-$(readlink -f "$0")}
    if [ ${#_swofty_missing[@]} -gt 0 ]; then
        if [ "${SWOFTY_ENV_RERUN:-}" != "$SELF" ] && command -v nix >/dev/null; then
            export SWOFTY_ENV_RERUN=$SELF
            exec nix shell "${_swofty_packages[@]}" -c "$SELF" "$@"
        fi
        if [ "${SWOFTY_ENV_RERUN:-}" = "$SELF" ]; then
            echo "$(basename "$SELF"): ${_swofty_missing[*]} still not on PATH under nix shell ${_swofty_packages[*]}." >&2
        else
            echo "$(basename "$SELF"): ${_swofty_missing[*]} not on PATH, and nix is not there to provide" \
                "${_swofty_packages[*]}. Install them or add them to PATH." >&2
        fi
        exit 1
    fi
fi
unset _swofty_package _swofty_missing _swofty_packages _swofty_tool
. "$(dirname "${BASH_SOURCE[0]}")/../../tools/pin.sh"
