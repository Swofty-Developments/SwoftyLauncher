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
    expect(at(p, 1, 2) == "local_multiplayer", "the Split Screen card (the stock Local Multiplayer item) ends row 2");
    expect(at(p, 2, 0) == "theatre" && p.cards[5].wide, "Theatre is a full-width row under them");
    expect(at(p, 3, 0) == mm::ID_SHOWROOM, "Showroom follows Theatre");
    expect(at(p, 4, 0) == mm::ID_DIVIDER, "a divider sits between the cards and Addon Settings");
    expect(at(p, 5, 0) == "mod_menu" && p.cards.back().id == "mod_menu", "Addon Settings is last");
    for (auto &c : p.cards)
        expect(c.id != "f1_world" && c.id != "pro_leagues" && c.id != "player_hub", "removed entry gone: " + c.id);
    expect(!mm::removed("local_multiplayer") && mm::removed("f1_world") && mm::removed("pro_leagues") && mm::removed("player_hub"),
           "Local Multiplayer is kept (it is the only way to Split Screen); F1 World, League Racing and Player Hub go");
    expect(p.dropped.size() == 4, "the three removed entries and the divider are reported dropped");
    auto bare = mm::layout({"career", "f1_world", "theatre", "mod_menu"}, true);
    expect(at(bare, 1, 2) == "", "no Split Screen card when the game builds no Local Multiplayer tile");

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
               std::string(mm::template_for("local_multiplayer")) == mm::TEMPLATE_MODES_THIRD &&
               std::string(mm::template_for("theatre")) == mm::TEMPLATE_WIDE_A && std::string(mm::template_for("swofty_showroom")) == mm::TEMPLATE_WIDE_B &&
               std::string(mm::template_for("mod_menu")) == mm::TEMPLATE_WIDE_C && std::string(mm::template_for("f1_world")).empty(),
           "every card has its row template and unknown ids have none");
    expect(std::string(mm::template_for("theatre", false)) == mm::TEMPLATE_DEFAULT && std::string(mm::template_for("swofty_showroom", false)) == mm::TEMPLATE_DEFAULT &&
               std::string(mm::template_for("local_multiplayer", false)) == mm::TEMPLATE_DEFAULT &&
               std::string(mm::template_for("career", false)).empty() && std::string(mm::template_for("swofty_divider")) == mm::TEMPLATE_DIVIDER,
           "without rows Theatre, Showroom and Split Screen take the default large tile; with rows the divider has its slot");
    expect(std::string(mm::SPLIT_SCREEN_TITLE) == "SPLIT SCREEN" && !std::string(mm::SPLIT_SCREEN_DESCRIPTION).empty(),
           "the Local Multiplayer item is titled Split Screen");
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
        expect(big.width == 12 + 300 + 28 + big.thumb_w + 12 && big.width > 480, "a longer title widens the card");
        auto capped = mm::plan_card(1.0f, 900);
        expect(capped.text_max == 420 && capped.width == 12 + 420 + 28 + capped.thumb_w + 12, "the text block is capped at 420 px");
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
    expect(std::string(mm::press_route(mm::Mode::time_trial)) == "swofty_time_trial" &&
               std::string(mm::press_route(mm::Mode::grand_prix_new)) == "swofty_grand_prix" &&
               std::string(mm::press_route(mm::Mode::grand_prix_saves)) == "swofty_grand_prix_saves" &&
               std::string(mm::press_route(mm::Mode::none)).empty(),
           "each card has its own launcher route");
    expect(mm::grand_prix_mode(true) == mm::Mode::grand_prix_saves, "the Grand Prix card opens the game's saves screen, saved season or not");
    expect(mm::grand_prix_mode(false) == mm::Mode::grand_prix_new, "SWOFTY_MAIN_MENU_GPSAVES=0 opens the new Grand Prix picker directly");
    expect(std::string(mm::press_route(mm::grand_prix_mode(true))) == "swofty_grand_prix_saves" &&
               std::string(mm::return_command(mm::grand_prix_mode(true))) == "grand_prix_resume",
           "the saves screen route and its session-end hand-over command belong to the default Grand Prix mode");
    expect(std::string(mm::return_command(mm::Mode::time_trial)) == "time_trial" &&
               std::string(mm::return_command(mm::Mode::grand_prix_saves)) == "grand_prix_resume" &&
               std::string(mm::return_command(mm::Mode::grand_prix_new)) == "grand_prix_new",
           "the session-end hand-over raises the command of the card that started the session");
    {
        auto route = mm::Back::route;
        expect(std::string(mm::back_replacement(route, mm::djb2("f1_world_activity_container"), "f1_world")) == "swofty_back_home" &&
                   std::string(mm::back_replacement(route, mm::djb2("f1_world_activity_container"), "home")) == "swofty_back_home",
               "the container's Back (to the hub or home) becomes the launcher's route home");
        expect(std::string(mm::back_replacement(route, mm::djb2("fe_showroom"), "back_1")) == "swofty_home" &&
                   std::string(mm::back_replacement(route, mm::djb2("grand_prix_continue"), "back_1")) == "swofty_home",
               "the Showroom's and the saves list's Back become the launcher's route home");
        expect(mm::back_replacement(route, mm::djb2("fe_f1_world"), "back_1") == nullptr &&
                   mm::back_replacement(route, mm::djb2("f1_world_activity_container"), "tt_track_select") == nullptr &&
                   mm::back_replacement(route, mm::djb2("fe_showroom"), "showroom_f1") == nullptr,
               "other routes stay the game's");
        expect(std::string(mm::back_replacement(mm::Back::home, mm::djb2("f1_world_activity_container"), "f1_world")) == "home" &&
                   mm::back_replacement(mm::Back::home, mm::djb2("fe_showroom"), "back_1") == nullptr,
               "the home variant fires the container's stock home route only");
        expect(mm::back_replacement(mm::Back::stock, mm::djb2("f1_world_activity_container"), "f1_world") == nullptr &&
                   mm::back_replacement(mm::Back::twostep, mm::djb2("f1_world_activity_container"), "f1_world") == nullptr,
               "the stock and two-step variants leave the route alone");
        expect(mm::back_variant_of("", true) == mm::Back::route && mm::back_variant_of("", false) == mm::Back::stock &&
                   mm::back_variant_of("home", false) == mm::Back::home && mm::back_variant_of("twostep", true) == mm::Back::twostep &&
                   mm::back_variant_of("stock", true) == mm::Back::stock,
               "the default Back is the launcher's route when its data is in the build, else the game's");
    }
    expect(mm::hides_panel("home_to_activity") && mm::hides_panel("swofty_time_trial") && mm::hides_panel("swofty_grand_prix_saves") &&
               !mm::hides_panel("swofty_showroom"),
           "the card routes hide the panel, the Showroom's route does not");
    expect(mm::recovery_for(mm::Expect::none, "frontend_flow", false) == mm::Recovery::none &&
               mm::recovery_for(mm::Expect::home, "frontend_flow", true) == mm::Recovery::none,
           "nothing to recover when nothing was expected or the main menu is up");
    expect(mm::recovery_for(mm::Expect::home, "f1_world_flow", false) == mm::Recovery::back_home_route &&
               mm::recovery_for(mm::Expect::container, "f1_world_flow", false) == mm::Recovery::back_home_route &&
               mm::recovery_for(mm::Expect::saves, "f1_world_flow", false) == mm::Recovery::back_home_route,
           "a screen that did not open while the f1_world flow is current takes the migration back to the main menu");
    expect(mm::recovery_for(mm::Expect::container, "frontend_flow", false) == mm::Recovery::open_home &&
               mm::recovery_for(mm::Expect::saves, "frontend_flow", false) == mm::Recovery::open_home &&
               mm::recovery_for(mm::Expect::home, "frontend_flow", false) == mm::Recovery::open_home,
           "a screen that did not open in the front-end flow reopens the main menu screen");
    expect(mm::recovery_for(mm::Expect::container, "frontend_flow", true) == mm::Recovery::none,
           "a main menu that is up is never replaced");
    expect(mm::migrate_failed("Screen fe_home failed to migrate because of 5") && mm::migrate_failed("No screen called x exists") && !mm::migrate_failed(""),
           "a failed migration is told from its error text");
    expect(mm::return_due(true, false, mm::Mode::time_trial, true) && !mm::return_due(true, false, mm::Mode::none, true) &&
               !mm::return_due(false, false, mm::Mode::time_trial, true) && !mm::return_due(true, true, mm::Mode::time_trial, true) &&
               !mm::return_due(true, false, mm::Mode::time_trial, false),
           "the hand-over is armed when a session that a card started has ended");
    expect(mm::entry_point_write(0x146066c74, true) == mm::EntryPointWrite::write,
           "the F1 World entry point is cleared through the address the binding set resolved");
    expect(mm::entry_point_write(0, true) == mm::EntryPointWrite::unresolved && mm::entry_point_write(0, false) == mm::EntryPointWrite::unresolved,
           "a build whose set did not resolve g_Game_F1WorldEntryPoint never writes it (1.9.0 wrote the Steam offset on the EA build)");
    expect(mm::entry_point_write(0x146066c74, false) == mm::EntryPointWrite::unwritable,
           "a resolved address that is not writable memory is not written");
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
