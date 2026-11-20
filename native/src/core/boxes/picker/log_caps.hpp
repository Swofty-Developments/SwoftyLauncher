#pragma once

#include <cstdint>

#include "core/guards/skips.hpp"

namespace era_picker {

constexpr uint32_t LOG_FIRST = 6;

template <int First> using Cap = swofty_core::guards::LogFirst<First>;

// The session-load lines (sessions.cpp), each logged for the first loads that reach it.
struct SessionLines {
    Cap<LOG_FIRST> career_remap, front_end_leave, driver_career, grand_prix, unload_put_back;
    Cap<LOG_FIRST * 2> time_trial_list, time_trial_swap;
};

// The Time Trial budget lines (time_trial.cpp).
struct BudgetLines {
    Cap<LOG_FIRST> empty_override, pool_capped;
    Cap<LOG_FIRST * 3> budget;
};

// The era select's build and press lines (tiles.cpp, virtual_lists.cpp), each logged for the first builds or presses.
struct PickerLines {
    Cap<LOG_FIRST> renarrow, builds, narrow, refused, detached;
};

}
