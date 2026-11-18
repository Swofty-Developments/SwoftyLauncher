#pragma once

#include <string>

#include "swofty_api.h"

// The crash handler's two entry points (crash.cpp), run by the core's subsystem table (runtime/subsystems.cpp).
// At attach: the exception handlers, the crash folder and the process facts the reports read.
void crash_attach(SwoftyApi *a, const std::wstring &dir);
// Once the game's code reads decrypted: the hooks on the game's own fatal paths.
void crash_game_hooks();
