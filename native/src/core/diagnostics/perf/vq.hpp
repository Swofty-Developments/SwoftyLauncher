#pragma once

#include <windows.h>

#include "pe_imports.h"

#include <cstddef>
#include <cstdint>
#include <string>
#include <unordered_map>

#include "core/diagnostics/perf_logic.hpp"

// VirtualQuery accounting (vq.cpp): the calls swofty modules, the game exe and the threads they start make to
// VirtualQuery, counted per module and per call site through their import slots, and the process's CPU use per thread.
// minute() and cpu() give the deltas since the last call for the profiler's minute lines.
namespace perf::vq {

constexpr int MODS = 64, SHARDS = 16, MADE = 512;
constexpr unsigned SITE_BITS = 10;
constexpr size_t SITES = size_t(1) << SITE_BITS;

struct Last {
    uint64_t cell[MODS + 1][4] = {};
    uint64_t site[SITES][2] = {};
    uint64_t spill[2] = {};
    uint64_t page_resident = 0, page_region = 0;
};

struct Seen {
    uint64_t created = 0, user = 0, kernel = 0;
    bool ours = false, alive = false;
    std::string what;
};

struct Cpu {
    std::unordered_map<DWORD, Seen> seen;
    uint64_t user = 0, kernel = 0, at = 0;
    bool primed = false;
};

// Hooks the core's own VirtualQuery import slots, unless SWOFTY_PERF_VQ=0; enabled() then reports whether it did.
void install();
bool enabled();
// Hooks the import slots of swofty modules (and the game exe) loaded since the last scan.
void scan();
// Logs the first import entry a Profiler import walk skipped as corrupt (pe_imports.h), once per process.
void note_import_skip(const char *module, const pe_import &first, int skipped);
logic::VqMinute minute(Last &last, double secs);
logic::CpuMinute cpu(Cpu &st, double secs);

}
