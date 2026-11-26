#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

inline constexpr address<"Hud_HideReasons_Add"> Hud_HideReasons_Add{}; // L2 experimental
using Hud_MfdVm_RefreshPanelAvailability_t = void(void *vm);
inline constexpr fn<"Hud_MfdVm_RefreshPanelAvailability", Hud_MfdVm_RefreshPanelAvailability_t> Hud_MfdVm_RefreshPanelAvailability{}; // L2 experimental
inline constexpr address<"Hud_PitStartClutchPrompt_Update"> Hud_PitStartClutchPrompt_Update{}; // L2 experimental
inline constexpr address<"Hud_PushProIncidentPointsMessage"> Hud_PushProIncidentPointsMessage{}; // L2 experimental
inline constexpr address<"Hud_RaceStartClutchPrompt_Update"> Hud_RaceStartClutchPrompt_Update{}; // L2 experimental
inline constexpr address<"Hud_ReplayHudVM_OnScreenDeactivated"> Hud_ReplayHudVM_OnScreenDeactivated{}; // L2 experimental
inline constexpr address<"Hud_SpectatorScreenVM_OnScreenDeactivated"> Hud_SpectatorScreenVM_OnScreenDeactivated{}; // L2 experimental
inline constexpr address<"Hud_StoryRaceEvent_Begin"> Hud_StoryRaceEvent_Begin{}; // L2 experimental

}
