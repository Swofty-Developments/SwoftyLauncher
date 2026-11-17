#!/usr/bin/env bash
# Runs the track registries test on the programs native/test/manifest.py builds for it into the folder given as $1.
# Run it as native/test/run_tests.sh tracks, which builds them first.
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"
[ -n "${SWOFTY_TEST_GRAPH:-}" ] || exec ./run_tests.sh tracks
OUT=$1
python3 ../src/core/apply/tracks/track_keys.py --check
python3 ../src/core/apply/tracks/venues.py --check
GAMEDB="/mnt/gaming/SteamLibrary/steamapps/common/F1 25/game_package/gamedatabases/gamedatabases.2025.bin"
if [[ ${MAKE_FIXTURE:-} == 1 && -f $GAMEDB ]]; then
    "$OUT/lower_test" --make-fixture "$GAMEDB" tracks/fixture/tracks.tsv
fi
"$OUT/lower_test" tracks ../../packages/sdk/std/registries
