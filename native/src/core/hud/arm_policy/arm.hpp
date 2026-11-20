#pragma once
#include "core/hud/arm_policy/modules.hpp"

#include <string>
#include <string_view>
#include <vector>

namespace hud::arm_policy {

// Overlays that patch the D3D12 entry points the core's device hooks patch. RivaTuner Statistics Server's
// hook attaches when a window is created and hooks over whatever is in the prologue; a prologue the core
// hooked first sends its trampoline to address 0, so the device hooks are not armed while it is in the process.
inline constexpr std::string_view DEVICE_HOOK_RIVALS[] = {"RTSSHooks64.dll", "RTSSHooks.dll"};

inline std::string_view device_hook_rival(std::string_view path)
{
    std::string_view name = base_name(path);
    for (auto r : DEVICE_HOOK_RIVALS)
        if (same_name(name, r))
            return r;
    return {};
}

inline std::string device_hook_rival_in(const std::vector<std::string> &modules)
{
    for (auto &m : modules)
        if (std::string_view r = device_hook_rival(m); !r.empty())
            return std::string(r);
    return {};
}

// Whether the device hooks arm: a rival overlay in the process keeps them off unless SWOFTY_OVERLAY_FG /
// "overlay_fg" forces them on (forced is -1 when unset).
inline bool device_hooks_allowed(std::string_view rival, int forced) { return rival.empty() || forced > 0; }

// What asks for the present hook at setup. An overlay addon that is on, a frame capture and a direct start's
// cover screen need it; the main menu panel and video card only decorate the home screen.
struct ArmReasons {
    bool overlay = false, capture = false, cover = false, menu = false;
    bool any() const { return overlay || capture || cover || menu; }
    bool menu_only() const { return menu && !overlay && !capture && !cover; }
};

// Whether the present hook stays off for a rival overlay. RivaTuner's hook attaches to the probe window
// arm_swapchain opens and patches the swapchain's entry points the core hooks; on some machines the game then
// jumps to an address that is not code two seconds after it makes its swapchain. So the swapchain hooks are not
// armed for what only draws: an overlay addon, a direct start's cover, the main menu panel and video card. They
// draw nothing and the game and the addons' other work run. A frame capture is asked for by name and arms, and
// SWOFTY_OVERLAY_RIVAL / "overlay_rival" arms the hook anyway (forced is -1 when unset).
inline bool present_hook_yields(const ArmReasons &reasons, std::string_view rival, int forced = -1)
{
    return reasons.any() && !reasons.capture && !rival.empty() && forced <= 0;
}

// Present layers that make the game's swapchain on a path the core's creation hooks miss, and the swapchain
// does not name its queue at Present either. Its present queue is then learned from ExecuteCommandLists,
// which takes the device hooks.
inline constexpr std::string_view PRESENT_LAYERS[] = {"NvPresent64.dll"};

inline std::string present_layer_in(const std::vector<std::string> &modules)
{
    for (auto &m : modules)
        for (auto l : PRESENT_LAYERS)
            if (same_name(base_name(m), l))
                return std::string(l);
    return {};
}

inline bool device_hooks_wanted_for(const std::vector<std::string> &modules) { return !present_layer_in(modules).empty(); }

// NVIDIA's modules that present the game's frames on a swapchain the creation hooks can miss: the NVIDIA app's
// present layer, DLSS Frame Generation and Streamline, in the order the log names them.
inline constexpr std::string_view NVIDIA_FG_SOURCES[] = {"NvPresent64.dll", "nvngx_dlssg.dll", "sl.dlss_g.dll",
                                                         "sl.interposer.dll"};

inline bool nvidia_fg_source(std::string_view path)
{
    std::string_view name = base_name(path);
    for (auto s : NVIDIA_FG_SOURCES)
        if (same_name(name, s))
            return true;
    return false;
}

// Whether the device hooks arm when a swapchain whose present queue cannot be read first reaches Present, and
// why: `source` names the NVIDIA modules in the process, `refused` says why they stay off. AMD's and Intel's
// frame generation calling Present, or SWOFTY_OVERLAY_FG / "overlay_fg" set to 0, keep them off.
struct OnDemandArm {
    std::string source, refused;
    bool arms() const { return !source.empty(); }
};

inline OnDemandArm on_demand_arm(const std::vector<std::string> &modules, std::string_view present_caller, int forced)
{
    OnDemandArm out;
    if (forced == 0) {
        out.refused = "SWOFTY_OVERLAY_FG=0 / \"overlay_fg\": false";
        return out;
    }
    if (fg_module(present_caller) && !nvidia_fg_source(present_caller)) {
        out.refused = std::string(base_name(present_caller)) + " presents it (AMD's or Intel's frame generation)";
        return out;
    }
    for (auto s : NVIDIA_FG_SOURCES)
        for (auto &m : modules)
            if (same_name(base_name(m), s)) {
                if (!out.source.empty())
                    out.source += ", ";
                out.source += s;
                break;
            }
    if (out.source.empty())
        out.refused = "no NVIDIA frame generation, Streamline or present layer module is in the process";
    return out;
}

}
