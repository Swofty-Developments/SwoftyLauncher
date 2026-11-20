#include "core/live/live_rows.hpp"

#include <cstdio>

using namespace swofty_core::live;

static_assert(F_CAR_SYSTEMS && F_ERS && F_WORLD && F_AERO && F_PROGRESS && F_REGISTRY && F_CLASS && F_PIT && F_PIT_LANE &&
              F_PENALTIES && F_CONTROL && F_SPECTATE && F_SPECTATE_GAP && F_MODE && F_BADGES && F_TRACK_NAME && F_TIMING &&
              F_FLAGS && F_TYRES && F_GRID && F_LAP_TIMES && F_TYRE_WEAR && F_TYRE_AGE);

int main() { std::puts("live gates ok"); }
