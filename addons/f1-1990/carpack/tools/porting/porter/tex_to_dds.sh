#!/usr/bin/env bash
# Convert a folder of replacement textures into DDS files the porter can inject.
# usage: tex_to_dds.sh <in dir> <out dir>
#   <stem>.png/.tga/.jpg -> <stem>.dds, power-of-two size (max 4096), full mip chain;
#   BC3 (DXT5) when the stem ends in _da/_a, or has alpha and is not an albedo (_d); else BC1 (DXT1), alpha dropped. Existing .dds are copied as-is.
#   FLAT_NORMALS=1: for every replaced <base>_d also write a flat <base>_n (stock normal maps do not match new UVs).
set -euo pipefail
IN="$1"; OUT="$2"; mkdir -p "$OUT"
shopt -s nullglob nocaseglob
for f in "$IN"/*.dds; do cp "$f" "$OUT/"; done
for f in "$IN"/*.png "$IN"/*.tga "$IN"/*.jpg; do
  stem="$(basename "${f%.*}")"
  read -r w h opaque < <(magick "$f" -format '%w %h %[opaque]\n' info: | head -1)
  p2() { local v=$1 r=1; while (( r * 2 <= v && r < 4096 )); do r=$((r * 2)); done; echo $r; }
  W=$(p2 "$w"); H=$(p2 "$h"); M=$W; (( H > M )) && M=$H
  mips=0; while (( (1 << mips) < M )); do mips=$((mips + 1)); done
  comp=dxt1; alpha=(-alpha off)
  if [[ "$stem" == *_da || "$stem" == *_a ]]; then comp=dxt5; alpha=();
  elif [[ "$stem" != *_d && ( "$opaque" == "False" || "$opaque" == "false" ) ]]; then comp=dxt5; alpha=(); fi
  magick "$f" "${alpha[@]}" -resize "${W}x${H}!" -define dds:compression=$comp -define dds:mipmaps=$mips "$OUT/$stem.dds"
  echo "$stem.dds  ${w}x${h} -> ${W}x${H} $comp mips=$((mips + 1))"
  if [[ "${FLAT_NORMALS:-0}" == "1" && "$stem" == *_d ]]; then
    n="${stem%_d}_n"
    [[ -e "$IN/$n.png" || -e "$IN/$n.dds" ]] || { magick -size 64x64 xc:'rgb(128,128,255)' -define dds:compression=dxt1 -define dds:mipmaps=6 "$OUT/$n.dds"; echo "$n.dds  flat normal 64x64"; }
  fi
done
