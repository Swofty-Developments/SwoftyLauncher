#pragma once

// Engine settings the game reads through Ne_Get_SettingBool (0x143a43b90 on the KB build) that shorten its own
// loading when they are on. Each is a developer switch the retail system/config.xml leaves out, so the getter
// writes its default (off). One core feature flag forces one setting; the research behind each row is in
// re/research/loading_pipeline.md.

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>

namespace runtime::loading {

struct SettingOverride {
    const char *feature;
    const char *setting;
    bool value;
    const char *effect;
};

// Game.IgnoreMinimumLoadingTime: Sess_SessionTransition_OnStateUpdate (0x1436f9890), Sess_Loader_RequestTrackLoad
// (0x142da7870) and Sess_StoryHub_OnStateEnter (0x1436fb5e0) each hold their state until 3.0 s have passed since it
// started, or 0.0 s with the setting on.
// Game.SkipIntroVideo: Sess_IntroVideo_ParseParams (0x142d6cc30) copies it to SessionStateIntroVideo+0xdc6, which
// lets Sess_IntroVideo_CanFinish (0x142d6d3b0) end the video before a session at once.
inline constexpr std::array<SettingOverride, 2> OVERRIDES{{
    {"loading_skip_minimum_time", "Game.IgnoreMinimumLoadingTime", true,
     "the 3 s minimum of the session transition, track load and story hub loading screens is dropped"},
    {"loading_skip_session_intro_video", "Game.SkipIntroVideo", true,
     "the intro video before a session may end as soon as it starts"},
}};

inline constexpr char PREFIX[] = "Game.";
inline constexpr size_t PREFIX_LEN = sizeof PREFIX - 1;

// Bit i of `active` arms OVERRIDES[i]. Returns the armed row whose setting is `name`, or nullptr. Every name the
// getter sees passes through here, so a name outside "Game." costs one short compare.
inline const SettingOverride *match(const char *name, uint32_t active)
{
    if (!active || !name || std::strncmp(name, PREFIX, PREFIX_LEN) != 0)
        return nullptr;
    for (size_t i = 0; i < OVERRIDES.size(); i++)
        if ((active >> i & 1) && std::strcmp(name, OVERRIDES[i].setting) == 0)
            return &OVERRIDES[i];
    return nullptr;
}

inline size_t index_of(const SettingOverride *row) { return row ? size_t(row - OVERRIDES.data()) : OVERRIDES.size(); }

// The rows a comma-separated feature list names (the SWOFTY_LOADING_FEATURES variable that in-game measurements set,
// so a run can arm a flag without a launcher setting). Unknown names and empty items are ignored.
inline uint32_t parse_list(const char *list)
{
    uint32_t bits = 0;
    for (const char *p = list; p && *p;) {
        const char *end = std::strchr(p, ',');
        size_t n = end ? size_t(end - p) : std::strlen(p);
        while (n && (*p == ' ' || *p == '\t'))
            p++, n--;
        while (n && (p[n - 1] == ' ' || p[n - 1] == '\t'))
            n--;
        for (size_t i = 0; i < OVERRIDES.size(); i++)
            if (n && std::strlen(OVERRIDES[i].feature) == n && std::strncmp(OVERRIDES[i].feature, p, n) == 0)
                bits |= 1u << i;
        if (!end)
            break;
        p = end + 1;
    }
    return bits;
}

}
