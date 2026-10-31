#include "runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <vector>

// Standings track guard (re/db/globals.tsv site_Career_StandingsWeekendTrackLoad).
//
// Career_Standings_BuildWeekendNode builds one weekend_<n> node per round of the season for the standings screens
// (Team HQ > Standings). For each round (rbp = round index, [rsp+0xa0] = the Weekend) it looks the round's track
// (Weekend +0x278) up in the career's TrackDataSystem (CareerEntity +0xf58) and keeps the TrackDataRecord in rbx, or
// 0 when the track has no record. One branch checks rbx; the other reads [rbx+0x2c0] without a check. A round at a
// community track that has since been turned off has no record: ACCESS_VIOLATION reading 0x2c0 at
// F1_25.exe+0x181e1a1 as the standings open.
//
// At site_Career_StandingsWeekendTrackLoad a null rbx continues at site_Career_StandingsWeekendTrackDone, the checked
// branch, which the game takes for a round without a record and which leaves that round's track fields empty. A
// round with a record runs the game's code unchanged.
//
// SWOFTY_STANDINGS_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (standings track guard)";
constexpr size_t FRAME_WEEKEND = 0xa0;
constexpr size_t WEEKEND_TRACK = 0x278;
constexpr size_t LOG_ROUNDS = 64;

uintptr_t track_done = 0;
bool offered = false, settled = false;
std::mutex logged_lock;
std::vector<uint32_t> logged;

void track_load(SwoftyContext *c)
{
    if (c->rbx)
        return;
    c->rip = track_done;
    uintptr_t weekend = 0;
    uint8_t track = 0;
    bool known = mem::read(c->rsp + FRAME_WEEKEND, weekend) && weekend && mem::read(weekend + WEEKEND_TRACK, track);
    uint32_t round = static_cast<uint32_t>(c->rbp) + 1;
    uint32_t key = (round << 9) | (known ? track : 0x100u);
    bool last = false;
    {
        std::lock_guard<std::mutex> hold(logged_lock);
        for (uint32_t k : logged)
            if (k == key)
                return;
        if (logged.size() >= LOG_ROUNDS)
            return;
        logged.push_back(key);
        last = logged.size() == LOG_ROUNDS;
    }
    const char *more = last ? " (further rounds are not logged)" : "";
    if (known)
        swofty::log("[career] standings: round %u has no track data (track %u is off); skipped%s", round, unsigned(track),
                    more);
    else
        swofty::log("[career] standings: round %u has no track data; skipped%s", round, more);
}

}

namespace runtime {

std::vector<CoreMixins> standings_guard_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_STANDINGS_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[career] standings guard off (SWOFTY_STANDINGS_GUARD=%s)", v);
        return out;
    }
    track_done = swofty::api->resolve("site_Career_StandingsWeekendTrackDone");
    if (!track_done || !swofty::api->resolve("site_Career_StandingsWeekendTrackLoad")) {
        track_done = 0;
        swofty::log("[career] standings guard off: needs site_Career_StandingsWeekendTrackLoad and "
                    "site_Career_StandingsWeekendTrackDone, which this build does not resolve");
        return out;
    }
    out.push_back(
        {MIXIN, {{"site_Career_StandingsWeekendTrackLoad", SWOFTY_MIXIN_SITE, 100, (void *)&track_load, nullptr}}});
    return out;
}

void standings_guard_commit()
{
    if (!offered || settled || !track_done)
        return;
    settled = true;
    swofty::log("[career] standings guard %s",
                core_mixins_active(MIXIN)
                    ? "armed: a standings round whose track has no track data is skipped instead of faulting"
                    : "did not install (see the [core] lines)");
}

}
