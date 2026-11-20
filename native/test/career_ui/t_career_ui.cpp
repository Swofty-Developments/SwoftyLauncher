// Test addon for run_career_ui_test.sh: fake_career_ui.exe calls t_ui_step(n) to drive the
// per-career character overrides and the career wizard control. The character entries are first
// registered from the addon's init and the wizard entries from SWOFTY_ON_LOAD, so the core offers
// its character hooks with this commit's addon mixins (the voice hook chains with the wrap below)
// and its wizard hooks right after the ON_LOAD handlers.
#include <swofty/career_wizard.hpp>
#include <swofty/characters.hpp>
#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <cstdint>

namespace ch = swofty::characters;
namespace wz = swofty::career::wizard;

constexpr char kGroup[] = "f1_career_1990";

int g_init_bad = -1, g_load_bad = -1;

SWOFTY_WRAP(Game_SpeechSystem_PostLine, void, void *speech, void *line, uint8_t retrigger) { orig(speech, line, retrigger); }

bool init()
{
    g_init_bad = (ch::swap(kGroup, "elena", "flavio") != ch::Result::ok) + (ch::mute(kGroup, "agent") != ch::Result::ok);
    return true;
}

SWOFTY_ON_LOAD
{
    g_load_bad = (wz::start_in_f1(kGroup) != wz::Result::ok) +
                 (wz::tile_route(kGroup, "career_wizard_driver_career_1990") != wz::Result::ok) +
                 (wz::skip_driver_icons(kGroup) != wz::Result::ok) + (wz::skip_driver_type(kGroup) != wz::Result::ok) +
                 (wz::hide_driver_tab(kGroup, "header_f2") != wz::Result::ok);
    swofty::log("[t_career_ui] at init %d, at load %d registration(s) not ok", g_init_bad, g_load_bad);
}

extern "C" __declspec(dllexport) int t_ui_step(int n)
{
    switch (n) {
    case 0:
        return (ch::available() ? 1 : 0) + (wz::available() ? 2 : 0) + (g_init_bad || g_load_bad ? 4 : 0);
    case 1: {
        int bad = 0;
        bad += ch::swap(kGroup, "elena", "flavio") != ch::Result::ok;
        bad += ch::mute(kGroup, "agent") != ch::Result::ok;
        bad += ch::string(kGroup, "lng_elena_name", "Flavio") != ch::Result::ok;
        bad += ch::strings_json(kGroup, R"({"eng": {"lng_email_from_agent": "Flavio Rossi"}})") != ch::Result::ok;
        bad += wz::start_in_f1(kGroup) != wz::Result::ok;
        bad += wz::tile_route(kGroup, "career_wizard_driver_career_1990") != wz::Result::ok;
        bad += wz::skip_driver_icons(kGroup) != wz::Result::ok;
        bad += wz::season_length(kGroup, wz::option::full_season) != wz::Result::ok;
        bad += wz::skip_driver_type(kGroup) != wz::Result::ok;
        bad += wz::hide_driver_tab(kGroup, "header_f2") != wz::Result::ok;
        bad += wz::hide_driver_tab(kGroup, "header_icon") != wz::Result::ok;
        return bad;
    }
    case 2:
        return int(ch::refresh());
    case 3:
        return ch::career_group() == kGroup ? 0 : 1;
    case 4:
        return ch::career_group().empty() ? 0 : 1;
    case 5:
        return wz::group() == kGroup ? 0 : 1;
    case 6:
        return int(ch::clear(kGroup)) + int(wz::clear(kGroup));
    case 7:
        return int(ch::swap("", "elena", "flavio")) * 10 + int(wz::replace_step(kGroup, 300, 6));
    case 8:
        return int(ch::strings_json(kGroup, "[1, 2]"));
    }
    return -1;
}

SWOFTY_ADDON(init)
