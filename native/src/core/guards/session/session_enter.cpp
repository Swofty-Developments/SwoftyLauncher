#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/session/session_enter_logic.hpp"

#include <swofty/mixin.hpp>

#include <cstdint>
#include <cstring>
#include <vector>

// Session enter AI record guard (re/db/functions.tsv Sess_Ai_BuildVehicleRecords, F1_25.exe+0x38a0e00).
//
// Game_GameModule_OnSessionEnter calls Sess_Ai_BuildVehicleRecords, which builds one 0x188-byte record for each vehicle
// slot; later code reads the records at [manager + 0x128] by vehicle slot. For a vehicle it hashes the car's name and
// looks the hash up in the map of each asset group in [manager + 0x108], stopping at the first group without it (the
// loader's "Asset group not found" line); the nodes found go into a local vector. The record is built from the first
// node with no check that there is one (mov rax,[rax] at 0x1438a41bc). A car that no group lists leaves the vector
// null, and the session start faults reading 0x0 (Supercars and era boxes on community tracks that carry no AI asset
// groups for those cars).
//
// site_Sess_AiRecords_NameHash is the instruction after the name hash is computed (r13 = hash, rcx = manager). When
// group 0's map (an MSVC std::map: head +0 leftmost, +8 root, node left +0, right +0x10, isnil byte +0x19, key +0x20)
// lacks the hash, the guard replaces r13 with the key of the map's leftmost node, so the game's own code finds a node
// and builds a full record in the vehicle's slot; the car borrows another car's AI entry. A hash group 0 lists runs the
// game's code unchanged.
//
// SWOFTY_SESSION_ENTER_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (session enter guard)";
constexpr const char *HASH = "site_Sess_AiRecords_NameHash";

namespace guards = swofty_core::guards;
namespace se = runtime::session_enter;

guards::LogFirst<se::LOG_FIRST> skips{"fixes"};
se::Memo memo;
constexpr uint64_t SLOW_US = 5000;
guards::LogFirst<se::LOG_FIRST> slow_logged;

void name_hash(SwoftyContext *c)
{
    LARGE_INTEGER t0, t1, hz;
    QueryPerformanceCounter(&t0);
    se::Fix f = memo.decide(c->rcx, c->rsp, c->r13, GetTickCount64(), guards::read_catching_faults, nullptr);
    QueryPerformanceCounter(&t1);
    QueryPerformanceFrequency(&hz);
    uint64_t us = static_cast<uint64_t>(t1.QuadPart - t0.QuadPart) * 1000000 / static_cast<uint64_t>(hz.QuadPart);
    if (us >= SLOW_US && slow_logged.note()) {
        swofty::log("[session] name hash decision took %llu us (memo %d hits, %d misses)", (unsigned long long)us, memo.hits,
                    memo.misses);
    }
    if (!f.replace)
        return;
    uint64_t hash = c->r13;
    c->r13 = f.key;
    auto line = skips.note();
    if (!line)
        return;
    swofty::log("[session] vehicle slot %u (hash %llx) is in no AI asset group; using %llx for its AI record%s", f.index,
                (unsigned long long)hash, (unsigned long long)f.key, line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::session_enter,
                        {
                            .set = MIXIN,
                            .name = "[session] enter guard",
                            .env = "SWOFTY_SESSION_ENTER_GUARD",
                            .needs = {HASH},
                            .mixins = {{HASH, SWOFTY_MIXIN_SITE, 100, (void *)&name_hash, nullptr}},
                            .armed = "a car that no AI asset group lists borrows an entry for its AI record",
                        }};

}
