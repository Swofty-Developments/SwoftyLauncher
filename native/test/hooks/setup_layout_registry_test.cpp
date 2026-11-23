// Host unit test for the setup.layout registry's records (setup_layout_parse.hpp): the shipped example parses to its
// tab slots, rows and steps, a bad record is refused with a reason, and the layout a scope's facts select is the last
// whose within matches (owner, data_group, content).

#include "core/apply/content/setup_layout_parse.hpp"

#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>

namespace c = swofty_core::apply::content;
namespace h = swofty_core::hooks;

static int fails = 0;
static void expect(bool ok, const std::string &what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static swofty::json::Value read(const std::string &path)
{
    std::ifstream f(path, std::ios::binary);
    std::stringstream ss;
    ss << f.rdbuf();
    return swofty::json::parse(ss.str());
}

int main()
{
    const std::string dir = "../../packages/sdk/std/registries/setup.layout/examples/";
    auto doc = read(dir + "f1_1990_fuel.json");
    expect(doc.is_object() && doc["setup.layout"].is_object(), "the shipped example reads");
    c::ParsedLayout p;
    std::string why;
    bool ok = c::parse_setup_layout("f1_1990", doc["setup.layout"]["f1_1990"], p, &why);
    expect(ok, "the shipped example parses: " + why);
    expect(p.layout.sections.size() == 3 && p.layout.sections[0].tab == 1 && p.layout.sections[1].tab == 7 &&
               p.layout.sections[2].tab == 0,
           "tab names resolve to the game's slots");
    auto fuel = c::tab_indexes(p.layout, 0);
    expect(fuel.size() == 5 && fuel[0] == 26 && fuel[1] == 37 && fuel[4] == 40,
           "the fuel tab lists fuel load and sliders 37 to 40");
    auto swaps = c::cell_swaps(p.layout);
    int stepped = 0;
    for (const auto &s : swaps)
        stepped += s.steps == 20;
    expect(stepped == 4, "four rows carry 20 steps");
    expect(p.layout.sections[1].rows[0].min == 42.0f && p.layout.sections[1].rows[0].max == 40.0f,
           "a descending ballast row keeps its order");

    for (const char *bad : {"tab-out-of-range", "tab-misspelt", "rows-empty", "row-without-name", "steps-zero", "no-sections"}) {
        auto rec = read(dir + "must-fail/" + bad + ".json");
        c::ParsedLayout q;
        std::string reason;
        bool parsed = c::parse_setup_layout("x", rec["setup.layout"]["x"], q, &reason);
        expect(!parsed && !reason.empty(), std::string("must-fail ") + bad + " is refused: " + reason);
    }

    auto make = [](const char *id, const char *within) {
        auto rec = swofty::json::parse(
            std::string("{\"within\":") + within +
            ",\"sections\":[{\"tab\":1,\"title\":\"W\",\"rows\":[{\"slider\":\"front_wing\",\"name\":\"F\"}]}]}");
        c::ParsedLayout out;
        std::string reason;
        if (!c::parse_setup_layout(id, rec, out, &reason))
            std::printf("make %s: %s\n", id, reason.c_str());
        return out;
    };
    std::vector<c::ParsedLayout> all;
    all.push_back(c::ParsedLayout{});
    all.back() = make("everywhere", "null");
    all.push_back(make("owned", R"([{"owner":"swofty.f1-1990"}])"));
    all.push_back(make("career", R"([{"data_group":"f1_career_1990"}])"));

    h::Facts none;
    expect(c::pick_layout(all, &none) == 0, "a layout without within applies in a scope nothing else matches");
    h::Facts box;
    box.owners = {"swofty.f1-1990"};
    expect(c::pick_layout(all, &box) == 1, "the owner selector picks the addon's box");
    h::Facts career;
    career.owners = {"swofty.f1-1990"};
    career.career.emplace();
    career.career->data_group = "f1_career_1990";
    expect(c::pick_layout(all, &career) == 2, "the data_group selector picks the 1990 career, the last match winning");
    h::Facts other;
    other.owners = {"other.addon"};
    expect(c::pick_layout(all, &other) == 0, "another addon's session gets the unscoped layout only");
    all.erase(all.begin());
    expect(c::pick_layout(all, &other) == -1, "with no unscoped layout another addon's session keeps the game's screen");
    expect(c::pick_layout(all, nullptr) == -1, "no facts, no layout");

    if (fails) {
        std::printf("setup_layout_registry_test: %d failure(s)\n", fails);
        return 1;
    }
    std::printf("setup_layout_registry_test: ok\n");
    return 0;
}
