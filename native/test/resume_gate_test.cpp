// Host unit test for the Resume Career gate: the needs table the launcher writes, the status line, the press state and the
// save slot key join (career_saves_logic.hpp slot_key_of). Built and run by native/test/run_tests.sh resume_gate.
#include "core/career/career_saves_logic.hpp"
#include "core/career/resume_gate_logic.hpp"
#include "core/career/resume_list_logic.hpp"

#include <cstdio>

namespace rg = career::resume_gate;
namespace cs = career::saves;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static const char *SETTINGS = R"({"schema":1,"resume_needs":[
 {"key":"67cdf241e227f68c","career":"driver_career:67cdf241e227f68c","addons":[
   {"id":"a.on","name":"Alpha","version":"1.0.0","on":true,"at_version":true},
   {"id":"a.off","name":"Beta","version":"1.0.0","on":false,"at_version":true},
   {"id":"a.old","name":"Gamma","version":"2.0.0","on":true,"at_version":false},
   {"id":"a.gone","version":"","on":false,"at_version":false}]},
 {"key":"7","career":"my_team:7","addons":[{"id":"a.on","name":"Alpha","version":"1.0.0","on":true,"at_version":true}]},
 {"key":"9","career":"x","addons":[]},
 {"career":"no key","addons":[]}]})";

static void table_and_status()
{
    rg::Table t = rg::parse(SETTINGS);
    expect(t.present && t.entries.size() == 3, "an entry with no key is skipped");
    expect(!rg::parse("").present && !rg::parse("{}").present && !rg::parse("garbage").present,
           "no resume_needs, an empty file and garbage leave the table absent");
    expect(rg::find(t, "no key") == nullptr && rg::find(t, "") == nullptr && rg::find(t, "123") == nullptr,
           "an unknown or empty key finds nothing");
    const rg::Entry *e = rg::find(t, "67cdf241e227f68c");
    expect(e && rg::unmet(*e).size() == 3, "off, another version and not installed are each unmet");
    expect(e && rg::unmet(*e)[0] == "Beta" && rg::unmet(*e)[1] == "Gamma 2.0.0" && rg::unmet(*e)[2] == "a.gone",
           "an add-on is named; one on at another version carries the installed version; one unknown by name uses its id");
    expect(rg::status_line(e) == "Missing: Beta, Gamma 2.0.0, a.gone", "the status line lists the unmet add-ons");
    expect(rg::status_line(rg::find(t, "7")) == "All addons ready", "a career with everything on says so");
    expect(rg::status_line(rg::find(t, "9")) == "All addons ready", "a career with no add-ons says so");
    expect(rg::status_line(nullptr).empty(), "a career with no data has no status");
    rg::Entry many;
    for (int i = 0; i < 6; i++)
        many.needs.push_back({"x" + std::to_string(i), "N" + std::to_string(i), "", false, true});
    expect(rg::status_line(&many) == "Missing: N0, N1, N2, N3 and 2 more", "a long list is cut with a count");
    expect(rg::prompt_text(*e).find("Beta, Gamma 2.0.0, a.gone") != std::string::npos, "the prompt names the add-ons");
}

static void press_state()
{
    rg::Table t = rg::parse(SETTINGS);
    rg::State s;
    const rg::Entry *bad = rg::find(t, "67cdf241e227f68c"), *good = rg::find(t, "7");
    expect(rg::press(good, "7", s) == rg::Verdict::Load, "a career with everything on loads at once");
    expect(rg::press(nullptr, "5", s) == rg::Verdict::Load, "a career with no data is never gated");
    expect(rg::press(bad, "67cdf241e227f68c", s) == rg::Verdict::Block, "the first press of a mismatched career is held");
    expect(rg::press(bad, "67cdf241e227f68c", s) == rg::Verdict::Load, "the second press loads it anyway");
    expect(rg::press(bad, "67cdf241e227f68c", s) == rg::Verdict::Block, "the load used the pending press up");
    rg::screen_changed(s);
    expect(rg::press(bad, "67cdf241e227f68c", s) == rg::Verdict::Block, "leaving the screen clears the pending press");
    rg::State other;
    other.armed = "99";
    expect(rg::press(bad, "67cdf241e227f68c", other) == rg::Verdict::Block && other.armed == "67cdf241e227f68c",
           "a press of another career moves the pending press to it");
}

static void dialog_and_title()
{
    rg::Table t = rg::parse(SETTINGS);
    const rg::Entry *bad = rg::find(t, "67cdf241e227f68c"), *good = rg::find(t, "7");
    expect(rg::dialog_text(*bad).find("Beta, Gamma 2.0.0, a.gone") != std::string::npos &&
               rg::dialog_text(*bad).find("press") == std::string::npos,
           "the dialog names the add-ons and does not tell the player to press again");
    expect(rg::loads("load_anyway") && !rg::loads("back") && !rg::loads("") && !rg::loads("yes"),
           "only the load_anyway answer loads");
    expect(rg::card_title("DRIVER CAREER", *good) == "DRIVER CAREER - ALL ADDONS READY", "a card title carries the status");
    expect(rg::card_title("DRIVER CAREER", *bad) == "DRIVER CAREER - MISSING: BETA, GAMMA 2.0.0, A.GONE",
           "an unmet card title lists the add-ons in capitals");
}

static void key_join()
{
    std::vector<cs::SlotRead> slots(3);
    slots[0].id = 1, slots[0].guid = 0x67cdf241e227f68cull, slots[0].details = true;
    slots[1].id = 1, slots[1].guid = 0xabcull, slots[1].details = true, slots[1].flags = cs::FLAG_MY_TEAM;
    slots[2].id = 2, slots[2].guid = 0, slots[2].details = true;
    expect(cs::slot_key_of(slots, 1, false) == "67cdf241e227f68c", "a driver card joins its slot by id and the guid key");
    expect(cs::slot_key_of(slots, 1, true) == "0000000000000abc", "a My Team card joins the My Team slot of the same id");
    expect(cs::slot_key_of(slots, 2, false) == "2", "a slot with no guid keys by its id");
    expect(cs::slot_key_of(slots, 3, false).empty(), "a slot the core did not list has no key");
    slots[0].details = false;
    expect(cs::slot_key_of(slots, 1, false).empty(), "a slot whose details did not read has no key");
}

static cs::SlotRead slot(uint64_t id, uint64_t guid, const char *name, uint8_t flags, uint64_t date, uint32_t season,
                         uint32_t weekend)
{
    cs::SlotRead s;
    s.id = id;
    s.guid = guid;
    s.details = true;
    s.name = name;
    s.flags = flags;
    s.date = date;
    s.season = season;
    s.weekend = weekend;
    return s;
}

static void list_rows()
{
    rg::Table t = rg::parse(SETTINGS);
    std::vector<cs::SlotRead> slots{slot(7, 0, "Mine", cs::FLAG_MY_TEAM, 50, 1, 4), slot(12, 0, "Old", 0, 10, 0, 0),
                                    slot(6, 0x67cdf241e227f68cULL, "Solo", 0, 90, 2, cs::NO_INDEX),
                                    slot(8, 0, "Hidden", cs::FLAG_HIDDEN, 99, 0, 0), slot(5, 0, "Undated", 0, 0, 0, 0)};
    auto rows = career::resume_list::rows(slots, t);
    expect(rows.size() == 3, "hidden and undated slots are not listed");
    expect(rows[0].name == "Solo" && rows[1].name == "Mine" && rows[2].name == "Old", "rows are newest first");
    expect(!rows[0].ready && rows[0].status.rfind("Missing: Beta", 0) == 0, "a row with missing add-ons says Missing");
    expect(rows[1].ready && rows[1].status == "All addons ready", "a row with everything on is ready");
    expect(rows[2].ready && rows[2].status.empty(), "a row with no data is ready and has no status");
    expect(rows[1].detail == "My Team - Season 2, Race 5", "the detail line names mode, season and race");
    expect(rows[0].detail == "Driver Career - Season 3", "an unknown race is left out");
    expect(career::resume_list::press(rows[0]) == career::resume_list::Action::Ask, "a mismatched row asks first");
    expect(career::resume_list::press(rows[1]) == career::resume_list::Action::Load, "a ready row loads at once");
    rg::Table modes = rg::parse(R"({"resume_needs":[{"key":"6","mode_label":"F1 1990","addons":[]}]})");
    std::vector<cs::SlotRead> m1990{slot(6, 0, "Senna", 0, 5, 0, 1)};
    auto labelled = career::resume_list::rows(m1990, modes);
    expect(labelled[0].detail == "F1 1990 - Season 1, Race 2" && labelled[0].mode == "F1 1990",
           "an addon mode row carries the launcher's mode label");
    auto sel = career::resume_list::selection_of(rows[0]);
    expect(sel.id == 6 && sel.guid == 0x67cdf241e227f68cULL, "a row hands the registry its slot id and guid");
    expect(cs::slot_key(sel.id, sel.guid) == rows[0].key, "the hand-off names the slot the row's key names");
    std::vector<cs::SlotRead> tie{slot(3, 0, "B", 0, 5, 0, 0), slot(2, 0, "A", 0, 5, 0, 0)};
    auto tied = career::resume_list::rows(tie, t);
    expect(tied[0].id == 2 && tied[1].id == 3, "equal dates order by slot id");
}

int main()
{
    table_and_status();
    press_state();
    key_join();
    dialog_and_title();
    list_rows();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
