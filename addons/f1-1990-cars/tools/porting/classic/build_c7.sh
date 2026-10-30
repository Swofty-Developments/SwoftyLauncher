#!/usr/bin/env bash
set -uo pipefail
PY=/nix/store/sfzmp6a4mkg45ajhwqk4i1xmaqz85hqs-python3-3.14.6-env/bin/python3
CL=/mnt/work/.mp45b/classic
AH=/mnt/work/.mp45b/porter_a
cd $CL
for spec in "${@:-march:c_jordan_91:force_india larrousse:c_ferrari_90:williams ligier_prost:c_mclaren_90:sauber minardi:c_jordan_91:toro_rosso brabham:c_mclaren_91:gm_cadillac dallara:c_williams_92:haas arrows:c_ferrari_90:haas}"; do
  for item in $spec; do
    IFS=: read name car team <<< "$item"
    taskset -c 2-7,10,11,14-31 $PY tools/classic_layout.py $car build/gridc13/$name --team $team --tex-out build/tex/$name --numbers --zones zones/$name.json > build/layout_gridc13_$name.log 2>&1 || { echo "LAYOUT FAIL $name"; continue; }
    rm -rf /mnt/work/.mp45b/bspec_tex/c7_$name
    nix-shell -p "python3.withPackages(ps: [ps.pillow ps.numpy])" --run "python3 $AH/bspec_tex.py $CL/build/tex/$name /mnt/work/.mp45b/bspec_tex/c7_$name $team" > /dev/null 2>&1 || { echo "BSPEC FAIL $name"; continue; }
    (cd $AH && FLAT_NORMALS=1 HARNESS_TEX_DIR=$CL/build/tex/$name taskset -c 2-7,10,11,14-31 timeout 900 ./tryteam.sh $team $CL/build/gridc13/$name gridc13_$name 2>&1 | grep -E "textures staged|SUCCESS|FAIL|summary" | sed "s/^/A $name: /")
    (cd $AH && FLAT_NORMALS=1 HARNESS_TEX_DIR=/mnt/work/.mp45b/bspec_tex/c7_$name taskset -c 2-7,10,11,14-31 timeout 900 ./tryteam.sh $team $CL/build/gridc13/$name gridc13b_$name 2>&1 | grep -E "textures staged|SUCCESS|FAIL|summary" | sed "s/^/B $name: /")
  done
done
echo ALL_BUILT
