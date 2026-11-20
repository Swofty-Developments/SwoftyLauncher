#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>

// The load-stall report's frames (stall_frames.cpp): each module's function table, read before the first thread is
// suspended, the lookup the unwinder walks with, and a frame's name with the nearest export of a system or addon
// module.
namespace crash {

// Reads every listed module's function table for stall_lookup.
void stall_tables();
// The function entry holding pc and its module's base, from the tables stall_tables read; allocates nothing.
PRUNTIME_FUNCTION stall_lookup(DWORD64 pc, DWORD64 *base);
// The export that starts the function holding ip (or, for a leaf with no function entry, one just before it).
const char *export_of(uint64_t ip, uint32_t &off);
// The frame's place, as the crash report names it, with its export when it has one.
void stall_frame(char *w, size_t cap, uint64_t ip);

}
