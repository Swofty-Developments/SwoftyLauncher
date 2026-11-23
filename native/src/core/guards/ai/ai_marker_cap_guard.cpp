#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/ai/ai_marker_cap_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>

// AI marker cap guard (re/bindings/game/Game_AiRoute_CompileMarkerChannel.json, F1_25.exe+0x677ae0 on the KB build).
//
// Game_AiRoute_BuildGraph compiles the marker tables of every AI lane with Game_AiRoute_CompileMarkerChannel: 7
// channels (0 generated side markers, 1 brake_line, 2 corner_end_line, 3 corner_start_line, 4 no writer found, 5
// retire_line, 6 generated width-class markers), each read from gate +0xc8 + c * 8 of the first link's from gate and
// of every link's to gate. The function gathers each channel into stack arrays of 256 distances (rsp+0x70+c*0x400)
// and 256 pointers (rsp+0x1c70+c*0x800) in a 0x5478-byte frame and counts with no bound (inc r9d at 0x140677d00), then
// copies each channel into the lane's block of the heap buffer BuildGraph sized exactly with
// Game_AiRoute_CalcLaneBufferSize_Thunk, and stores the block at lane +0x490 + c * 8. A lane with more than 256
// markers on one channel overwrites the next channel's arrays and, from channel 6, the saved registers and the return
// address: the track crashes while it loads (a 25 km Nordschleife needs ~548 width-class markers on channel 6).
//
// The guard wraps the function. It counts the lane's markers the way the game does; when no channel has more than 256
// the game's function runs unchanged, so every stock track compiles byte for byte as before. Otherwise core writes the
// same blocks itself, straight into the game's buffer (whose size the game computed from the same count), with no
// array of its own. Every reader of the tables (the AI line lookups in 0x14063e370, 0x140650f90, 0x1406663d0,
// 0x14066df70, 0x14066e4e0, 0x14066e830, 0x14066eec0 and BuildGraph itself) walks them with the u32 count of the
// block, so more than 256 entries need nothing else.
//
// SWOFTY_AI_MARKER_CAP_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (ai marker cap guard)";
constexpr const char *COMPILE = "Game_AiRoute_CompileMarkerChannel";

namespace guards = swofty_core::guards;
namespace amc = runtime::ai_marker_cap;

using Compile = void(void *, uint32_t *);
Compile *orig_compile = nullptr;
guards::LogFirst<amc::LOG_FIRST> compiled{"lanes"};

void compile(void *lane, uint32_t *out)
{
    amc::Counts c = amc::count(reinterpret_cast<uint64_t>(lane));
    if (!amc::over_stock(c)) {
        orig_compile(lane, out);
        return;
    }
    amc::Result r = amc::compile(reinterpret_cast<uint64_t>(lane), reinterpret_cast<uint64_t>(out), c);
    if (auto line = compiled.note())
        swofty::log("[ai] marker cap: an AI lane has %u markers on one channel, more than the game's 256 (side %u, brake %u, "
                    "corner end %u, corner start %u, ch4 %u, retire %u, width class %u); core compiled its tables "
                    "(%llu bytes, %u channels dropped)%s",
                    amc::most(c), c.n[0], c.n[1], c.n[2], c.n[3], c.n[4], c.n[5], c.n[6], (unsigned long long)r.bytes, r.dropped,
                    line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::ai_marker_cap,
                        {
                            .set = MIXIN,
                            .name = "[ai] marker cap guard",
                            .env = "SWOFTY_AI_MARKER_CAP_GUARD",
                            .needs = {COMPILE},
                            .mixins = {{COMPILE, SWOFTY_MIXIN_WRAP, 100, (void *)&compile, (void **)&orig_compile}},
                            .armed = "an AI lane with more than 256 markers on one channel gets its marker tables "
                                     "compiled by core instead of overflowing the game's stack arrays",
                        }};

}
