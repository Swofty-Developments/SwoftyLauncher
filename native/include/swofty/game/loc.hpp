#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

using Loc_LangSet_Find_t = const char *(void *set, const char *key, int32_t mode);
inline constexpr fn<"Loc_LangSet_Find", Loc_LangSet_Find_t> Loc_LangSet_Find{}; // L2 experimental

}
