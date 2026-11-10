// Host unit test for the main menu logic (main_menu_logic.hpp): the row layout, the removed entries, the status
// strip text, the home menu state file and the Time Trial warning rule. run_main_menu_test.sh builds and runs it.
#include "../src/core/main_menu_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace mm = runtime::main_menu;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static std::vector<std::string> retail()
{
    return {"story", "career", "mod_menu", "f1_world", "divider", "local_multiplayer", "pro_leagues", "player_hub",
            "theatre"};
}

static std::string at(const mm::Plan &p, int row, int col)
{
    for (auto &c : p.cards)
        if (c.row == row && c.column == col)
            return c.id;
    return "";
}

int main()
{
    auto p = mm::layout(retail(), true);
    expect(at(p, 0, 0) == "story" && at(p, 0, 1) == "career", "Braking Point and Career lead the first row");
    expect(at(p, 1, 0) == mm::ID_GRAND_PRIX && at(p, 1, 1) == mm::ID_TIME_TRIAL, "Grand Prix and Time Trial share row 2");
    expect(at(p, 2, 0) == "theatre" && p.cards[4].wide, "Theatre is a full-width row under them");
    expect(at(p, 3, 0) == mm::ID_SHOWROOM, "Showroom follows Theatre");
    expect(at(p, 4, 0) == "mod_menu" && p.cards.back().id == "mod_menu", "Addon Settings is last");
    for (auto &c : p.cards)
        expect(c.id != "f1_world" && c.id != "local_multiplayer" && c.id != "pro_leagues" && c.id != "player_hub",
               "removed entry gone: " + c.id);
    expect(p.dropped.size() == 5, "the four removed entries and the divider are reported dropped");

    auto q = mm::layout({"career", "home_navigate_to_challenge_event", "theatre", "mod_menu", "f1_world"}, false);
    expect(at(q, 0, 1) == "home_navigate_to_challenge_event", "the challenge tile joins the first row");
    expect(at(q, 3, 0) == "mod_menu", "no Showroom card when its route is unavailable");
    auto r = mm::layout({"career", "future_tile", "theatre", "mod_menu", "f1_world"}, true);
    expect(at(r, 0, 1) == "future_tile", "an unknown stock entry stays in the first row");
    auto none = mm::layout({"career", "theatre", "mod_menu"}, true);
    expect(at(none, 1, 0) == "" && at(none, 1, 1) == "", "no Grand Prix or Time Trial card without the F1 World tile");

    auto s = mm::parse_state(
        R"({"schema":1,"launcher_version":"1.8.37","discord":{"signed_in":true,"name":"swofty","avatar":"C:/a.png"},)"
        R"("video":{"title":"T","url":"https://youtu.be/x","thumbnail":"C:/t.jpg"}})");
    expect(s.loaded && s.version == "1.8.37", "the state file loads");
    expect(mm::version_line(s) == "Swofty Launcher 1.8.37", "version line");
    expect(mm::discord_line(s) == "Signed in to Discord as swofty", "signed in line");
    expect(mm::has_avatar(s) && mm::has_video(s), "avatar and video present");
    expect(mm::watch_line("Y") == "Press Y to watch", "watch line");
    auto out = mm::parse_state(R"({"schema":1,"launcher_version":"1.8.37","discord":{"signed_in":false,"name":""}})");
    expect(mm::discord_line(out) == "Not signed in to Discord", "signed out line");
    expect(!mm::has_avatar(out) && !mm::has_video(out), "signed out has no avatar, no video card without a video");
    expect(!mm::parse_state("not json").loaded && !mm::parse_state(R"({"schema":2})").loaded, "bad state is not loaded");

    expect(mm::time_trial_warning_due(out, true, false), "signed out Time Trial warns");
    expect(!mm::time_trial_warning_due(out, true, true), "the warning shows once per session");
    expect(!mm::time_trial_warning_due(out, false, false), "Grand Prix does not warn");
    expect(!mm::time_trial_warning_due(s, true, false), "signed in does not warn");
    expect(!mm::time_trial_warning_due(mm::State{}, true, false), "no state file, no warning");
    expect(mm::djb2("") == 5381 && mm::djb2("a") == 5381 * 33 + 'a', "djb2 matches the game's string hash");
    expect(mm::open_request_json("https://youtu.be/a") == R"({"url":"https://youtu.be/a"})", "open request json");
    expect(mm::open_request_json("a\"b") == R"({"url":"a\"b"})", "open request json escapes quotes");
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
