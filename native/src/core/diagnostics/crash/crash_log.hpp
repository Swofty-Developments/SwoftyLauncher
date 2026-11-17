#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <string>

#include "core/diagnostics/crash/blackbox.hpp"
#include "swofty_api.h"

namespace crash {

// The core's log channel. Every api->log line passes through tap() on its way to the loader; cl() writes the crash
// code's own lines and, with SWOFTY_EA_DIAG=1, mirrors them into swofty\ea-diagnostics.log.
struct CrashLog {
    SwoftyApi *api;
    void (*loader_log)(const char *, ...);
    std::wstring diag_path;
};

// The black box: the last events and the last game-log warnings, each in a fixed ring that never locks or allocates.
struct Recorder {
    blackbox::Ring<256> events;
    blackbox::Ring<64> warnings;
};

extern CrashLog logger;
extern Recorder recorder;

std::wstring data_folder();
std::string fmt(const char *f, ...);
void diag_line(const std::string &s);
void cl(const char *f, ...);
void tap(const char *f, ...);

}
