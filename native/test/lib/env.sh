# Sourced by the native test scripts: the one check that the tools a script needs are on PATH. NEEDS names them
# (default: zig); when one is missing and nix is at hand, the script reruns under nix shell with it. It also sets PIN
# (tools/pin.sh).
SELF=${SELF:-$(readlink -f "$0")}
declare -A _swofty_package=([zig]=nixpkgs#zig [c++]=nixpkgs#gcc [ninja]=nixpkgs#ninja
    [wine]=nixpkgs#wineWow64Packages.stable [xvfb-run]=nixpkgs#xvfb-run)
_swofty_missing=()
for _swofty_tool in ${NEEDS:-zig}; do
    command -v "$_swofty_tool" >/dev/null || _swofty_missing+=("${_swofty_package[$_swofty_tool]}")
done
if [ ${#_swofty_missing[@]} -gt 0 ] && command -v nix >/dev/null; then
    exec nix shell "${_swofty_missing[@]}" -c "$SELF" "$@"
fi
unset _swofty_package _swofty_missing _swofty_tool
. "$(dirname "${BASH_SOURCE[0]}")/../../tools/pin.sh"
