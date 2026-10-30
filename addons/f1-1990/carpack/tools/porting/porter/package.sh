#!/usr/bin/env bash
# Copy a try's patched ERP to /mnt/work/.mp45b/out/mclaren.erp and build /mnt/work/.mp45b/out/mp45b-test.zip
# (layout of "F1 2021 Cars for F1 25.zip": mod.serps at the root, game files under one top folder).
# usage: package.sh <try tag>
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TAG="$1"; ZN="${2:-mp45b-test}"; EN="${3:-mclaren.erp}"; PKG="2025_asset_groups/f1_2025_vehicle_package/teams/mclaren/wep"
SRC="$HERE/tries/$TAG/F1 25/$PKG/mclaren.erp"
grep -q "SUCCESS" "$HERE/tries/$TAG/report.txt"
OUT=/mnt/work/.mp45b/out; mkdir -p "$OUT"
cp "$SRC" "$OUT/$EN"
cp "$HERE/tries/$TAG/report.txt" "$OUT/${EN%.erp}_patch_report.txt"
cp "$HERE/tries/$TAG/writer_log.txt" "$OUT/${EN%.erp}_writer_log.txt" 2>/dev/null || true
cp "$HERE/tries/$TAG/render_LOD0.png" "$OUT/${EN%.erp}_preview_LOD0.png" 2>/dev/null || true
echo "$TAG" > "$OUT/${EN%.erp}_SOURCE_TRY.txt"
ST=$(mktemp -d); NAME="MP4-5B test"
mkdir -p "$ST/$NAME/$PKG"
cp "$SRC" "$ST/$NAME/$PKG/mclaren.erp"
printf '{\n  "mod_name": "MP4-5B test",\n  "author": "private",\n  "version": "%s"\n}\n' "$TAG" > "$ST/mod.serps"
rm -f "$OUT/$ZN.zip"
(cd "$ST" && /nix/store/i4v40ki3sjh1a7kvgdnw6czs269ij2p2-zip-3.0/bin/zip -qr -X "$OUT/$ZN.zip" mod.serps "$NAME")
rm -rf "$ST"
unzip -l "$OUT/$ZN.zip"
sha256sum "$OUT/$EN"
