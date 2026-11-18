#pragma once
#include <windows.h>

#include "core/hud/arm_policy.hpp"
#include "core/hud/hardware_settings.hpp"

#include <dxgi1_6.h>

#include <string>
#include <vector>

namespace hud::overlay {

// The overlay's switches from the environment and swofty/assets.json, read together the first time one is asked for.
class Settings {
public:
    bool gate = false;
    std::vector<std::string> off;
    int before_fg_forced = -1, rival_forced = -1;
    float paper_white = 350;

    static Settings read();
    bool killed(const char *what) const;

private:
    std::string kill;
};

const Settings &settings();

// Read and logged once, when the overlay arms: whether it draws before frame generation from the start.
bool before_fg_wanted();
// RivaTuner Statistics Server attaches to the process when the probe window is made, so arm_before_fg asks this
// after arm_swapchain; its shared memory names it when it runs but has not attached yet, which is what setup
// sees before the probe window exists.
std::string device_hook_rival();
std::vector<std::string> loaded_modules();

// The game's HDR mode from its hardware settings, read again when the file changes; -1 until it is read.
class HdrWatch {
public:
    int mode(ULONGLONG now);

private:
    hud::hardware_settings::Poll poll;
    FILETIME at{};
    int value = -1;
};

// The output the overlay draws a swapchain's frames in, logged when it changes.
class OutputMode {
public:
    UINT pick(IDXGISwapChain3 *sc, DXGI_FORMAT fmt, UINT color_space);

private:
    HdrWatch hdr;
    void *told_sc = nullptr;
    unsigned told_out = ~0u;
};

// The game's frame generation setting, read again when its hardware settings change. Once it is seen on, it stays
// on for the session.
struct FgWatch {
    hud::hardware_settings::Poll poll;
    FILETIME at{};
    hud::arm_policy::FgSetting setting;
    bool tripped = false, told = false;

    bool frame_gen_off(ULONGLONG now);
};

}
