#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

inline constexpr address<"Hud_HideReasons_Add"> Hud_HideReasons_Add{}; // L2 experimental
using Hud_MfdRow_AddOption_t = void(void *list_handle, const char *id, const char *lng, uint32_t value);
inline constexpr fn<"Hud_MfdRow_AddOption", Hud_MfdRow_AddOption_t> Hud_MfdRow_AddOption{}; // L2 experimental
using Hud_MfdRow_BaseInit_t = void(void *row, void *list_handle, uint8_t vehicle, const char *name);
inline constexpr fn<"Hud_MfdRow_BaseInit", Hud_MfdRow_BaseInit_t> Hud_MfdRow_BaseInit{}; // L2 experimental
using Hud_PitRepairRow_Init_t = void(void *row, void *list_handle, uint8_t vehicle, const char *name);
inline constexpr fn<"Hud_PitRepairRow_Init", Hud_PitRepairRow_Init_t> Hud_PitRepairRow_Init{}; // L2 experimental
using Hud_PitRepairRow_Sync_t = void(void *row);
inline constexpr fn<"Hud_PitRepairRow_Sync", Hud_PitRepairRow_Sync_t> Hud_PitRepairRow_Sync{}; // L2 experimental
using Hud_PitRepairRow_UpdateEnabled_t = void(void *row, bool force);
inline constexpr fn<"Hud_PitRepairRow_UpdateEnabled", Hud_PitRepairRow_UpdateEnabled_t> Hud_PitRepairRow_UpdateEnabled{}; // L2 experimental
using Hud_PitSettingsVm_Destroy_t = void(void *vm);
inline constexpr fn<"Hud_PitSettingsVm_Destroy", Hud_PitSettingsVm_Destroy_t> Hud_PitSettingsVm_Destroy{}; // L2 experimental
inline constexpr address<"Hud_PitStartClutchPrompt_Update"> Hud_PitStartClutchPrompt_Update{}; // L2 experimental
inline constexpr address<"Hud_PushProIncidentPointsMessage"> Hud_PushProIncidentPointsMessage{}; // L2 experimental
inline constexpr address<"Hud_RaceStartClutchPrompt_Update"> Hud_RaceStartClutchPrompt_Update{}; // L2 experimental
inline constexpr address<"Hud_ReplayHudVM_OnScreenDeactivated"> Hud_ReplayHudVM_OnScreenDeactivated{}; // L2 experimental
inline constexpr address<"Hud_SpectatorScreenVM_OnScreenDeactivated"> Hud_SpectatorScreenVM_OnScreenDeactivated{}; // L2 experimental
inline constexpr address<"Hud_StoryRaceEvent_Begin"> Hud_StoryRaceEvent_Begin{}; // L2 experimental

}
