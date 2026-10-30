#!/usr/bin/env bash
# usage: package26.sh <try tag> <zip name> <mod name> [extra common.erp]
set -euo pipefail
TAG="$1"; ZN="$2"; MOD="$3"; COMMON="${4:-}"
OUT=/mnt/work/.mp45b/out; PK="2025_asset_groups/f1_2026_vehicle_package/teams"
SRC="${SRC_ERP:-/mnt/work/.mp45b/porter_a/tries/$TAG/F1 25/$PK/mclaren/wep/mclaren.erp}"
ST=$(mktemp -d -p /tmp/claude-1000/-home-swofty/623b806e-f751-4d86-a2ea-220667966158/scratchpad)
mkdir -p "$ST/$MOD/$PK/mclaren/wep"
cp "$SRC" "$ST/$MOD/$PK/mclaren/wep/mclaren.erp"
[[ -n "$COMMON" ]] && cp "$COMMON" "$ST/$MOD/$PK/common.erp"
if [[ -n "${GAME_VERSION:-}" ]]; then
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s",\n  "game_version": "%s"\n}\n' "$MOD" "$TAG" "$GAME_VERSION" > "$ST/mod.serps"
else
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s"\n}\n' "$MOD" "$TAG" > "$ST/mod.serps"
fi
rm -f "$OUT/$ZN.zip"
(cd "$ST" && /nix/store/i4v40ki3sjh1a7kvgdnw6czs269ij2p2-zip-3.0/bin/zip -qr -X "$OUT/$ZN.zip" mod.serps "$MOD")
rm -rf "$ST"
unzip -l "$OUT/$ZN.zip" | tail -n +4 | grep -v '/$' | head
