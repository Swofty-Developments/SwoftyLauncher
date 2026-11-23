#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>

#include "core/apply/tracks/box.hpp"
#include "core/guards/tracks/track_data_lookup_logic.hpp"
#include "core/guards/tracks/track_select_logic.hpp"

// The track select guard (work/std16/TRACKS-PLAN.md R1; re/db/subsystems/tracks.md#select, verify V1625-V1637).
// The Grand Prix track select (HomeSoloTrackSelectViewModel::onCommandRaised 0x141c06dc0, advance_pressed
// 0xfc8f026c) and the Time Trial one (TrackSelectViewModel::onCommandRaised 0x141f28410, select_pressed 0x2b0357a)
// are wrapped at their function entries (func_start signatures), which also covers the Grand Prix adjustor thunk.
// On a confirm the guard asks the core's decision (apply/tracks: the active box, the track base, the facts) for the
// track the game last looked up in its track data (site_Trk_TrackData_SetMatched, rdx = the TrackDataRecord: stat
// id +0x01, name +0xd0) after the screen's last selected_track_changed, which the screen's contract does for the
// selected track (Game_BaseGameModeSetupDataContract_UpdateSelectedTrackContextual 0x143852b40). A track that
// doesn't fit the box shows the popup of docs/std/systems/tracks.md and the confirm returns without calling the
// game, which leaves no state (the handlers write nothing before their confirm tests). No lookup since the last
// selection change, or no active box: the confirm runs and one line says why. SWOFTY_TRACK_GUARD=0 or off turns
// the guard off.
//
// The track data lookup guard shares the site, in its own mixin set. The game's track lookup by name (Steam 26566308
// 0x1407a17e0, KB 0x1407a17a0, no KB row; 11 callers on Steam 26566308, the loading screen's current_track among them)
// walks the entity's TrackDataSystem (+0xf58) records at +0x28 for the one whose +0x78 name matches and passes null to
// Game_TrackData_GetTrack when none does, which reads the null record's stat id at the site. A Grand Prix save whose
// weekend is on a track this install has no record for (a removed track addon) does that on Continue, in the loading
// screen's build (Steam 0x141c9fd64): Send Logs 2026-11-22 aober420 1574181978926882986, ACCESS_VIOLATION reading 0x1
// at the site's relocated movzx with rdx 0, 2 of 2 Continues; the theatre check of 2026-11-12 faulted at the same
// instruction opening a replay whose files are gone. The guard hands GetTrack the record track_data_lookup_logic.hpp
// picks from the matched set (melbourne, else the set's first track), so the lookup returns that track's data where it
// faulted, and the weekend track guard moves the weekend itself to melbourne when the mode starts.
// SWOFTY_TRACK_LOOKUP_GUARD=0 or off turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace trk = swofty_core::apply::tracks;
namespace ts = runtime::track_select;
namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (track select guard)";
constexpr const char *GP = "Trk_GrandPrixTrackSelectVm_OnCommandRaised";
constexpr const char *TT = "Trk_TimeTrialTrackSelectVm_OnCommandRaised";
constexpr const char *SITE = "site_Trk_TrackData_SetMatched";
constexpr const char *LOOKUP_MIXIN = "swofty_core.dll (track data lookup guard)";
constexpr const char *LOOKUP_ENV = "SWOFTY_TRACK_LOOKUP_GUARD";
constexpr uintptr_t RECORD_STAT_ID = 0x01, RECORD_NAME = 0xd0;
constexpr uint32_t LOG_FIRST = 8;
constexpr int LOOKUP_LOG_FIRST = 8;

using Handler = void *(void *vm, uint32_t **cmd);

Handler *gp_orig, *tt_orig;
std::atomic<uintptr_t> last_record{0};
std::atomic<uint64_t> counter{0}, record_seq{0}, select_seq{0};
std::atomic<bool> live, lookup_live;
guards::LogFirst<LOG_FIRST * 4> logged;
guards::LogFirst<LOOKUP_LOG_FIRST> lookups{"lookups"};
const uint8_t placeholder_record[2] = {0, runtime::track_data_lookup::PLACEHOLDER_STAT_ID};
std::mutex confirmed_lock;
std::string confirmed_time_trial;

void site_matched(SwoftyContext *c)
{
    last_record.store(c->rdx, std::memory_order_relaxed);
    record_seq.store(++counter, std::memory_order_relaxed);
    if (c->rdx || !lookup_live.load(std::memory_order_relaxed))
        return;
    namespace tl = runtime::track_data_lookup;
    tl::Pick p;
    c->rdx = uintptr_t(tl::record_at_site(c->rdx, c->r11, reinterpret_cast<uintptr_t>(placeholder_record),
                                          guards::read_if_readable, nullptr, p));
    if (auto line = lookups.note())
        swofty::log("[tracks] track data lookup guard: %s%s", tl::line(p).c_str(), line.tail);
}

void note(const std::string &line)
{
    if (logged.note())
        swofty::log("%s", line.c_str());
}

bool refused(bool time_trial)
{
    uintptr_t rec = last_record.load(std::memory_order_relaxed);
    std::string name;
    uint8_t stat = 0xff;
    if (!ts::looked_up(rec, record_seq.load(), select_seq.load()) || !mem::read(rec + RECORD_STAT_ID, stat)) {
        note(std::string("[tracks] guard: ") + (time_trial ? "Time Trial" : "Grand Prix") +
             " confirm with no track looked up since the last selection; allowed");
        return false;
    }
    uintptr_t at = 0;
    if (!mem::read(rec + RECORD_NAME, at) || !mem::c_string(at, name, 64))
        name.clear();
    if (time_trial) {
        std::lock_guard<std::mutex> lk(confirmed_lock);
        confirmed_time_trial = name;
    }
    trk::Active a = trk::active();
    if (!a.set) {
        note("[tracks] guard: " + name + " confirmed with no box picked; allowed");
        return false;
    }
    const trk::BaseTrack *track = ts::track_of(trk::base(), name, stat);
    trk::Decision d = trk::guard_package(a.package, track, time_trial ? trk::Mode::time_trial : trk::Mode::grand_prix,
                                         track ? std::string_view(track->name) : std::string_view(name));
    note(d.line);
    if (d.ok)
        return false;
    runtime::popup_show(d.title, d.body, "swofty.core");
    return true;
}

void *handle(Handler *orig, void *vm, uint32_t **cmd, uint32_t confirm, bool time_trial)
{
    uint32_t hash = 0;
    uintptr_t p = 0;
    if (cmd && mem::read(reinterpret_cast<uintptr_t>(cmd), p) && p && mem::read(p, hash)) {
        ts::Command what = ts::command(hash, confirm);
        if (what == ts::Command::selection_changed)
            select_seq.store(++counter);
        else if (what == ts::Command::confirm && live && refused(time_trial))
            return nullptr;
    }
    return orig(vm, cmd);
}

void *gp_entry(void *vm, uint32_t **cmd) { return handle(gp_orig, vm, cmd, ts::ADVANCE_PRESSED, false); }

void *tt_entry(void *vm, uint32_t **cmd) { return handle(tt_orig, vm, cmd, ts::SELECT_PRESSED, true); }

class TrackSelectGuard final : public guards::Guard {
public:
    TrackSelectGuard() : Guard(guards::CoreSetId::track_select) {}

private:
    void build(std::vector<runtime::CoreMixins> &out) override
    {
        char v[16] = {}, lv[16] = {};
        bool select = !guards::env_off("SWOFTY_TRACK_GUARD", v);
        bool lookup = !guards::env_off(LOOKUP_ENV, lv);
        if (!select)
            swofty::log("[tracks] track select guard off (SWOFTY_TRACK_GUARD=%s)", v);
        if (!lookup)
            swofty::log("[tracks] track data lookup guard off (SWOFTY_TRACK_LOOKUP_GUARD=%s)", lv);
        if (!swofty::api->resolve(SITE)) {
            if (select)
                swofty::log("[tracks] track select guard off: needs %s, which this build does not resolve", SITE);
            if (lookup)
                swofty::log("[tracks] track data lookup guard off: needs %s, which this build does not resolve", SITE);
            return;
        }
        for (const char *id : {GP, TT})
            if (select && !swofty::api->resolve(id)) {
                swofty::log("[tracks] track select guard off: needs %s, which this build does not resolve", id);
                select = false;
            }
        if (select || lookup)
            out.push_back({LOOKUP_MIXIN, {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&site_matched, nullptr}}});
        repair = lookup;
        if (select)
            out.push_back({MIXIN,
                           {{GP, SWOFTY_MIXIN_WRAP, 100, (void *)&gp_entry, (void **)&gp_orig},
                            {TT, SWOFTY_MIXIN_WRAP, 100, (void *)&tt_entry, (void **)&tt_orig}}});
    }

    void settle() override
    {
        bool site = offered_set(LOOKUP_MIXIN) && runtime::core_mixins_active(LOOKUP_MIXIN);
        if (offered_set(LOOKUP_MIXIN) && repair) {
            lookup_live = site;
            swofty::log("[tracks] track data lookup guard %s",
                        guards::install_state(site, "armed: a track data lookup for a track with no record reads melbourne's "
                                                    "track data (else the first track's) instead of faulting"));
        }
        if (!offered_set(MIXIN))
            return;
        live = site && runtime::core_mixins_active(MIXIN);
        swofty::log("[tracks] track select guard %s",
                    guards::install_state(
                        live, "armed: a Grand Prix or Time Trial confirm of a track the box can't run shows why and stays on the "
                              "screen"));
    }

    bool repair = false;
};

TrackSelectGuard guard;

}

namespace runtime {

std::string track_select_time_trial()
{
    std::lock_guard<std::mutex> lk(confirmed_lock);
    return confirmed_time_trial;
}

}
