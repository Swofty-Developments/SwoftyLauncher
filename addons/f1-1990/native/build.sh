#!/usr/bin/env bash
set -euo pipefail
SELF=$(readlink -f "$0")
cd "$(dirname "$SELF")"

if command -v zig >/dev/null; then
    ZIG=(zig)
else
    ZIG=(nix run nixpkgs#zig --)
fi

export ZIG_GLOBAL_CACHE_DIR=$PWD/build/zig-cache ZIG_LOCAL_CACHE_DIR=$PWD/build/zig-cache
mkdir -p build

REPO=../../..
SDK=${SDK_INCLUDE:-$REPO/packages/sdk/include}
GAME=${GAME_INCLUDE:-$REPO/native/include}
CXX=("${ZIG[@]}" c++ -target x86_64-windows-gnu -std=c++23)
SRC=(src/plugin.cpp src/grid_best_of.cpp src/contract_skip.cpp)

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990.dll "${SRC[@]}"

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_tyres.dll src/fq_tyres.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_tyreclass.dll src/tyre_class.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_replay.dll src/replay_holders.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_ui.dll src/career_menu_ui.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_voice.dll src/agent_voice.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_bspec.dll src/bspec.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_agent.dll src/agent_character.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_tabs.dll src/driver_select_tabs.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_pu.dll src/pu_rules.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_parts.dll src/pu_parts.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_quali.dll src/quali_engine.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_rnd.dll src/rnd_ui.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_rndlook.dll src/rnd_look.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_wizard.dll src/career_wizard.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_intro.dll src/intro_video.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_sessions.dll src/session_names.cpp

"${CXX[@]}" -O2 -Wall -Wextra -Wno-nullability-completeness -I"$GAME" -I$SDK -D__USE_MINGW_ANSI_STDIO=1 -shared -s \
    -o f1_1990_qualilaps.dll src/quali_laps.cpp

rm -f f1_1990.lib f1_1990_tyres.lib f1_1990_tyreclass.lib f1_1990_replay.lib f1_1990_ui.lib f1_1990_voice.lib f1_1990_bspec.lib f1_1990_agent.lib \
    f1_1990_tabs.lib f1_1990_pu.lib f1_1990_parts.lib f1_1990_quali.lib f1_1990_rnd.lib f1_1990_rndlook.lib f1_1990_wizard.lib f1_1990_intro.lib f1_1990_sessions.lib f1_1990_qualilaps.lib
echo "built $(pwd)/f1_1990.dll $(pwd)/f1_1990_tyres.dll $(pwd)/f1_1990_tyreclass.dll $(pwd)/f1_1990_replay.dll $(pwd)/f1_1990_ui.dll $(pwd)/f1_1990_voice.dll $(pwd)/f1_1990_bspec.dll $(pwd)/f1_1990_agent.dll $(pwd)/f1_1990_tabs.dll $(pwd)/f1_1990_pu.dll $(pwd)/f1_1990_quali.dll $(pwd)/f1_1990_rnd.dll $(pwd)/f1_1990_rndlook.dll $(pwd)/f1_1990_wizard.dll $(pwd)/f1_1990_intro.dll $(pwd)/f1_1990_sessions.dll $(pwd)/f1_1990_parts.dll $(pwd)/f1_1990_qualilaps.dll"
