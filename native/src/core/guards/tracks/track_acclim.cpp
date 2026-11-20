#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/guards/tracks/track_acclim_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <vector>

// Track Acclimatisation guards (re/db/functions.tsv Game_PracticeTrackAcclimatisation_RefreshState and
// Game_PracticeTrackAcclimatisation_FlagCurrentTarget).
//
// The Track Acclimatisation practice programme copies its targets (0x370-byte corner entries at self+0x350..+0x358)
// from the track's TrackAcclimatisationResource (Game_PracticeTrackAcclimatisation_CopyTargets). A track without
// that resource's CornerData, such as an imported community track, leaves the vector empty and null. The
// programme's start and most of its other slots return when the vector is empty, but two slots index the current
// entry without that check, and the practice programmes' message handlers run them for every programme:
// - the refresh (slot +0xf0), as the car leaves the garage: ACCESS_VIOLATION reading 0x4 at F1_25.exe+0x299e59b;
// - the penalty flag (slot +0x1a8), on a penalty or warning for the car: ACCESS_VIOLATION reading 0xc4 at
//   F1_25.exe+0x29a0634.
//
// Both guards return from their slot when the vector is empty, as the programme's start does. A programme with
// targets runs the game's code unchanged. The penalty flag is a leaf without unwind data, so its guard is a site at
// its entry (site_Game_AcclimPenalty_Entry) that continues at its ret (site_Game_AcclimPenalty_Return).
//
// SWOFTY_ACCLIM_GUARD=0 turns both off.

namespace {

namespace guards = swofty_core::guards;
namespace ta = runtime::track_acclim;

constexpr const char *MIXIN = "swofty_core.dll (track acclimatisation guard)";
constexpr const char *PENALTY_MIXIN = "swofty_core.dll (track acclimatisation penalty guard)";
constexpr const char *TARGET = "Game_PracticeTrackAcclimatisation_RefreshState";
constexpr const char *PENALTY_ENTRY = "site_Game_AcclimPenalty_Entry";
constexpr const char *PENALTY_RETURN = "site_Game_AcclimPenalty_Return";
constexpr int LOG_FIRST = 4;

using Refresh = void(void *);
Refresh *orig_refresh = nullptr;
uintptr_t penalty_return = 0;
guards::LogFirst<LOG_FIRST> skipped{"skips"}, penalty_skipped{"skips"};

void refresh(void *self)
{
    uint64_t begin = 0;
    if (!ta::no_targets(reinterpret_cast<uintptr_t>(self), guards::read_if_readable, nullptr, begin)) {
        orig_refresh(self);
        return;
    }
    auto line = skipped.note();
    if (!line)
        return;
    swofty::log("[acclim] Track Acclimatisation programme 0x%llx has no targets at this track (targets 0x%llx); "
                "refresh skipped%s",
                (unsigned long long)(uintptr_t)self, (unsigned long long)begin, line.tail);
}

void penalty_entry(SwoftyContext *c)
{
    uint64_t begin = 0;
    if (!ta::no_targets(c->rcx, guards::read_if_readable, nullptr, begin))
        return;
    c->rip = penalty_return;
    auto line = penalty_skipped.note();
    if (!line)
        return;
    swofty::log("[acclim] Track Acclimatisation programme 0x%llx has no targets at this track (targets 0x%llx); "
                "penalty flag skipped%s",
                (unsigned long long)c->rcx, (unsigned long long)begin, line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::track_acclim,
                        {
                            .set = MIXIN,
                            .name = "[acclim] track acclimatisation guard",
                            .env = "SWOFTY_ACCLIM_GUARD",
                            .needs = {TARGET},
                            .mixins = {{TARGET, SWOFTY_MIXIN_WRAP, 100, (void *)&refresh, (void **)&orig_refresh}},
                            .armed = "a practice programme refresh with no targets returns instead of faulting",
                            .parts = {{
                                .set = PENALTY_MIXIN,
                                .name = "[acclim] track acclimatisation penalty guard",
                                .keeps = {{PENALTY_RETURN, &penalty_return}},
                                .needs = {PENALTY_ENTRY, PENALTY_RETURN},
                                .mixins = {{PENALTY_ENTRY, SWOFTY_MIXIN_SITE, 100, (void *)&penalty_entry, nullptr}},
                                .armed = "a practice programme penalty flag with no targets returns instead of faulting",
                            }},
                        }};

}
