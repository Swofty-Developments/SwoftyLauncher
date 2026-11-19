// Host unit test for the list row press's checks: the wizard data group match, the save-safety refusals before PressSlot and
// the New Career redirect skip (resume_press_logic.hpp). Built and run by native/test/run_tests.sh resume_press.
#include "core/career/resume_press_logic.hpp"

#include <cstdio>

namespace rl = career::resume_list;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static rl::Facts good()
{
    rl::Facts f;
    f.row = 0;
    f.target = 7;
    f.listed = true;
    f.mode_found = true;
    f.game_mode = "driver_career_1990";
    f.mode_stat = f.want_stat = 81;
    f.want_group = "f1_career_1990";
    f.wizard_group = "f1_career_1990";
    f.stock[0] = {true, true, 7};
    f.stock[1] = {true, true, 3};
    f.stock[2] = {true, false, 0};
    f.card0 = f.stock[0];
    return f;
}

static void groups()
{
    expect(rl::group_ok("f1_career_1990", "f1_career_1990"), "a matching content group is ok");
    expect(!rl::group_ok("f1_career_1990", "f1_career_2026"), "the stock tile's group is not the content career's");
    expect(!rl::group_ok("f1_career_1990", ""), "an empty wizard group is not the content career's");
    expect(!rl::group_ok("", "f1_career_2025") && !rl::group_ok("", "f1_career_2026") && !rl::group_ok("", ""),
           "an unknown group is never ok, not even under a stock group");
    expect(rl::group_needs_write("f1_career_1990", "f1_career_2026"), "a differing group is written");
    expect(!rl::group_needs_write("f1_career_1990", "f1_career_1990"), "a matching group is left alone");
    expect(!rl::group_needs_write("", "f1_career_2026"), "an unknown career never writes the group");
    expect(rl::suffix_for("f1_career_2025", "x") == "25" && rl::suffix_for("f1_career_2026", "") == "26",
           "stock groups have their own suffix");
    expect(rl::suffix_for("f1_career_1990", "90") == "90" && rl::suffix_for("f1_career_1990", "").empty(),
           "a content group takes its content's suffix");
    expect(rl::stock_group("f1_career_2025") && !rl::stock_group("f1_career_1990"), "stock groups are the 2025 and 2026 ones");
}

static void cards()
{
    rl::Facts f = good();
    expect(rl::stock_has(f.stock, 7) && rl::stock_has(f.stock, 3), "a saved card is found by its slot id");
    expect(!rl::stock_has(f.stock, 9) && !rl::stock_has(f.stock, 0), "an unknown id and an empty card are not found");
    f.stock[0].read = false;
    expect(!rl::stock_has(f.stock, 7), "an unread card is not found");
}

static void opened()
{
    rl::Facts f = good();
    expect(rl::opened_refusal(f).empty(), "a clean screen with the slot on a card is pressed");
    f.stock[0] = {true, true, 4};
    expect(rl::opened_refusal(f).empty(), "a listed career not among the first three cards is pressed");
    f.listed = false;
    expect(rl::opened_refusal(f).find("slot id 7") != std::string::npos, "an unlisted slot off the cards is refused");
    f = good();
    f.busy = 1;
    expect(rl::opened_refusal(f).find("busy") != std::string::npos, "a busy screen is refused");
    f = good();
    f.pending = 3;
    expect(rl::opened_refusal(f).find("pending") != std::string::npos, "a pending action is refused");
}

static void press()
{
    rl::Facts f = good();
    expect(rl::press_refusal(f).empty(), "the Mansell row presses with its content group and mode");
    f.wizard_group = "f1_career_2026";
    std::string r = rl::press_refusal(f);
    expect(r.find("f1_career_2026") != std::string::npos && r.find("f1_career_1990") != std::string::npos,
           "a stale wizard group is refused naming both groups");
    f = good();
    f.mode_found = false;
    expect(rl::press_refusal(f).find("no game mode") != std::string::npos, "no matching game mode is refused");
    f = good();
    f.card0.has_save = false;
    expect(rl::press_refusal(f).find("new career") != std::string::npos, "a card with no save would start a new career");
    f = good();
    f.card0.read = false;
    expect(!rl::press_refusal(f).empty(), "an unread card 0 is refused");
    f = good();
    f.card0.id = 9;
    expect(rl::press_refusal(f).find("card 0 holds slot id 9") != std::string::npos, "card 0 must hold the target");
    f = good();
    f.pending = 2;
    expect(rl::press_refusal(f).find("pending") != std::string::npos, "a pending action is refused before the press");
    f = good();
    f.busy = 1;
    expect(rl::press_refusal(f).find("busy") != std::string::npos, "a busy screen is refused before the press");
    f = good();
    f.want_group.clear();
    f.wizard_group = "f1_career_2025";
    expect(rl::press_refusal(f).find("unknown") != std::string::npos, "a career with no group is refused under a stock group");
    f = good();
    f.game_mode.clear();
    expect(rl::press_refusal(f).find("no game mode") != std::string::npos, "a row with no game mode is refused");
    f = good();
    f.wizard_group = "f1_career_2025";
    f.want_group = "f1_career_2025";
    f.mode_stat = 70;
    r = rl::press_refusal(f);
    expect(r.find("resolves game mode statId 70 for group f1_career_2025") != std::string::npos &&
               r.find("driver_career_1990 (statId 81)") != std::string::npos,
           "a group whose mode differs from the row's game mode is refused naming both");
    f = good();
    f.want_stat = -1;
    expect(rl::press_refusal(f).find("not in the game's data") != std::string::npos, "an unresolvable game mode is refused");
    f = good();
    f.want_group = f.wizard_group = "f1_career_2025";
    f.game_mode = "driver_career_25";
    f.mode_stat = f.want_stat = 70;
    expect(rl::press_refusal(f).empty(), "a stock career with its own group and mode passes");
}

static void new_career()
{
    bool mt = false;
    expect(rl::is_new_career_route("swofty_new_career") && !rl::is_new_career_route("career"), "the New Career route is named");
    expect(rl::redirects(true, 2, "solo_career_continue_driver", mt, false), "the continue route redirects normally");
    expect(!rl::redirects(true, 2, "solo_career_continue_driver", mt, true), "after New Career the continue route is left alone");
    expect(!rl::redirects(true, rl::parse("").size(), "solo_career_continue_driver", mt, false),
           "zero rows redirect nothing and the list's own route is never redirected");
    expect(!rl::redirects(true, 2, rl::LIST_ROUTE, mt, false), "the list route is not a continue route, so it never loops");
    expect(rl::new_career_live(5000, 5000 + rl::NEW_CAREER_LIFE_MS) && !rl::new_career_live(5000, 5001 + rl::NEW_CAREER_LIFE_MS),
           "a New Career choice lasts its life");
    expect(!rl::new_career_live(0, 100), "no choice is never live");
}

static void setup_diag()
{
    expect(rl::setup_after_press(rl::SETUP_FLOW, 1000, 1000 + rl::SETUP_DIAG_MS),
           "the season setup flow just after a press is noted");
    expect(!rl::setup_after_press(rl::SETUP_FLOW, 1000, 1001 + rl::SETUP_DIAG_MS), "a later season setup flow is not noted");
    expect(!rl::setup_after_press("frontend_flow", 1000, 1500) && !rl::setup_after_press(rl::SETUP_FLOW, 0, 1500),
           "other flows and no press are not noted");
}

static void zero_rows()
{
    auto es = rl::parse(R"({"resume_list":[]})");
    expect(es.empty() && rl::find(es, 0) == nullptr, "an empty resume_list has no rows and no row 0");
    expect(rl::parse(R"({"resume_list":null})").empty(), "a null resume_list has no rows");
    std::vector<career::saves::SlotRead> none;
    rl::Entry e{0, "7", "", false, true};
    rl::Selection sel;
    expect(!rl::select(none, e, sel), "no listed slots selects nothing");
}

int main()
{
    groups();
    cards();
    opened();
    press();
    new_career();
    setup_diag();
    zero_rows();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
