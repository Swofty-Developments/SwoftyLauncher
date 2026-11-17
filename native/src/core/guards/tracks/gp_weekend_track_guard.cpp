#include "apply/moments.hpp"
#include "gp_weekend_track_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstddef>
#include <cstdint>
#include <mutex>
#include <set>
#include <string>
#include <utility>
#include <vector>

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
}

// Grand Prix weekend track guard (re/db/globals.tsv site_Game_TyreAllocationComponent_PresetTrackRead).
//
// Entering Grand Prix (the activity container's solo_gp, from the F1 World hub or the launcher's Grand Prix card) runs
// Game_Game_SetCurrentGameMode -> Game_GameModeChampionship_Begin -> Game_TyreAllocationComponent_OnStartWeekend ->
// Game_TyreAllocationComponent_SetPresetChoice for the mode's current weekend before any screen shows. SetPresetChoice
// takes the weekend from [component+0x270] vt+0x98 (r9, a Career Weekend), reads its track stat id (Weekend+0x278) and
// indexes the career's TrackDataSystem (component+0x278 -> CareerEntity+0xf58) item vector +0x28..+0x30 (16 bytes per
// stat id, record at +8) with it. A stat id past the vector or on a null item calls Ne_FatalAbort. A Grand Prix
// remembers the track it last raced, so a Grand Prix last raced on an added track whose addon is now off or gone (or
// that had another stat id then) aborts every entry into Grand Prix: EA bundles of 2026-11-12, track 47 against 43
// slots, and against 84 slots with item 47 null.
//
// The site is the movzx of the track byte (r9 = the weekend, r14 = the component). When the item for the weekend's
// track is missing and the mode is Grand Prix, the guard writes the stat id of the melbourne record (else the first
// record) into the weekend, the same repair apply/moments.cpp makes at the session load, so the game's own code reads a
// track it has. Another mode's missing track is logged and left to the game. Each weekend and track is logged once.
//
// The site is found by byte scan (the KB row is L1: subsystem game is owned by re-career-teams). Its pattern matches once
// on the Steam exe (.xtls) and on the EA dump (.rdata, 0x140bca48f).
//
// SWOFTY_GP_WEEKEND_TRACK_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace g = swofty::game;
namespace wt = runtime::gp_weekend_track;

const runtime::Sig S_PRESET_TRACK_READ{"site_Game_TyreAllocationComponent_PresetTrackRead",
                                      "4C 89 4C 24 40 41 0F B6 81 78 02 00 00", 0xbcbb1f, runtime::Sig::addr, 0, 5, 0};

constexpr uintptr_t COMPONENT_CAREER = 0x278;
constexpr uintptr_t CAREER_TRACK_DATA = 0xf58;
constexpr uintptr_t TRACK_DATA_ITEMS_BEGIN = 0x28;
constexpr uintptr_t TRACK_DATA_ITEMS_END = 0x30;
constexpr uintptr_t ITEM_STRIDE = 0x10;
constexpr uintptr_t ITEM_RECORD = 0x8;
constexpr uintptr_t RECORD_NAME = 0xd0;

uintptr_t weekend_vtable = 0;
safetyhook::MidHook h_track;
std::mutex seen_lock;
std::set<std::pair<uintptr_t, int>> seen;

bool item_vector(uintptr_t component, uintptr_t &begin, size_t &count)
{
    uintptr_t career = 0, data = 0, end = 0;
    if (!mem::read(component + COMPONENT_CAREER, career) || !mem::object_like(career) ||
        !mem::read(career + CAREER_TRACK_DATA, data) || !mem::object_like(data) ||
        !mem::read(data + TRACK_DATA_ITEMS_BEGIN, begin) || !mem::read(data + TRACK_DATA_ITEMS_END, end) || end < begin ||
        (end - begin) % ITEM_STRIDE || (end - begin) / ITEM_STRIDE > wt::MAX_ITEMS)
        return false;
    count = (end - begin) / ITEM_STRIDE;
    return true;
}

bool has_record(uintptr_t begin, size_t count, size_t i)
{
    uintptr_t record = 0;
    return i < count && mem::read(begin + i * ITEM_STRIDE + ITEM_RECORD, record) && record;
}

std::vector<wt::Item> read_items(uintptr_t begin, size_t count)
{
    std::vector<wt::Item> out(count);
    for (size_t i = 0; i < count; ++i) {
        uintptr_t record = 0, name = 0;
        if (!mem::read(begin + i * ITEM_STRIDE + ITEM_RECORD, record) || !record)
            continue;
        out[i].present = true;
        if (mem::read(record + RECORD_NAME, name) && name)
            mem::c_string(name, out[i].name, 64);
    }
    return out;
}

void track_read(safetyhook::Context &c)
{
    uintptr_t weekend = c.r9, vt = 0, begin = 0;
    size_t count = 0;
    uint8_t track = 0;
    if (!weekend_vtable || !mem::read(weekend, vt) || vt != weekend_vtable ||
        !mem::read(weekend + offsetof(g::Weekend, track_stat_id), track) || !item_vector(c.r14, begin, count) ||
        has_record(begin, count, track))
        return;
    std::vector<wt::Item> items = read_items(begin, count);
    if (wt::present(track, items))
        return;
    std::string mode = swofty_core::apply::moments::current_mode("the tyre allocation");
    wt::Decision d = wt::decide(track, items, mode == "grand_prix");
    if (d.repair)
        *reinterpret_cast<uint8_t *>(weekend + offsetof(g::Weekend, track_stat_id)) = uint8_t(d.to);
    std::lock_guard<std::mutex> lock(seen_lock);
    if (!seen.insert({weekend, track}).second || seen.size() > size_t(wt::LOG_CAP))
        return;
    swofty::log("[gp] %s", wt::line(d, track, items.size(), mode, d.repair ? items[size_t(d.to)].name : "").c_str());
}

void setup()
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_GP_WEEKEND_TRACK_GUARD", v, sizeof v);
    if (n && n < sizeof v && wt::switch_off(v)) {
        swofty::log("[gp] weekend track guard off (SWOFTY_GP_WEEKEND_TRACK_GUARD=%s)", v);
        return;
    }
    weekend_vtable = swofty::api ? swofty::api->resolve("vt_Career_Weekend") : 0;
    if (!weekend_vtable) {
        swofty::log("[gp] weekend track guard off: vt_Career_Weekend does not resolve on this build");
        return;
    }
    uintptr_t at = runtime::find(S_PRESET_TRACK_READ, "gp", "gp");
    if (!at) {
        swofty::log("[gp] weekend track guard off: %s is not found on this build", S_PRESET_TRACK_READ.what);
        return;
    }
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), track_read);
    if (!h) {
        swofty::log("[gp] weekend track guard: %s: the hook failed: %s", S_PRESET_TRACK_READ.what,
                    runtime::hook_error(h.error()).c_str());
        return;
    }
    h_track = std::move(*h);
    swofty::log("[gp] weekend track guard armed: a Grand Prix whose remembered track has no track data record starts on "
                "melbourne instead of aborting in its tyre allocation");
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("grand prix weekend track guard", setup);
}
