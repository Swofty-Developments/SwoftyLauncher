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
   {"id":"a.old","name":"Gamma","version":"2.0.0","on":true,"at_version":false,"made_with":"1.0.4"},
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
    expect(e && rg::missing(*e) == std::vector<std::string>{"Beta", "a.gone"},
           "an add-on that is off or not installed is missing; one unknown by name uses its id");
    expect(e && rg::other_versions(*e) == std::vector<std::string>{"Gamma (saved with 1.0.4, installed 2.0.0)"},
           "an add-on on at another version names the version the career was saved with and the installed one");
    expect(rg::status_line(e) == "Missing: Beta, a.gone; Other version: Gamma (saved with 1.0.4, installed 2.0.0)",
           "the status line keeps missing add-ons and other versions apart");
    expect(rg::status_line(rg::find(t, "7")) == "All addons ready", "a career with everything on says so");
    expect(rg::status_line(rg::find(t, "9")) == "All addons ready", "a career with no add-ons says so");
    expect(rg::status_line(nullptr).empty(), "a career with no data has no status");
    rg::Entry many;
    for (int i = 0; i < 6; i++)
        many.needs.push_back({"x" + std::to_string(i), "N" + std::to_string(i), "", false, true});
    expect(rg::status_line(&many) == "Missing: N0, N1, N2, N3 and 2 more", "a long list is cut with a count");
    expect(rg::prompt_text(*e).find("does not have on: Beta, a.gone.") != std::string::npos &&
               rg::prompt_text(*e).find("other versions of: Gamma (saved with 1.0.4, installed 2.0.0).") != std::string::npos,
           "the prompt names the missing add-ons and the other versions");
    rg::Table only = rg::parse(R"({"resume_needs":[{"key":"1","addons":[
     {"id":"s.2018","name":"SWOFTY 2018","version":"1.0.5","on":true,"at_version":false,"made_with":"1.0.4"},
     {"id":"a.ss","name":"Silly Season","version":"1.0.0","on":true,"at_version":false,"wants":">=1.0.1"},
     {"id":"a.bare","name":"Bare","version":"3.0.0","on":true,"at_version":false}]}]})");
    const rg::Entry *v = rg::find(only, "1");
    expect(v && rg::missing(*v).empty(), "add-ons that are on are never missing");
    expect(rg::status_line(v) == "Other version: SWOFTY 2018 (saved with 1.0.4, installed 1.0.5), Silly Season (needs "
                                 ">=1.0.1, installed 1.0.0), Bare 3.0.0",
           "a career saved with other versions says so, not missing");
    expect(v && rg::dialog_text(*v) == "This career was saved with other versions of add-ons this profile has on: SWOFTY 2018 "
                                       "(saved with 1.0.4, installed 1.0.5), Silly Season (needs >=1.0.1, installed 1.0.0), "
                                       "Bare 3.0.0. Loading it anyway can break it.",
           "the dialog of a career saved with other versions says it does not lack an add-on");
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
    expect(rg::dialog_text(*bad).find("Beta, a.gone") != std::string::npos &&
               rg::dialog_text(*bad).find("press") == std::string::npos,
           "the dialog names the add-ons and does not tell the player to press again");
    expect(rg::loads("load_anyway") && !rg::loads("back") && !rg::loads("") && !rg::loads("yes"),
           "only the load_anyway answer loads");
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
    namespace rl = career::resume_list;
    auto es = rl::parse(R"({"resume_list":[
        {"index":0,"mode":"driver","key":"67cdf241e227f68c","ready":false,"status":"Missing: Beta"},
        {"index":1,"mode":"my_team","key":"7","ready":true,"status":"Ready"},
        {"index":2,"mode":"driver","key":"","ready":true,"status":"Ready"},
        {"mode":"driver","key":"9","ready":true}]})");
    expect(es.size() == 2, "entries without a key or an index are dropped");
    expect(es[0].index == 0 && !es[0].my_team && !es[0].ready && es[0].status == "Missing: Beta",
           "a not-ready driver entry parses");
    expect(es[1].my_team && es[1].ready, "a ready My Team entry parses");
    auto gm = rl::parse(R"({"resume_list":[{"index":0,"key":"7","group":"f1_career_1990","game_mode":"driver_career_1990"}]})");
    expect(gm.size() == 1 && gm[0].group == "f1_career_1990" && gm[0].game_mode == "driver_career_1990",
           "a row carries its data group and game mode");
    expect(rl::parse("").empty() && rl::parse("{}").empty() && rl::parse("nonsense").empty(), "no block gives no entries");
    expect(rl::find(es, 1) == &es[1] && !rl::find(es, 5), "an entry is found by its index");

    size_t i = 99;
    expect(rl::command_index("swofty_resume:12", i) && i == 12, "a row command names its index");
    expect(!rl::command_index("swofty_resume:", i) && !rl::command_index("swofty_resume:1x", i) &&
               !rl::command_index("slot_pressed", i),
           "other commands are not row commands");

    std::vector<cs::SlotRead> slots{slot(7, 0, "Mine", cs::FLAG_MY_TEAM, 50, 1, 4), slot(7, 0, "Solo7", 0, 40, 1, 4),
                                    slot(6, 0x67cdf241e227f68cULL, "Solo", 0, 90, 2, cs::NO_INDEX),
                                    slot(8, 0, "Hidden", cs::FLAG_HIDDEN, 99, 0, 0)};
    rl::Selection sel;
    expect(rl::select(slots, es[0], sel) && sel.id == 6 && sel.guid == 0x67cdf241e227f68cULL,
           "a guid key maps to its slot id and guid");
    expect(rl::select(slots, es[1], sel) && sel.id == 7 && sel.guid == 0,
           "the My Team entry maps to the My Team slot, not the Driver one");
    rl::Entry hidden{3, "8", "", false, true};
    expect(!rl::select(slots, hidden, sel), "a hidden slot is never selected");
    rl::Entry gone{4, "55", "", false, true};
    expect(!rl::select(slots, gone, sel), "a key no slot has selects nothing");

    bool mt = true;
    expect(rl::redirects(true, 2, "solo_career_continue_driver", mt) && !mt, "the Driver continue route redirects");
    expect(rl::redirects(true, 2, "solo_career_continue_my_team", mt) && mt, "the My Team continue route redirects");
    expect(!rl::redirects(false, 2, "solo_career_continue_driver", mt), "an unarmed screen redirects nothing");
    expect(!rl::redirects(true, 0, "solo_career_continue_driver", mt), "an empty list redirects nothing");
    expect(!rl::redirects(true, 2, "career_continue_driver", mt), "other routes are left alone");

    expect(rl::press(es[0]) == rl::Action::Ask && rl::press(es[1]) == rl::Action::Load, "not-ready rows ask, ready rows load");
    expect(rl::dialog_text(es[0]).find("Missing: Beta") != std::string::npos, "the dialog names the status");
    auto rows = rl::parse(R"({"resume_list":[
        {"index":0,"key":"8","ready":false,"loadable":false,"status":"Missing: F1 1990"},
        {"index":1,"key":"9","ready":false,"loadable":true,"status":"Missing: Beta"}]})");
    expect(rows.size() == 2 && !rows[0].loadable && rows[1].loadable && es[0].loadable,
           "loadable parses, and a row without it is loadable");
    expect(rl::press(rows[0]) == rl::Action::Refuse && rl::press(rows[1]) == rl::Action::Ask,
           "a row the launcher marks not loadable is refused; a loadable row that is not ready asks");
    expect(rl::refuse_text(rows[0]).find("Missing: F1 1990") != std::string::npos &&
               rl::refuse_text(rows[0]).find("anyway") == std::string::npos,
           "the refusal names the status and offers no Load Anyway");
    expect(!rl::expired(1000, 1000 + rl::TARGET_LIFE_MS) && rl::expired(1000, 1001 + rl::TARGET_LIFE_MS),
           "a target expires after its life");
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
