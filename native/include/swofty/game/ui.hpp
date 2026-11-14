#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

using Ui_DataDrivenMessage_Init_t = int(void *elem, void *rec);
inline constexpr fn<"Ui_DataDrivenMessage_Init", Ui_DataDrivenMessage_Init_t> Ui_DataDrivenMessage_Init{}; // L2 experimental
using Ui_Init_CarDriverTeamSelectors_t = uint64_t(void *self);
inline constexpr fn<"Ui_Init_CarDriverTeamSelectors", Ui_Init_CarDriverTeamSelectors_t> Ui_Init_CarDriverTeamSelectors{}; // L2 experimental
using Ui_OnlineCustomItem_GetAction_t = void *(void *item, void *out_action);
inline constexpr fn<"Ui_OnlineCustomItem_GetAction", Ui_OnlineCustomItem_GetAction_t> Ui_OnlineCustomItem_GetAction{}; // L2 experimental
using Ui_PresetEventItem_Populate_t = void(void *item);
inline constexpr fn<"Ui_PresetEventItem_Populate", Ui_PresetEventItem_Populate_t> Ui_PresetEventItem_Populate{}; // L2 experimental

}
