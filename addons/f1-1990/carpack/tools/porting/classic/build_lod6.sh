#!/usr/bin/env bash
set -uo pipefail
PY=/nix/store/sfzmp6a4mkg45ajhwqk4i1xmaqz85hqs-python3-3.14.6-env/bin/python3
CL=/mnt/work/.mp45b/classic
AH=/mnt/work/.mp45b/porter_a
BT=/mnt/work/.mp45b/bspec_tex
LODS="--lods 0,1,2,3,4,5,5 --shadow-lod 3"
cd $CL
run() {
  local name=$1 car=$2 team=$3 zones=$4 btex=$5
  local z=""; [ -n "$zones" ] && z="--zones zones/$zones.json"
  taskset -c 2-7,10,11,14-31 $PY tools/classic_layout.py $car build/gridc13/$name --team $team --tex-out build/tex/$name --numbers $z $LODS > build/layout_gridc13_$name.log 2>&1 || { echo "LAYOUT FAIL $name"; return; }
  rm -rf $BT/$btex
  nix-shell -p "python3.withPackages(ps: [ps.pillow ps.numpy])" --run "python3 $AH/bspec_tex.py $CL/build/tex/$name $BT/$btex $team" > /dev/null 2>&1 || { echo "BSPEC FAIL $name"; return; }
  (cd $AH && FLAT_NORMALS=1 HARNESS_TEX_DIR=$CL/build/tex/$name taskset -c 2-7,10,11,14-31 timeout 900 ./tryteam.sh $team $CL/build/gridc13/$name gridc13_$name 2>&1 | grep -E "Patch ERP|LOD6" | sed "s/^/A $name: /")
  (cd $AH && FLAT_NORMALS=1 HARNESS_TEX_DIR=$BT/$btex taskset -c 2-7,10,11,14-31 timeout 900 ./tryteam.sh $team $CL/build/gridc13/$name gridc13b_$name 2>&1 | grep -E "Patch ERP|LOD6" | sed "s/^/B $name: /")
}
run mclaren c_mclaren_90 mclaren "" c_mclaren
run ferrari c_ferrari_90 ferrari "" c_ferrari
run williams c_williams_92 williams "" c_williams
run lotus c_benetton_94 lotus "" c_lotus
run mercedes c_jordan_91 mercedes "" c_mercedes
run redbull c_mclaren_91 redbull "" c_redbull
run march c_jordan_91 force_india march c7_march
run larrousse c_ferrari_90 williams larrousse c7_larrousse
run ligier_prost c_mclaren_90 sauber ligier_prost c7_ligier_prost
run minardi c_jordan_91 toro_rosso minardi c7_minardi
run brabham c_mclaren_91 gm_cadillac brabham c7_brabham
run dallara c_williams_92 haas dallara c7_dallara
run arrows c_ferrari_90 haas arrows c7_arrows
echo ALL_BUILT
