#include "runtime.hpp"

#include <windows.h>

#include "session_enter_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Session enter AI record guard (re/db/functions.tsv Sess_Ai_BuildVehicleRecords, F1_25.exe+0x38a0e00).
//
// Game_GameModule_OnSessionEnter calls Sess_Ai_BuildVehicleRecords, which builds one 0x188-byte record for each vehicle
// slot. For a vehicle it resolves the car profile names, hashes the car's name and looks the hash up in the map of each
// asset group in [manager + 0x108], stopping at the first group without it (the loader's "Asset group not found" line);
// the nodes found go into a local vector at [rsp + 0x118]. The record is then built from the first node, read with no
// check that there is one (mov rax,[rsp+0x118]; mov rax,[rax] at 0x1438a41bc). A car that no group lists leaves the
// vector null, and the session start faults reading 0x0 (Supercars and era boxes on community tracks that carry no
// AI asset groups for those cars).
//
// site_Sess_AiRecords_FirstGroupRead is the load of the vector. When it is null the guard sets rbx to the manager
// ([rsp + 0x30]) and continues at site_Sess_AiRecords_VehicleNext, the loop tail that frees the local vector and moves
// to the next vehicle; the vehicle gets no record. A vehicle with at least one node runs the game's code unchanged.
//
// SWOFTY_SESSION_ENTER_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (session enter guard)";
constexpr const char *READ = "site_Sess_AiRecords_FirstGroupRead";
constexpr const char *NEXT = "site_Sess_AiRecords_VehicleNext";

namespace se = runtime::session_enter;

uintptr_t vehicle_next = 0;
bool offered = false, settled = false;
std::atomic<int> skipped{0};
se::Skips skips;

bool read_memory(uint64_t address, void *out, size_t n, void *)
{
    if (!runtime::readable(reinterpret_cast<const void *>(address), n))
        return false;
    memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

void first_group_read(SwoftyContext *c)
{
    se::Skip s = se::decide(c->rsp, read_memory, nullptr);
    if (!s.skip)
        return;
    c->rbx = s.manager;
    c->rip = vehicle_next;
    int n = skips.note();
    skipped++;
    if (!skips.logged(n))
        return;
    swofty::log("[session] vehicle slot %u is in none of the AI asset groups, so it gets no AI record%s", s.index,
                skips.last(n) ? " (further skips are not logged)" : "");
}

}

namespace runtime {

std::vector<CoreMixins> session_enter_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_SESSION_ENTER_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[session] enter guard off (SWOFTY_SESSION_ENTER_GUARD=%s)", v);
        return out;
    }
    vehicle_next = swofty::api->resolve(NEXT);
    if (!vehicle_next || !swofty::api->resolve(READ)) {
        vehicle_next = 0;
        swofty::log("[session] enter guard off: needs %s and %s, which this build does not resolve", READ, NEXT);
        return out;
    }
    out.push_back({MIXIN, {{READ, SWOFTY_MIXIN_SITE, 100, (void *)&first_group_read, nullptr}}});
    return out;
}

void session_enter_commit()
{
    if (!offered || settled || !vehicle_next)
        return;
    settled = true;
    swofty::log("[session] enter guard %s",
                core_mixins_active(MIXIN) ? "armed: a vehicle that no AI asset group lists gets no AI record"
                                          : "did not install (see the [core] lines)");
}

}
