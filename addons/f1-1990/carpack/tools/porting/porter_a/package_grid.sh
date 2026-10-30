#!/usr/bin/env bash
# usage: package_grid.sh <try prefix e.g. grid13> <zip name> <mod name> [common.erp]   (env GAME_VERSION optional)
set -euo pipefail
PREFIX="$1"; ZN="$2"; MOD="$3"; COMMON="${4:-}"
OUT=/mnt/work/.mp45b/out; PK="2025_asset_groups/f1_2026_vehicle_package/teams"
ST=$(mktemp -d -p /tmp/claude-1000/-home-swofty/623b806e-f751-4d86-a2ea-220667966158/scratchpad)
for t in mclaren ferrari williams lotus mercedes redbull force_india sauber toro_rosso gm_cadillac haas; do
  grep -q SUCCESS "/mnt/work/.mp45b/porter_a/tries/${PREFIX}_$t/report.txt"
  mkdir -p "$ST/$MOD/$PK/$t/wep"
  cp "/mnt/work/.mp45b/porter_a/tries/${PREFIX}_$t/F1 25/$PK/$t/wep/$t.erp" "$ST/$MOD/$PK/$t/wep/$t.erp"
done
[[ -n "$COMMON" ]] && cp "$COMMON" "$ST/$MOD/$PK/common.erp"
if [[ -n "${GAME_VERSION:-}" ]]; then
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s",\n  "game_version": "%s"\n}\n' "$MOD" "$PREFIX" "$GAME_VERSION" > "$ST/mod.serps"
else
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s"\n}\n' "$MOD" "$PREFIX" > "$ST/mod.serps"
fi
rm -f "$OUT/$ZN.zip"
(cd "$ST" && /nix/store/i4v40ki3sjh1a7kvgdnw6czs269ij2p2-zip-3.0/bin/zip -qr -X "$OUT/$ZN.zip" mod.serps "$MOD")
rm -rf "$ST"
unzip -l "$OUT/$ZN.zip" | grep -E '\.erp|serps' | awk '{print $1, $4, $5, $6, $7, $8, $9, $10}'
ls -la "$OUT/$ZN.zip"
