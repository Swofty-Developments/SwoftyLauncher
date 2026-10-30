#!/usr/bin/env bash
set -uo pipefail
P=$(cat /tmp/claude-1000/-home-swofty/623b806e-f751-4d86-a2ea-220667966158/scratchpad/pyenv.path)/bin/python3
C="/tmp/claude-1000/-home-swofty/623b806e-f751-4d86-a2ea-220667966158/scratchpad/mp45b/ASR - 1990 - McLaren MP4-5B - V1.1/assettocorsa/content/cars"
v=asr_1990_mclaren_mp4-5b; G=/mnt/work/.mp45b/gltf/$v
cd /mnt/work/.mp45b/kn5
for t in mclaren ferrari williams lotus mercedes redbull force_india sauber toro_rosso gm_cadillac haas; do
  for variant in gridn gridn13; do
    extra="--rim-radius 0.248"; [ $variant = gridn13 ] && extra=""
    mkdir -p $G/$variant
    taskset -c 2-7,10,11,14-31 $P porter_layout.py "$C/$v" $G/$variant/$t --donor 2026 --donor-json /mnt/work/.mp45b/porter_a/donors26.json --team $t $extra --tex-dir $G/textures/png --tex-out $G/porter_tex --numbers > /dev/null 2>$G/$variant/err_$t.txt || echo "LAYOUT FAIL $variant $t"
    (cd /mnt/work/.mp45b/porter_a && FLAT_NORMALS=1 HARNESS_TEX_DIR=$G/grid26/tex_$t timeout 900 ./tryteam.sh $t $G/$variant/$t ${variant}_$t 2>&1 | grep -E "Patch ERP|LOD0 summary" | sed "s/^/$variant $t: /")
  done
done
echo ALL_BUILT
