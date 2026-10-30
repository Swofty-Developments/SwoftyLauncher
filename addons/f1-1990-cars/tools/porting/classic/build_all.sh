#!/usr/bin/env bash
set -uo pipefail
PY=/nix/store/sfzmp6a4mkg45ajhwqk4i1xmaqz85hqs-python3-3.14.6-env/bin/python3
CL=/mnt/work/.mp45b/classic
cd $CL
for pair in c_ferrari_90:ferrari c_williams_92:williams c_mclaren_91:redbull c_jordan_91:mercedes c_benetton_94:lotus c_mclaren_90:mclaren; do
  car=${pair%%:*}; team=${pair##*:}
  for variant in gridc gridc13; do
    extra="--rim-radius 0.248"; [ $variant = gridc13 ] && extra=""
    taskset -c 2-7,10,11,14-31 $PY tools/classic_layout.py $car build/$variant/$team --team $team --tex-out build/tex/$team $extra --numbers > build/layout_${variant}_$team.log 2>&1 || { echo "LAYOUT FAIL $variant $team"; continue; }
    (cd /mnt/work/.mp45b/porter_a && FLAT_NORMALS=1 HARNESS_TEX_DIR=$CL/build/tex/$team timeout 900 ./tryteam.sh $team $CL/build/$variant/$team ${variant}_$team 2>&1 | grep -E "Patch ERP|textures staged|LOD0 summary" | sed "s/^/$variant $team: /")
  done
done
echo ALL_BUILT
