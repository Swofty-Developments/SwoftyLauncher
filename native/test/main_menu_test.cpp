// Host unit test for the main menu logic (main_menu_logic.hpp): the row layout, the removed entries, the status
// strip text, the home menu state file and the Time Trial warning rule. run_main_menu_test.sh builds and runs it.
#include "../src/core/main_menu_logic.hpp"

#include <cmath>
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
    expect(at(p, 4, 0) == mm::ID_DIVIDER, "a divider sits between the cards and Addon Settings");
    expect(at(p, 5, 0) == "mod_menu" && p.cards.back().id == "mod_menu", "Addon Settings is last");
    for (auto &c : p.cards)
        expect(c.id != "f1_world" && c.id != "local_multiplayer" && c.id != "pro_leagues" && c.id != "player_hub",
               "removed entry gone: " + c.id);
    expect(p.dropped.size() == 5, "the four removed entries and the divider are reported dropped");

    auto q = mm::layout({"career", "home_navigate_to_challenge_event", "theatre", "mod_menu", "f1_world"}, false);
    expect(at(q, 0, 1) == "home_navigate_to_challenge_event", "the challenge tile joins the first row");
    expect(at(q, 4, 0) == "mod_menu", "no Showroom card when its route is unavailable");
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
    expect(mm::watch_line("RB") == "Press RB to watch", "watch line");
    expect(std::string(mm::template_for("story")) == mm::TEMPLATE_TOP_LEFT && std::string(mm::template_for("career")) == mm::TEMPLATE_TOP_RIGHT &&
               std::string(mm::template_for("swofty_grand_prix")) == mm::TEMPLATE_MODES_LEFT && std::string(mm::template_for("swofty_time_trial")) == mm::TEMPLATE_MODES_RIGHT &&
               std::string(mm::template_for("theatre")) == mm::TEMPLATE_WIDE_A && std::string(mm::template_for("swofty_showroom")) == mm::TEMPLATE_WIDE_B &&
               std::string(mm::template_for("mod_menu")) == mm::TEMPLATE_WIDE_C && std::string(mm::template_for("f1_world")).empty(),
           "every card has its row template and unknown ids have none");
    expect(std::string(mm::template_for("theatre", false)) == mm::TEMPLATE_DEFAULT && std::string(mm::template_for("swofty_showroom", false)) == mm::TEMPLATE_DEFAULT &&
               std::string(mm::template_for("career", false)).empty() && std::string(mm::template_for("swofty_divider")) == mm::TEMPLATE_DIVIDER,
           "without rows Theatre and Showroom take the default large tile; with rows the divider has its slot");
    expect(mm::with_separators(1234) == "1,234" && mm::with_separators(999) == "999" && mm::with_separators(1000000) == "1,000,000" &&
               mm::with_separators(12) == "12" && mm::with_separators(100000) == "100,000",
           "thousands separators");
    expect(mm::online_line(1234) == "1,234 currently playing" && mm::online_line(1) == "1 currently playing" &&
               mm::online_line(0).empty() && mm::online_line(-5).empty(),
           "the online line: separators, singular count, hidden for 0 or less");
    expect(mm::offline_text(0) == "Launched with Swofty Launcher" && mm::offline_text(2500) == "Launched with Swofty Launcher\n2,500 currently playing",
           "the Launched with panel text gains the online line");
    expect(mm::parse_state(R"({"schema":1,"online":321,"discord":{}})").online == 321 && mm::parse_state(R"({"schema":1,"discord":{}})").online == 0,
           "the state file's online field");
    {
        auto list = mm::scene_to_px(-8.6, 3.4, 1914, 1079);
        auto note = mm::scene_to_px(-8.6, -1.77, 1914, 1079);
        auto logo = mm::scene_to_px(-8.65, 3.7, 1914, 1079);
        expect(list.x > 92 && list.x < 104, "the list's left edge (x -8.6) lands at the screenshot's 98 px");
        expect(note.y > 700 && note.y < 722, "the description (y -1.77) lands at the screenshot's 713 px");
        expect(logo.y > 160 && logo.y < 176, "the logo's bottom edge (y 3.7) lands at the screenshot's 168 px");
        auto small = mm::scene_to_px(-8.6, 3.4, 1366, 768);
        expect(small.x / 1366.0f > 0.0481f && small.x / 1366.0f < 0.0661f, "the left edge keeps its place relative to the frame at 1366x768");
    }
    {
        auto small = mm::plan_card(1.0f, 100);
        expect(small.width == 480 && small.height == 150 + 24, "a short title keeps the minimum width and the thumbnail sits in an even 12 px inset");
        auto big = mm::plan_card(1.0f, 300);
        expect(big.width == 12 + 300 + 16 + big.thumb_w + 12 && big.width > 480, "a longer title widens the card");
        auto capped = mm::plan_card(1.0f, 900);
        expect(capped.text_max == 420 && capped.width == 12 + 420 + 16 + capped.thumb_w + 12, "the text block is capped at 420 px");
        auto scaled = mm::plan_card(2.0f, 100);
        expect(scaled.width == 960 && scaled.thumb_h == 300, "the card scales with the screen height");
        expect(std::abs(big.thumb_w / big.thumb_h - 16.0 / 9.0) < 1e-4, "the thumbnail is 16:9");
        auto br = mm::scene_to_px(mm::CARD_RIGHT_X, mm::CARD_BOTTOM_Y, 1908, 1028);
        expect(br.x > 1795 && br.x < 1805 && br.y > 950 && br.y < 962, "the card's right and bottom anchors land at (1800, 956) at 1908x1028");
    }
    expect(mm::panel_due(true, true, true, false, true), "the panel is due on the open home screen once prepared");
    expect(!mm::panel_due(false, true, true, false, true) && !mm::panel_due(true, false, true, false, true) &&
               !mm::panel_due(true, true, false, false, true) && !mm::panel_due(true, true, true, true, true) &&
               !mm::panel_due(true, true, true, false, false),
           "the panel is not due before the tile patch, off the home screen, without a state file, after it disabled itself, or before it is prepared");
    expect(!mm::panel_too_slow(1.0) && !mm::panel_too_slow(4.0) && mm::panel_too_slow(4.1), "the draw budget is a few ms");
    expect(std::string(mm::TT_BODY).find("will not be saved") != std::string::npos && std::string(mm::TT_BODY).find("OK to continue") != std::string::npos,
           "the warning says laps are not saved and that OK continues");
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
