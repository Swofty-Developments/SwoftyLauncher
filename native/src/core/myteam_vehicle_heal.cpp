#include "myteam_vehicle_heal_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

// My Team vehicle heal (re/db/functions.tsv Sess_Build_SessionVehicleClassLists, Ent_EntrantsSystem_ReplaceVehicle).
//
// A My Team car is a custom vehicle: kind 1 in the entrants system's vehicle tables ([entrants+0x328+kind*0x10], slots
// indexed by stat id), saved in the profile's entrants 'vehicles' list next to every other career's My Team cars. The
// player team's entrants carry its handle (EntrantRecord +0x5c, kind 1 and the car's stat id; the save slot's vehicle,
// 0x4000 | id). Launcher 1.9.21's My Team vehicle base guard dropped every saved car on load (it read row+0x200, which
// Gdb_Get_RowByIndex sets to -1 for every row it builds), and 1.9.22 still dropped More Engine Suppliers cars. The next
// save wrote the list without them, so the career's handle names nothing: entering a session faults in 0x142458710
// reading [vehicle+0xcea] (F1_25.exe+0x2458792 under Game_GameModule_OnSessionEnter).
//
// site_Sess_SessionEnter_VehicleLookup is the zero check after the first lookup of each session entrant's vehicle in
// Sess_Build_SessionVehicleClassLists, ahead of the unchecked one that faults. When the lookup returns nothing for a
// kind 1 handle, the heal rebuilds that car the way the game replaces a My Team car: Ent_EntrantsSystem_ReplaceVehicle
// with the entrant's handle and the vehicle data of a stock My Team car (myteam_mercedes_2026 and the other 2026 cars when
// the session's stock cars are 2026 ones, else myteam_mercedes and the other 2025 cars). The rebuilt car is returned in
// rax, so the session loads with it, and the next save keeps it: the career is healed with the stock car in place of the
// lost one. Every rebuilt car is logged with the stock car it took. A lookup that finds a car runs the game's code unchanged.
//
// SWOFTY_MYTEAM_VEHICLE_HEAL=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace mh = runtime::myteam_heal;

constexpr const char *MIXIN = "swofty_core.dll (my team vehicle heal)";
constexpr const char *SITE = "site_Sess_SessionEnter_VehicleLookup";
constexpr const char *REPLACE = "Ent_EntrantsSystem_ReplaceVehicle";

using Replace_t = int (*)(void *entrants, uint32_t handle, const void *vehicle);

uintptr_t replace_at = 0;
bool offered = false, settled = false;
std::mutex heal_lock;
mh::Ids healed;
mh::Ids failed;

struct Game {
    bool u64(uint64_t a, uint64_t &out)
    {
        return mem::read(static_cast<uintptr_t>(a), out);
    }

    bool u32(uint64_t a, uint32_t &out)
    {
        return mem::read(static_cast<uintptr_t>(a), out);
    }

    bool text(uint64_t a, std::string &out)
    {
        return mem::c_string(static_cast<uintptr_t>(a), out, mh::LABEL_CHARS);
    }
};

void session_vehicle(SwoftyContext *c)
{
    if (c->rax || !replace_at)
        return;
    Game game;
    uint32_t handle = 0;
    if (!game.u32(c->rsi + mh::ENTRANT_VEHICLE, handle) || !mh::healable(handle) || !c->rbx)
        return;
    std::lock_guard<std::mutex> hold(heal_lock);
    if (uint64_t v = mh::vehicle(game, c->rbx, handle)) {
        c->rax = v;
        return;
    }
    uint16_t id = mh::id(handle);
    for (uint16_t f : failed.seen)
        if (f == id)
            return;
    uint64_t first = 0;
    game.u64(c->rdi + mh::FRAME_FIRST_ENTRANT, first);
    mh::Source source;
    if (!mh::plan(game, c->rbx, first, c->r12, source)) {
        if (failed.first(id))
            swofty::log("[myteam] heal: the player's My Team car (custom vehicle %u) is missing from the save and no stock My "
                        "Team car is loaded to rebuild it from; the session start fails as before",
                        (unsigned)id);
        return;
    }
    int r = reinterpret_cast<Replace_t>(replace_at)(reinterpret_cast<void *>(c->rbx), handle,
                                                     reinterpret_cast<const void *>(source.vehicle + mh::VEHICLE_DATA));
    uint64_t v = mh::replaced(r) ? mh::vehicle(game, c->rbx, handle) : 0;
    if (!v) {
        if (failed.first(id))
            swofty::log("[myteam] heal: rebuilding the player's My Team car (custom vehicle %u) from '%s' failed (%s returned "
                        "%d); the session start fails as before",
                        (unsigned)id, source.label.c_str(), REPLACE, r);
        return;
    }
    c->rax = v;
    if (healed.first(id))
        swofty::log("[myteam] heal: the player's My Team car (custom vehicle %u) was missing from the save (launcher 1.9.21 "
                    "dropped every My Team car on load, 1.9.22 the More Engine Suppliers ones, and the next save wrote the "
                    "career without it); rebuilt it from the stock My Team car '%s' (%s season) so the career can be raced. "
                    "The next save keeps it",
                    (unsigned)id, source.label.c_str(), source.y2026 ? "2026" : "2025");
}

}

namespace runtime {

std::vector<CoreMixins> myteam_vehicle_heal_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_MYTEAM_VEHICLE_HEAL", v, sizeof v);
    if (n && n < sizeof v && mh::switch_off(v)) {
        swofty::log("[myteam] vehicle heal off (SWOFTY_MYTEAM_VEHICLE_HEAL=%s)", v);
        return out;
    }
    replace_at = swofty::api->resolve(REPLACE);
    if (!replace_at || !swofty::api->resolve(SITE)) {
        replace_at = 0;
        swofty::log("[myteam] vehicle heal off: needs %s and %s, which this build does not resolve", SITE, REPLACE);
        return out;
    }
    out.push_back({MIXIN, {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&session_vehicle, nullptr}}});
    return out;
}

void myteam_vehicle_heal_commit()
{
    if (!offered || settled || !replace_at)
        return;
    settled = true;
    swofty::log("[myteam] vehicle heal %s",
                core_mixins_active(MIXIN) ? "armed: a My Team car missing from the save is rebuilt from a stock My Team car "
                                            "when its session starts"
                                          : "did not install (see the [core] lines)");
}

}
