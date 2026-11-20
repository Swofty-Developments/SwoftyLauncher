#pragma once
#include "core/hud/arm_policy/modules.hpp"

#include <string_view>

namespace hud::arm_policy {

enum class Gate { Direct, FrameGenNamed, FrameGenStreamline };

inline Gate gate_for(bool fg_named, bool streamline, bool fg_setting_off)
{
    if (fg_named)
        return Gate::FrameGenNamed;
    return streamline && !fg_setting_off ? Gate::FrameGenStreamline : Gate::Direct;
}

// The gate for a swapchain from its frame generation tag (Swapchain::fg) and Streamline flag. FidelityFX's
// loader passes through like Streamline's hook. fg_setting_off() is asked only when it decides the gate (it reads
// the game's hardware settings).
template <class FgOff> inline Gate gate_for_swapchain(std::string_view fg, bool streamline, FgOff fg_setting_off)
{
    const bool ffx = ffx_loader(fg), named = !fg.empty() && !ffx, through = streamline || ffx;
    return gate_for(named, through, named || !through || fg_setting_off());
}

// A swapchain whose queue was learned after the device hooks armed on demand for it: the game named its back
// buffers before they armed, so drawing before frame generation has none to draw into, and it is drawn on at Present.
template <class FgOff> inline Gate gate_for_swapchain(std::string_view fg, bool streamline, bool armed_late, FgOff fg_setting_off)
{
    return armed_late ? Gate::Direct : gate_for_swapchain(fg, streamline, fg_setting_off);
}

inline bool draws_before_present(Gate gate, bool device_hooks) { return gate != Gate::Direct && device_hooks; }

inline bool draws_at_present(Gate gate) { return gate == Gate::Direct; }

}
