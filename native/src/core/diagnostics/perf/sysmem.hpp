#pragma once

#include "core/diagnostics/perf_logic.hpp"

// The memory the profiler's minute line reports (sysmem.cpp): system RAM and commit, the game's working set and
// private bytes, and each hardware adapter's VRAM through DXGI.
namespace perf::sysmem {

// Logs the RAM and every GPU adapter once, at the profiler's start.
void probe();
logic::MemoryMinute sample(double secs);

}
