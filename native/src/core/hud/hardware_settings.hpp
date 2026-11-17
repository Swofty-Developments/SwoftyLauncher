#pragma once
#include "core/hud/hud_guard.hpp"

#include <string>

namespace hardware_settings {

std::wstring path();
hud::guard::FgSetting frame_gen(const std::wstring &path);

}
