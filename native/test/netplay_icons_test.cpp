// Host unit test for the driver platform icon key match (netplay_icons_logic.hpp). run_events_test.sh builds
// and runs it.
#include "../src/core/netplay_icons_logic.hpp"

#include <cstdio>
#include <string>

namespace icons = runtime::netplay_icons;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

int main()
{
    const char *hidden[] = {
        "#overrides.icons.session_crossplay_platform.icon_platforms_steam",
        "#overrides.icons.session_crossplay_platform.icon_platforms_playstation",
        "#overrides.icons.session_crossplay_platform.icon_platforms_xbox",
        "#overrides.icons.session_crossplay_platform.icon_platforms_ea",
        "#overrides.icons.session_crossplay_platform.icon_platforms_generic_console",
        "#overrides.icons.session_crossplay_platform.icon_platforms_generic_pc",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_steam",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_playstation",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_xbox",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_ea",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_generic_console",
        "overrides.icons.session_crossplay_platform.icon_platforms_steam",
    };
    for (const char *k : hidden)
        expect(icons::driver_platform_key(k), std::string("hidden: ") + k);

    const char *kept[] = {
        nullptr,
        "",
        "#",
        "#overrides.icons.session_crossplay_platform.icon_platforms_ai",
        "#overrides.icons.session_crossplay_platform_large.icon_platforms_ai",
        "#overrides.icons.session_crossplay_platform.icon_platforms_steam_extra",
        "#overrides.icons.session_crossplay_platform.icon_platforms_",
        "#overrides.icons.session_crossplay_platform",
        "#overrides.icons.session_crossplay_platform_large",
        "#overrides.icons.session_crossplay_platformx.icon_platforms_steam",
        "#overrides.icons.session_lrg[drive]",
        "#overrides.icons.mp_status_icons[locked]",
        "#overrides.default.blank",
        "#overrides.teams.badges_broadcast.forceindia_26",
        "##overrides.icons.session_crossplay_platform.icon_platforms_steam",
        "view_model_data.hud_spectator_2.pill_0",
    };
    for (const char *k : kept)
        expect(!icons::driver_platform_key(k), std::string("kept: ") + (k ? k : "(null)"));

    expect(std::string(icons::blank) == "#overrides.default.blank", "the replacement is the game's blank image key");
    expect(!icons::driver_platform_key(icons::blank), "the replacement is not itself a platform key");

    printf("%s (%d failure%s)\n", fails ? "FAILED" : "passed", fails, fails == 1 ? "" : "s");
    return fails ? 1 : 0;
}
