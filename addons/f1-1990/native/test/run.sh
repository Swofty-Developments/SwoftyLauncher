#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
mkdir -p ../build
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/grid_rank_test grid_rank_test.cpp
../build/grid_rank_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/contract_target_test contract_target_test.cpp
../build/contract_target_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/fq_tyres_test fq_tyres_test.cpp
../build/fq_tyres_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -I../../../../packages/sdk/include -o ../build/profile_test profile_test.cpp
../build/profile_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/tile_logic_test tile_logic_test.cpp
../build/tile_logic_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/speech_line_test speech_line_test.cpp
../build/speech_line_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/json_strings_test json_strings_test.cpp
../build/json_strings_test ../../data/flavio-strings.json ../../data/rnd-art.json ../../data/rnd-strings.json ../../data/rnd-look.json \
    ../../data/session-strings.json ../../data/pu-strings.json
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/character_ref_test character_ref_test.cpp
../build/character_ref_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/content_tabs_test content_tabs_test.cpp
../build/content_tabs_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/pu_scope_test pu_scope_test.cpp
../build/pu_scope_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/pu_parts_test pu_parts_test.cpp
../build/pu_parts_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/rnd_progress_test rnd_progress_test.cpp
../build/rnd_progress_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/rnd_art_test rnd_art_test.cpp
../build/rnd_art_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/rnd_look_test rnd_look_test.cpp
../build/rnd_look_test ../../data/rnd-look.json
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/intro_video_test intro_video_test.cpp
../build/intro_video_test
${CXX:-g++} -std=c++20 -O1 -Wall -Wextra -I../src -o ../build/calendar_test calendar_test.cpp
../build/calendar_test ../../data/calendar-1990.json
