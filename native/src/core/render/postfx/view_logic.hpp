#pragma once
#include <cstdint>
#include <string>
#include <string_view>

namespace render::postfx {

enum View : uint32_t {
    UNKNOWN = 0,
    COCKPIT,
    TCAM,
    TCAM_OFFSET,
    CHASE_NEAR,
    CHASE_FAR,
    OTHER,
    TRACKSIDE,
    REVERSE_POD,
    NOSE,
    NOSE_ALT,
    WING,
    VIEWS
};
enum Flag : uint32_t { PAUSED = 1, REPLAY = 2, PHOTO = 4, MENU = 8, CUTSCENE = 16, SPECTATING = 32 };

inline constexpr std::string_view VIEW_NAMES[VIEWS] = {"unknown",     "cockpit",   "tcam",     "tcam_offset",
                                                       "chase_near",  "chase_far", "other",    "trackside",
                                                       "reverse_pod", "nose",      "nose_alt", "wing"};
inline constexpr std::string_view FLAG_NAMES[] = {"paused", "replay", "photo", "menu", "cutscene", "spectating"};

inline uint32_t view_bit(std::string_view name)
{
    for (uint32_t i = 1; i < VIEWS; i++)
        if (VIEW_NAMES[i] == name)
            return 1u << i;
    return 0;
}

inline uint32_t flag_bit(std::string_view name)
{
    for (uint32_t i = 0; i < std::size(FLAG_NAMES); i++)
        if (FLAG_NAMES[i] == name)
            return 1u << i;
    return 0;
}

inline std::string flag_text(uint32_t flags)
{
    std::string out;
    for (uint32_t i = 0; i < std::size(FLAG_NAMES); i++)
        if (flags & (1u << i))
            out += (out.empty() ? "" : "+") + std::string(FLAG_NAMES[i]);
    return out.empty() ? "-" : out;
}

inline std::string_view view_text(uint32_t kind) { return kind < VIEWS ? VIEW_NAMES[kind] : "?"; }

struct When {
    uint32_t views = 0, skip = 0;
};

// A pass with views runs only while the view is known and one of them; skip flags stop it while the
// view is known to have them. Without either it always runs.
inline const char *ineligible(const When &w, uint32_t kind, uint32_t flags, bool valid)
{
    if (w.views && !valid)
        return "the view is unknown";
    if (w.views && !(w.views & (1u << kind)))
        return "not in one of its views";
    if (valid && (w.skip & flags))
        return "a skipped state";
    return nullptr;
}

}
