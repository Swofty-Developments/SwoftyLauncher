#pragma once
#include <windows.h>

#include "core/hud/hud_guard.hpp"

#include <dxgi1_6.h>

#include <string>
#include <vector>

namespace hud::overlay {

bool gate_on();
const std::vector<std::string> &overlay_off();
bool killed(const char *what);
int before_fg_forced();
int rival_forced();
bool before_fg_wanted();
// RivaTuner Statistics Server attaches to the process when the probe window is made, so arm_before_fg asks this
// after arm_swapchain; its shared memory names it when it runs but has not attached yet, which is what setup
// sees before the probe window exists.
std::string device_hook_rival();
std::vector<std::string> loaded_modules();
std::wstring hardware_settings_path();
float paper_white();
UINT output_mode(IDXGISwapChain3 *sc, DXGI_FORMAT fmt, UINT color_space);

// The game's frame generation setting, read again when its hardware settings change. Once it is seen on, it stays
// on for the session.
struct FgWatch {
    std::wstring path;
    FILETIME at{};
    ULONGLONG checked = 0;
    hud::guard::FgSetting setting;
    bool tripped = false, told = false;

    bool frame_gen_off(ULONGLONG now);
};

}
