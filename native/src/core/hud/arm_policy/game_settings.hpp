#pragma once
#include <cstdlib>
#include <string>

namespace hud::arm_policy {

inline bool xml_int(const std::string &text, const char *element, const char *attr, int &out)
{
    size_t at = text.find(std::string("<") + element + " ");
    if (at == std::string::npos)
        return false;
    size_t end = text.find('>', at);
    std::string key = std::string(" ") + attr + "=\"";
    size_t k = text.find(key, at);
    if (k == std::string::npos || k > end)
        return false;
    out = atoi(text.c_str() + k + key.size());
    return true;
}

struct FgSetting {
    bool known = false;
    int mode = -1, multi = -1;
    bool on() const { return !known || mode != 0; }
    const char *word() const { return !known ? "unknown" : mode != 0 ? "on" : "off"; }
};

inline int hdr_mode(const std::string &hardware_settings)
{
    int mode = -1;
    xml_int(hardware_settings, "hdr", "mode", mode);
    return mode;
}

inline constexpr unsigned COLOR_SPACE_UNSET = ~0u;

enum Output : unsigned { SDR = 0, HDR10_PQ = 1, SCRGB = 2 };

inline Output output_for(int format, unsigned color_space, int game_hdr_mode)
{
    constexpr unsigned G2084_P2020 = 12;
    if (format == 24)
        return color_space == G2084_P2020 || (color_space == COLOR_SPACE_UNSET && game_hdr_mode > 0) ? HDR10_PQ : SDR;
    if (format == 10)
        return color_space == G2084_P2020 ? HDR10_PQ : SCRGB;
    return SDR;
}

inline FgSetting fg_setting(const std::string &hardware_settings)
{
    FgSetting f;
    f.known = xml_int(hardware_settings, "frame_gen", "mode", f.mode);
    xml_int(hardware_settings, "multi_frame_gen", "value", f.multi);
    return f;
}

// -1 when SWOFTY_OVERLAY_FG / "overlay_fg" is unset. Unset, it follows the game's setting at start.
inline bool before_fg_wanted(int forced, const FgSetting &game)
{
    if (forced >= 0)
        return forced != 0;
    return game.known && game.mode != 0;
}

}
