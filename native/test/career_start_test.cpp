// Host unit test for the career start gate's pure parts (career_start.hpp): the route table maps
// the front-end career routes to a mode and a kind and leaves every other route alone, blocks are
// held per owner and mode, and the popup throttle lets one popup through per gap.
// native/test/run_tests.sh career_start builds and runs it.
#include "core/career/career_start.hpp"

#include <cstdio>
#include <string>

namespace cs = career::start;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static void route(const char *name, uint32_t mode, uint32_t kind)
{
    const cs::Route *r = cs::classify(name);
    expect(r && r->mode == mode && r->kind == kind, std::string("route ") + name);
}

int main()
{
    route("career_dlc_select_my_team", SWOFTY_CAREER_MY_TEAM, SWOFTY_CAREER_START_PICK);
    route("solo_career_continue_my_team", SWOFTY_CAREER_MY_TEAM, SWOFTY_CAREER_START_CONTINUE);
    route("career_wizard_my_team_career", SWOFTY_CAREER_MY_TEAM, SWOFTY_CAREER_START_NEW);
    route("career_wizard_my_team_season_length", SWOFTY_CAREER_MY_TEAM, SWOFTY_CAREER_START_NEW);
    route("to_manage_career_my_team", SWOFTY_CAREER_MY_TEAM, SWOFTY_CAREER_START_CONTINUE);
    route("career_dlc_select_driver", SWOFTY_CAREER_DRIVER, SWOFTY_CAREER_START_PICK);
    route("solo_career_continue_driver", SWOFTY_CAREER_DRIVER, SWOFTY_CAREER_START_CONTINUE);
    route("career_wizard_driver_career", SWOFTY_CAREER_DRIVER, SWOFTY_CAREER_START_NEW);
    route("career_wizard_driver_career_to_f1", SWOFTY_CAREER_DRIVER, SWOFTY_CAREER_START_NEW);
    route("to_manage_career_driver", SWOFTY_CAREER_DRIVER, SWOFTY_CAREER_START_CONTINUE);
    for (const char *other : {"back_1", "career", "career_continue_my_team", "team_hq_my_team_upgrades", "open",
                              "career_wizard_track_select", "customisation_my_team_decal_menu", "", "career_dlc_select"})
        expect(!cs::classify(other), std::string("not a start route: '") + other + "'");

    cs::Blocks b;
    expect(!b.find(SWOFTY_CAREER_MY_TEAM), "no block at first");
    expect(!b.set(1, "a", SWOFTY_CAREER_NONE, "t", "b"), "mode none is refused");
    expect(b.set(1, "a", SWOFTY_CAREER_MY_TEAM, "t1", "b1"), "addon a blocks my_team");
    expect(b.set(1, "a", SWOFTY_CAREER_MY_TEAM, "t2", "b2") && b.size() == 1, "a second block by a replaces its text");
    expect(b.find(SWOFTY_CAREER_MY_TEAM) && b.find(SWOFTY_CAREER_MY_TEAM)->title == "t2", "the block holds the new text");
    expect(!b.find(SWOFTY_CAREER_DRIVER), "driver_career stays open");
    expect(b.set(2, "c", SWOFTY_CAREER_MY_TEAM, "", ""), "addon c blocks my_team without a popup");
    expect(!b.clear(2, SWOFTY_CAREER_DRIVER), "c holds no driver block");
    expect(b.clear(1, SWOFTY_CAREER_MY_TEAM) && b.find(SWOFTY_CAREER_MY_TEAM)->by == "c", "a lifts its block, c's stays");
    b.drop_owner(2);
    expect(!b.find(SWOFTY_CAREER_MY_TEAM) && b.size() == 0, "dropping c's module drops its block");

    SwoftyCareerStart s = cs::make_start(*cs::classify("career_dlc_select_my_team"), true);
    expect(s.mode == SWOFTY_CAREER_MY_TEAM && s.kind == SWOFTY_CAREER_START_PICK && s.blocked == 1 &&
               std::string(s.route) == "career_dlc_select_my_team",
           "career.starting data carries mode, kind, blocked and route");

    uint64_t last = 0;
    expect(cs::due(10000, last, 1500), "the first popup is due");
    expect(!cs::due(10800, last, 1500), "a second press 0.8 s later shows no second popup");
    expect(cs::due(11600, last, 1500), "a press 1.6 s after the first shows the popup again");

    printf("%s\n", fails ? "career_start_test: FAILED" : "career_start_test: all ok");
    return fails ? 1 : 0;
}
