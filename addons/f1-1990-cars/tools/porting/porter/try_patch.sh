#!/usr/bin/env bash
# Patch a copy of the stock 2025 McLaren ERP from a glTF LOD folder with Jefry's porter (headless, Wine).
# usage: try_patch.sh <gltfFolder> [full|vanilla] [tag]
# Output: /mnt/work/.mp45b/porter/tries/<tag>/{mclaren.erp (inside F1 25/... tree), report.txt, writer_log.txt, run.log}
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$(realpath "$1")"; MODE="${2:-full}"; TAG="${3:-$(date +%H%M%S)}"
T="$HERE/tries/$TAG"; PKG="2025_asset_groups/f1_2025_vehicle_package/teams"
mkdir -p "$T/F1 25/$PKG/mclaren/wep"
cp "$HERE/stock/F1 25/$PKG/common.erp" "$HERE/stock/F1 25/$PKG/common_fom.erp" "$T/F1 25/$PKG/"
cp "$HERE/stock/F1 25/$PKG/mclaren/wep/mclaren.erp" "$T/F1 25/$PKG/mclaren/wep/"
ERP="$T/F1 25/$PKG/mclaren/wep/mclaren.erp"
mkdir -p "$T/input"
find "$SRC" -maxdepth 1 -type f \( -iname '*.gltf' -o -iname '*.glb' -o -iname '*.bin' -o -iname '*.json' \) -exec cp {} "$T/input/" \;
SRC="$T/input"
if [[ -n "${HARNESS_TEX_DIR:-}" ]]; then
  "$HERE/tex_to_dds.sh" "$(realpath "$HARNESS_TEX_DIR")" "$T/dds" > "$T/dds.log" 2>&1 || { echo "texture conversion failed, see $T/dds.log"; exit 1; }
  export HARNESS_TEX_DIR="$T/dds"
fi
TMPW="$HERE/wineprefix/drive_c/users/$USER/AppData/Local/Temp/F1modding3dtoolV1"
(
  flock 9
  rm -f "$TMPW/_writer_log.txt"
  set +e
  "$HERE/porter.sh" patch "$ERP" "$SRC" "$MODE" "$T/report.txt" > "$T/run.log" 2>&1
  echo "exit=$?" >> "$T/run.log"
  set -e
  cp "$TMPW/_writer_log.txt" "$T/writer_log.txt" 2>/dev/null || true
) 9>"$HERE/.lock"
grep '^\[harness\]' "$T/run.log" | grep -v 'extracted idf' || true
grep -E 'tex-inject|SKIPPED|\.dds ->' "$T/report.txt" 2>/dev/null | cut -c1-200 || true
if [[ -f "$T/report.txt" ]]; then
  head -6 "$T/report.txt" | sed -n '1p;6p'
  grep -E -i 'summary --|height|no stock descriptor|dropped|collision|fail|error|not found|anchor sources|preserve-origin|Parsed glTF|New size|ERP rewritten' "$T/report.txt" | cut -c1-300 | head -60
fi
if [[ "${VERIFY:-1}" == "1" && -f "$T/report.txt" ]] && grep -q "SUCCESS" "$T/report.txt"; then
  mkdir -p "$T/reexport"
  ( flock 9; "$HERE/porter.sh" export "$ERP" "$T/reexport/mclaren.gltf" > "$T/reexport.log" 2>&1 ) 9>"$HERE/.lock"
  PY=/nix/store/wydrx87bj21vqbawan4856kfp3m1zipx-python3-3.14.6-env/bin/python3.14
  for f in "$T"/input/LOD[0-7].gl*; do
    l=$(basename "$f" | sed -E 's/^LOD([0-9]).*/\1/')
    [[ -f "$T/reexport/mclaren_LOD$l.gltf" ]] || continue
    "$PY" "$HERE/tools/bbox_cmp.py" "$f" "$T/reexport/mclaren_LOD$l.gltf" > "$T/bbox_LOD$l.txt" 2>&1 || true
  done
  RPY=/nix/store/cpf8s36h7521xhb08rsd8qj9hhvg5bw9-python3-3.14.6-env/bin/python3.14
  taskset -c 2-7,10,11,14-31 "$RPY" "$HERE/tools/render_textured.py" "$T/reexport/mclaren_LOD0.gltf" "$T/render_LOD0.png" "$TAG LOD0, re-export of the patched ERP" > /dev/null 2>&1 && echo "preview render: $T/render_LOD0.png"
  echo "verify (input vs re-export of the patched ERP, per-part world bbox; parts off by > 5 mm):"
  for b in "$T"/bbox_LOD*.txt; do echo "  $(basename "$b" .txt | sed 's/bbox_//'): $(grep -v 'maxdiff 0.00[0-4]' "$b" | grep -v 'only in B' | awk '{print $1}' | tr '\n' ' ')"; done
fi
echo "erp: $ERP"
echo "report: $T/report.txt  writer log: $T/writer_log.txt"
