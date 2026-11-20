#pragma once
#include <string>
#include <string_view>
#include <vector>

namespace hud::arm_policy {

inline constexpr std::string_view FG_MODULES[] = {
    "sl.dlss_g.dll",
    "nvngx_dlssg.dll",
    "amd_fidelityfx_dx12.dll",
    "amd_fidelityfx_framegeneration_dx12.dll",
    "ffx_frameinterpolation_x64.dll",
    "ffx_fsr3_x64.dll",
    "libxess_fg.dll",
    "dlssg_to_fsr3_amd_is_better.dll",
};

inline bool same_name(std::string_view a, std::string_view b)
{
    if (a.size() != b.size())
        return false;
    for (size_t i = 0; i < a.size(); i++) {
        char x = a[i], y = b[i];
        if (x >= 'A' && x <= 'Z')
            x = char(x - 'A' + 'a');
        if (y >= 'A' && y <= 'Z')
            y = char(y - 'A' + 'a');
        if (x != y)
            return false;
    }
    return true;
}

inline std::string_view base_name(std::string_view path)
{
    size_t at = path.find_last_of("\\/");
    return at == std::string_view::npos ? path : path.substr(at + 1);
}

inline bool fg_module(std::string_view path)
{
    std::string_view name = base_name(path);
    for (auto m : FG_MODULES)
        if (same_name(name, m))
            return true;
    return false;
}

inline bool streamline_hook(std::string_view path) { return same_name(base_name(path), "sl.dlss_g.dll"); }

// FidelityFX's API loader (FSR 3.1). F1 25 creates its swapchain through it for upscaling too, so it is on the
// creation stack whatever the settings; like Streamline's hook it passes the game's frames through while the
// game's settings have frame generation off. `fg` is a module name, or one with " (calls Present)" after it.
inline bool ffx_loader(std::string_view fg)
{
    std::string_view name = base_name(fg);
    return same_name(name.substr(0, name.find(' ')), "amd_fidelityfx_dx12.dll");
}

// A swapchain made before the core's creation hooks has no creation stack, so Streamline is read off the
// module that calls Present: its interposer, common layer or frame generation hook.
inline bool streamline_caller(std::string_view path)
{
    std::string_view name = base_name(path);
    return same_name(name, "sl.interposer.dll") || same_name(name, "sl.common.dll") || streamline_hook(name);
}

// sl::Result::eOk is 0; Streamline's other codes are all failures.
inline bool streamline_result_ok(int result) { return result == 0; }

// What a queue learned from a swapchain's own GetDevice makes of the swapchain: only a direct queue is drawn on.
inline std::string learned_fg(bool direct_queue, std::string_view present_fg)
{
    if (!present_fg.empty())
        return std::string(present_fg) + " (calls Present)";
    return direct_queue ? std::string() : std::string("a non-direct present queue");
}

inline std::string fg_in(const std::vector<std::string> &modules)
{
    for (auto &m : modules)
        if (fg_module(m) && !streamline_hook(m))
            return std::string(base_name(m));
    return {};
}

inline bool streamline_in(const std::vector<std::string> &modules)
{
    for (auto &m : modules)
        if (streamline_hook(m))
            return true;
    return false;
}

inline std::string chain(const std::vector<std::string> &modules, std::string_view skip_a, std::string_view skip_b)
{
    std::string out;
    std::string_view last;
    for (auto &m : modules) {
        std::string_view name = base_name(m);
        if (name.empty() || same_name(name, skip_a) || same_name(name, skip_b) || same_name(name, last))
            continue;
        if (!out.empty())
            out += " <- ";
        out += name;
        last = name;
    }
    return out.empty() ? "unknown" : out;
}

}
