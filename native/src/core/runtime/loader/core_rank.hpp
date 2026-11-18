#pragma once

#include <cstddef>
#include <cstdint>

// The order swofty_core.dll's own SDK registrations, its mixins and on-load entries, are listed in, whichever order
// its units initialize in. Every unit that registers one names its rank with SWOFTY_CORE_RANK before its first
// registration (a hook guard's unit gets it from SWOFTY_HOOK_GUARD, under its HookGuardId name), and the core's
// SwoftyAddonInit check calls rank_core_mixins before the SDK reads the list, which orders it by rank and keeps each
// unit's own registrations in the order it declares them. tools/init_check.py fails the build when a registering unit
// has no rank or the ranks give another order than the run order, and records the order they give in
// tools/core_init_order.txt (explicit).

namespace runtime::loader {

enum class CoreRank : uint8_t {
    pit_garage_state,
    session_job_probe,
    car_live,
    session_points,
    gp_weekend_track,
    career_ops,
    hub_tab,
    rnd_suppliers,
    replay_read,
    ai_curves,
    ai_ers,
    calendar_track,
    teamhq_track,
    setup_vm,
    teammate_pairs,
    race_retirement,
    sequencer_records,
    activity_resume,
    player_name,
    items,
    ui,
    home_tiles,
    count,
};

constexpr size_t CORE_RANK_COUNT = size_t(CoreRank::count);

// Records where the list stood when its unit's registrations began.
struct RankMark {
    explicit RankMark(CoreRank rank);
};

void rank_core_mixins();

}

#define SWOFTY_CORE_RANK(rank)                                                                                                   \
    namespace {                                                                                                                  \
    const ::runtime::loader::RankMark swofty_core_rank{::runtime::loader::CoreRank::rank};                                       \
    }
