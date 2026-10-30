#!/usr/bin/env bash
# One-shot build: A's porter folder + textures (porter/tex_livery overlaid by A's porter_tex) -> try <tag> -> out/.
# usage: build_final.sh <tag> [variant folder under the car dir, default porter] [zip name] [erp name]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TAG="$1"; VAR="${2:-porter}"; CAR=/mnt/work/.mp45b/gltf/asr_1990_mclaren_mp4-5b
TEX="$HERE/tries/$TAG.tex"; rm -rf "$TEX"; mkdir -p "$TEX"
cp "$HERE"/tex_livery/* "$TEX"/ 2>/dev/null || true
for TD in "$CAR/tex_inject" "$CAR/porter_tex"; do
  [[ -d "$TD" ]] || continue
  for f in "$TD"/*; do
    stem="$(basename "${f%.*}")"; rm -f "$TEX/$stem".*; cp "$f" "$TEX/"
  done
done
HARNESS_TEX_DIR="$TEX" "$HERE/try_patch.sh" "$CAR/$VAR" full "$TAG"
"$HERE/package.sh" "$TAG" "${3:-mp45b-test}" "${4:-mclaren.erp}" | tail -2
