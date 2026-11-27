#pragma once

#include "core/hooks/dispatch.hpp"

namespace swofty_core::hooks {

namespace mfd_point {
inline constexpr const char *POINT = "hud.mfd.pages";
}

void declare_mfd_pages_point(Table &t);

}
