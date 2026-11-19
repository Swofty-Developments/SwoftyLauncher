// Host unit test for the Resume Career gate: the needs table the launcher writes, the status line, the press state and the
// save slot key join (career_saves_logic.hpp slot_key_of). Built and run by native/test/run_tests.sh resume_gate.
#include "core/career/career_saves_logic.hpp"
#include "core/career/resume_gate_logic.hpp"

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
    expect(rg::status_line(e) == "Needs: Beta, Gamma 2.0.0, a.gone", "the status line lists the unmet add-ons");
    expect(rg::status_line(rg::find(t, "7")) == "All add-ons on", "a career with everything on says so");
    expect(rg::status_line(rg::find(t, "9")) == "All add-ons on", "a career with no add-ons says so");
    expect(rg::status_line(nullptr).empty(), "a career with no data has no status");
    rg::Entry many;
    for (int i = 0; i < 6; i++)
        many.needs.push_back({"x" + std::to_string(i), "N" + std::to_string(i), "", false, true});
    expect(rg::status_line(&many) == "Needs: N0, N1, N2, N3 and 2 more", "a long list is cut with a count");
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

int main()
{
    table_and_status();
    press_state();
    key_join();
    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
