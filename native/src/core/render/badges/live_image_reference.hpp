#pragma once
#include <string>
#include <string_view>

// The archive-backed families accepted by the Live image service. Runtime
// badges retain their separate named-resource reader.
namespace live_image_reference {
struct Catalogue {
    const char *path;
};
inline constexpr Catalogue BADGES{"overrides.osd.timing_tower.team_badges_hud"};
inline constexpr Catalogue TYRES[] = {{"overrides.tyre_allocation.key_icons"},
                                     {"overrides.tyre_allocation.compound_icons"},
                                     {"overrides.tyre_allocation.tyre_icons"},
                                     {"overrides.tyres.tyres_small"}};

inline std::string prefix(const Catalogue &c) { return "#" + std::string(c.path) + "."; }

inline bool name(std::string_view s)
{
    if (s.empty() || s.size() > 80)
        return false;
    for (unsigned char c : s)
        if (c < 32 || c > 126 || std::string_view("./\\:#[]").find(char(c)) != std::string_view::npos)
            return false;
    return true;
}

inline const Catalogue *tyre(std::string_view reference)
{
    for (const auto &c : TYRES) {
        const auto p = prefix(c);
        if (reference.starts_with(p) && name(reference.substr(p.size())))
            return &c;
    }
    return nullptr;
}

inline const Catalogue *catalogue(std::string_view reference)
{
    const auto p = prefix(BADGES);
    if (reference.starts_with(p))
        return &BADGES;
    return tyre(reference);
}
} // namespace live_image_reference
