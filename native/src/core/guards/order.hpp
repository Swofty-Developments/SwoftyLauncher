#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <string_view>

namespace swofty_core::guards {

// The guards that hook game code through safetyhook themselves, in the order they are armed (when_code_ready): the
// order their sources link in (tools/core_link_order.txt), which is the order their SDK on-load entries run in.
enum class HookGuardId : uint8_t {
    activity_resume,
    sequencer_records,
    race_retirement,
    teammate_pairs,
    setup_vm,
    teamhq_track,
    ai_curves,
    replay_read,
    rnd_suppliers,
    gp_weekend_track,
    count,
};

constexpr size_t HOOK_GUARD_COUNT = size_t(HookGuardId::count);

constexpr std::array<std::string_view, HOOK_GUARD_COUNT> HOOK_GUARD_NAMES = {
    "activity_resume",
    "sequencer_records",
    "race_retirement",
    "teammate_pairs",
    "setup_vm",
    "teamhq_track",
    "ai_curves",
    "replay_read",
    "rnd_suppliers",
    "gp_weekend_track",
};

constexpr std::string_view hook_guard_name(HookGuardId id) { return HOOK_GUARD_NAMES[size_t(id)]; }

}
