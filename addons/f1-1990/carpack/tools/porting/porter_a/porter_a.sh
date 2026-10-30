#!/usr/bin/env bash
# Runs Jefry's porter (shipped F1modding3dtoolV1.dll) headless under Wine via PorterHarness.dll.
# usage: porter.sh export <erp> <out.gltf>
#        porter.sh patch <erp> <gltfFolder> [full|vanilla] [report.txt] [cleanErp]
# Paths may be Linux paths; they are mapped to Z:\ for Wine.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WINE=/nix/store/3p54hkkvrxgf63vrnqnzd7xm721hzlng-wine-wow64-staging-11.14/bin/wine
export WINEPREFIX=/mnt/work/.mp45b/porter/wineprefix
export WINEDEBUG="${WINEDEBUG:--all}"
export WINEDLLOVERRIDES="mshtml="
export DOTNET_ROOT='C:\Program Files\dotnet'
export DOTNET_CLI_TELEMETRY_OPTOUT=1
if [[ -n "${HARNESS_TEX_DIR:-}" && "$HARNESS_TEX_DIR" == /* ]]; then export HARNESS_TEX_DIR="Z:$(echo "$HARNESS_TEX_DIR" | tr / '\\')"; fi
export WINEDEBUG
unset WAYLAND_DISPLAY
XVFB=/nix/store/90pgwg2ighm77ic1h5wjhyfbbvri9rqd-xvfb-21.1.24/bin/Xvfb
DNUM="${HARNESS_DISPLAY_NUM:-92}"
if [[ ! -S /tmp/.X11-unix/X$DNUM ]]; then
  (setsid taskset -c 2-7,10,11,14-31 "$XVFB" ":$DNUM" -screen 0 1280x1024x24 -nolisten tcp 9>&- >/dev/null 2>&1 &)
  for i in $(seq 1 50); do [[ -S /tmp/.X11-unix/X$DNUM ]] && break; sleep 0.1; done
fi
export DISPLAY=":$DNUM"
args=()
for a in "$@"; do
  if [[ "$a" == /* ]]; then args+=("Z:$(echo "$a" | tr / '\\')"); else args+=("$a"); fi
done
exec taskset -c 2-7,10,11,14-31 "$WINE" "C:\\Program Files\\dotnet\\dotnet.exe" "$HERE/run/PorterHarnessA.dll" "${args[@]}"
