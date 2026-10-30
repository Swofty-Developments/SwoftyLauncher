#!/usr/bin/env bash
# usage: package_mix.sh <zip name> <mod name> <default prefix> <team=prefix,...> [common.erp] [teams]   (env GAME_VERSION)
set -euo pipefail
ZN="$1"; MOD="$2"; DEF="$3"; MAP="$4"; COMMON="${5:-}"; TEAMS="${6:-mclaren ferrari williams lotus mercedes redbull force_india sauber toro_rosso gm_cadillac haas}"
OUT=/mnt/work/.mp45b/out; PK="2025_asset_groups/f1_2026_vehicle_package/teams"
ST=$(mktemp -d -p /tmp/claude-1000/-home-swofty/623b806e-f751-4d86-a2ea-220667966158/scratchpad)
declare -A P
IFS=',' read -ra kv <<< "$MAP"
for e in "${kv[@]}"; do [[ -n "$e" ]] && P[${e%%=*}]=${e##*=}; done
for t in $TEAMS; do
  pre=${P[$t]:-$DEF}
  grep -q SUCCESS "/mnt/work/.mp45b/porter_a/tries/${pre}_$t/report.txt"
  mkdir -p "$ST/$MOD/$PK/$t/wep"
  cp "/mnt/work/.mp45b/porter_a/tries/${pre}_$t/F1 25/$PK/$t/wep/$t.erp" "$ST/$MOD/$PK/$t/wep/$t.erp"
  echo "$t <- ${pre}_$t"
done
[[ -n "$COMMON" ]] && cp "$COMMON" "$ST/$MOD/$PK/common.erp"
if [[ -n "${GAME_VERSION:-}" ]]; then
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s",\n  "game_version": "%s"\n}\n' "$MOD" "$ZN" "$GAME_VERSION" > "$ST/mod.serps"
else
  printf '{\n  "mod_name": "%s",\n  "author": "private",\n  "version": "%s"\n}\n' "$MOD" "$ZN" > "$ST/mod.serps"
fi
rm -f "$OUT/$ZN.zip"
(cd "$ST" && /nix/store/i4v40ki3sjh1a7kvgdnw6czs269ij2p2-zip-3.0/bin/zip -qr -X "$OUT/$ZN.zip" mod.serps "$MOD")
rm -rf "$ST"
ls -la "$OUT/$ZN.zip"
