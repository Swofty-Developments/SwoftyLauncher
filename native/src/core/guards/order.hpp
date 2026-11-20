#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <string_view>

namespace swofty_core::guards {

// The guards that hook game code through safetyhook themselves, in the order they are armed (when_code_ready): the
// order 1.9 armed them in, which is the order their sources' SDK on-load entries run in. Those run in reverse link
// order (tools/core_link_order.txt): lld lays a DLL's static constructors out in link order and the mingw crt calls them
// from the last (gccmain.c __do_global_ctors), and the SDK lists on-load entries as they are constructed.
enum class HookGuardId : uint8_t {
    pit_garage_state,
    gp_weekend_track,
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
    count,
};

constexpr size_t HOOK_GUARD_COUNT = size_t(HookGuardId::count);

constexpr std::array<std::string_view, HOOK_GUARD_COUNT> HOOK_GUARD_NAMES = {
    "pit_garage_state", "gp_weekend_track",  "rnd_suppliers",   "replay_read", "ai_curves",
    "ai_ers",           "calendar_track",    "teamhq_track",    "setup_vm",    "teammate_pairs",
    "race_retirement",  "sequencer_records", "activity_resume",
};

constexpr std::string_view hook_guard_name(HookGuardId id) { return HOOK_GUARD_NAMES[size_t(id)]; }

}
