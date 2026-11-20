#include "core/guards/career/myteam_vehicle_guard_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>
#include <string>
#include <vector>

// My Team vehicle base guard (re/db/functions.tsv Game_Load_MyTeamVehicleBases, Sess_Load_VehicleAiConfig).
//
// Loading a My Team career runs Game_Load_MyTeamVehicleBases under Game_SaveData_DeserialiseContainer. For each saved
// vehicle base it reads the base's stat_id and file_name, finds the file_name's row in entrants.vehicles with
// Gdb_Find_RowByLabel, and builds the base's AI config from that row with Sess_Load_VehicleAiConfig, with no check
// that the row was found. A file_name whose row is gone, such as an engine supplier addon's My Team base
// (myteam_mes_porsche_2026) with the addon off, leaves the row 'Invalid' with index -1. The first field read then
// returns Gdb_Get_FieldInt's miss slot, and Sess_Load_VehicleAiConfig faults reading -1: at F1_25.exe+0x2502abd
// (Sess_Load_VehicleAiConfig+0x2dd) when the slot's qword is not canonical, or in Gdb_Find_RowByLabel+0x31 under
// Sess_Load_VehicleAiConfig+0x452 when it is and the vehicle_package label is read through it. The slot's content
// depends on the loaded game data, so the same save crashes at either place.
//
// site_Game_MyTeamVehicleBases_AiConfigLoad is the AI config call's argument setup (r12 = the row). When the row's
// index is -1 the guard restores the loop's index and container from the frame and continues at
// site_Game_MyTeamVehicleBases_NextBase, the loop step after the game stores a base. That base is not stored and the
// rest of the save loads. A base whose row exists runs the game's code unchanged. Each missing row is logged once with
// the base and the row's name, and with the addon's name for a row prefix a known addon uses (myteam_mes_ is More
// Engine Suppliers), so Send Logs names the addon to turn back on.
//
// SWOFTY_MYTEAM_VEHICLE_GUARD=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;
namespace mv = runtime::myteam_vehicle;

constexpr const char *MIXIN = "swofty_core.dll (my team vehicle base guard)";
constexpr const char *LOAD = "site_Game_MyTeamVehicleBases_AiConfigLoad";
constexpr const char *NEXT = "site_Game_MyTeamVehicleBases_NextBase";

uintptr_t next_base = 0;
guards::FirstKeys<std::string, mv::LOG_LABELS> logged{"vehicles"};
guards::FirstKeys<std::string, mv::LOG_LABELS> left_logged;

struct Game {
    bool u64(uint64_t a, uint64_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }

    bool u32(uint64_t a, uint32_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }

    bool i32(uint64_t a, int32_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }

    bool text(uint64_t a, std::string &out) { return mem::c_string(static_cast<uintptr_t>(a), out, mv::LABEL_CHARS); }

    bool forced(const std::string &label) { return runtime::myteam_keep_forced(label); }

    mv::Row vehicle_row(const std::string &label)
    {
        if (runtime::myteam_keep_forced(label))
            return mv::Row::missing;
        const SwoftyDb *db = runtime::db_table();
        if (!db)
            return mv::Row::unknown;
        SwoftyDbCursor c{};
        int r = db->row("entrants.vehicles", label.c_str(), &c);
        if (r == SWOFTY_ASSET_OK)
            return mv::Row::present;
        return r == SWOFTY_ASSET_INVALID ? mv::Row::missing : mv::Row::unknown;
    }
};

void say(const mv::Skip &skip)
{
    auto line = logged.note(skip.label);
    if (!line)
        return;
    const char *more = line.tail;
    if (const mv::KnownAddon *addon = mv::addon_for(skip.label))
        swofty::log("[myteam] save: My Team vehicle base %llu (stat id %u) uses vehicle '%s' from %s (%s), which is not in "
                    "the game data (no entrants.vehicles row; the addon is off or gone); the base is skipped and the rest of the "
                    "career loads. Turn %s back on, or if it is on, add that engine back in its settings, to bring this car "
                    "back%s",
                    (unsigned long long)skip.index, skip.stat_id, skip.label.c_str(), addon->name, addon->id, addon->name, more);
    else
        swofty::log("[myteam] save: My Team vehicle base %llu (stat id %u) uses vehicle '%s', which is not in the game data "
                    "(no entrants.vehicles row; the addon that adds it is off or gone); the base is skipped and the rest of "
                    "the career loads. Turn that addon back on to bring this car back%s",
                    (unsigned long long)skip.index, skip.stat_id, skip.label.c_str(), more);
}

void say_left(const mv::Skip &skip)
{
    if (left_logged.note(skip.label))
        swofty::log("[myteam] save: My Team vehicle base %llu (stat id %u) uses vehicle '%s', whose row index reads as missing; "
                    "it is not a known addon's vehicle or its entrants.vehicles row is in the game data, so the game's own "
                    "load runs for it",
                    (unsigned long long)skip.index, skip.stat_id, skip.label.c_str());
}

void ai_config_load(SwoftyContext *c)
{
    Game game;
    mv::Skip skip;
    if (!mv::skip_base(game, c->rsp, c->r12, skip)) {
        if (skip.left)
            say_left(skip);
        return;
    }
    if (runtime::myteam_keep_substitute(c, skip.stat_id, skip.label))
        return;
    c->rdi = skip.container;
    c->r12 = skip.index;
    c->rip = next_base;
    say(skip);
}

guards::SiteGuard guard{guards::CoreSetId::myteam_vehicle_guard,
                        {
                            .set = MIXIN,
                            .name = "[myteam] vehicle base guard",
                            .env = "SWOFTY_MYTEAM_VEHICLE_GUARD",
                            .keeps = {{NEXT, &next_base}},
                            .needs = {LOAD, NEXT},
                            .mixins = {{LOAD, SWOFTY_MIXIN_SITE, 100, (void *)&ai_config_load, nullptr}},
                            .armed = "a saved My Team vehicle base whose vehicle row is gone is skipped instead of faulting",
                        }};

}
