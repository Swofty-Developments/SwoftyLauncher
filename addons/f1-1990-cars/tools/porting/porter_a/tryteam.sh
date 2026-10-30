#!/usr/bin/env bash
# usage: tryteam.sh <team folder> <gltfFolder> <tag>   (2026 package; env HARNESS_TEX_DIR, FLAT_NORMALS)
set -euo pipefail
HERE=/mnt/work/.mp45b/porter; AH=/mnt/work/.mp45b/porter_a
TEAM="$1"; SRC="$(realpath "$2")"; TAG="$3"
T="$AH/tries/$TAG"; PKG="2025_asset_groups/f1_2026_vehicle_package/teams"
rm -rf "$T"; mkdir -p "$T/F1 25/$PKG/$TEAM/wep" "$T/input"
cp "$AH/stock26/F1 25/$PKG/common.erp" "$T/F1 25/$PKG/"
cp "$AH/stock26/F1 25/$PKG/$TEAM/wep/$TEAM.erp" "$T/F1 25/$PKG/$TEAM/wep/"
ERP="$T/F1 25/$PKG/$TEAM/wep/$TEAM.erp"
find "$SRC" -maxdepth 1 -type f -iname 'LOD*.glb' -exec cp {} "$T/input/" \;
"$HERE/tex_to_dds.sh" "$(realpath "$HARNESS_TEX_DIR")" "$T/dds" > "$T/dds.log" 2>&1
TMPW="$HERE/wineprefix/drive_c/users/$USER/AppData/Local/Temp/F1modding3dtoolV1"
(
  flock 9
  rm -f "$TMPW/_writer_log.txt"
  set +e
  HARNESS_PREFIX="$TEAM" HARNESS_TEX_DIR="$T/dds" "$AH/porter_b2.sh" patch "$ERP" "$T/input" full "$T/report.txt" > "$T/run.log" 2>&1
  echo "exit=$?" >> "$T/run.log"
  set -e
  cp "$TMPW/_writer_log.txt" "$T/writer_log.txt" 2>/dev/null || true
) 9>"$HERE/.lock"
grep -c "texture staging" "$T/run.log" | sed 's/^/textures staged: /'
head -1 "$T/report.txt"
grep -E "summary --|no stock descriptor|Height" "$T/report.txt" | head -8
