#include "rows_test.hpp"

#include "core/apply/content/career_rules.hpp"
#include "core/apply/content/drs.hpp"
#include "core/apply/content/points.hpp"

namespace {

using swofty_core::registry::Context;
using swofty_core::registry::Store;

Value parse(const char *s) { return swofty::json::parse(s); }

bool has(const std::string &line, const std::string &needle) { return line.find(needle) != std::string::npos; }

void pick_tests()
{
    Value s2010 = parse(R"({"data_group":"f1_career_progressive","year":2010,
                            "box":"swofty.swofty-seasons-2010/sw_2010|game/f1_2025"})");
    Value s2011 = parse(R"({"data_group":"f1_career_progressive","year":2011,
                            "box":"swofty.swofty-seasons-2011/sw_2011|game/f1_2025"})");
    Value s2012 = parse(R"({"data_group":"f1_career_progressive","year":2012,
                            "box":"swofty.swofty-seasons-2012/sw_2012|game/f1_2025"})");
    Value s2014 = parse(R"({"data_group":"f1_career_progressive","year":2014})");
    Value pack2010 = parse(R"({"year":2010,"box":"swofty.swofty-seasons-2010/sw_2010"})");
    Value other2010 = parse(R"({"data_group":"f1_career_other","year":2010,"box":"mod.o/o_2010?"})");
    std::vector<std::pair<std::string, const Value *>> seasons = {{"swofty.progressive/2010", &s2010},
                                                                  {"swofty.progressive/2011", &s2011},
                                                                  {"swofty.progressive/2012", &s2012},
                                                                  {"swofty.progressive/2014", &s2014},
                                                                  {"swofty.swofty-seasons-2010/sw_2010", &pack2010},
                                                                  {"mod.o/2010", &other2010}};
    std::map<std::string, Value> packages = {
        {"swofty.swofty-seasons-2010/sw_2010",
         parse(R"({"entrant_list":"sw_f1_2010","rules":{"drs":"swofty.swofty-seasons-2010/sw10_no_drs",
                   "weekend_mode":"f1_1990_weekend","safety_car":"reduced"}})")},
        {"swofty.swofty-seasons-2011/sw_2011", parse(R"({"entrant_list":"sw_f1_2011"})")},
        {"mod.o/o_2010", parse(R"({"rules":{"drs":"mod.o/none"}})")}};
    content::PackageLookup lookup = [&](const std::string &id) -> const Value * {
        auto it = packages.find(id);
        return it == packages.end() ? nullptr : &it->second;
    };
    content::CareerSeasonFacts f{"f1_career_progressive_save:7", "f1_career_progressive", 2010};
    content::CareerBox b = content::career_box_pick(f, seasons, lookup);
    std::printf("      %s\n", b.line.c_str());
    check(b.season == "swofty.progressive/2010" && b.package == "swofty.swofty-seasons-2010/sw_2010" &&
              b.entrant_list == "sw_f1_2010",
          "a career in 2010 races the box its data group's 2010 season names, the first alternative that is a package");
    check(b.runs == std::vector<std::string>{"drs"} && b.left_out == std::vector<std::string>{"weekend_mode", "safety_car"} &&
              has(b.line, "rules drs; left out in a career: weekend_mode, safety_car"),
          "it runs the box's DRS rule and leaves the weekend helper and the race settings to the career");
    f.safety_car = true;
    b = content::career_box_pick(f, seasons, lookup);
    std::printf("      %s\n", b.line.c_str());
    check(b.runs == std::vector<std::string>{"drs", "safety_car"} && b.left_out == std::vector<std::string>{"weekend_mode"} &&
              has(b.line, "rules drs, safety_car; left out in a career: weekend_mode"),
          "SWOFTY_CAREER_SAFETY_CAR=1: the season runs the box's safety car rule too; the weekend helper stays left out");
    f.safety_car = false;
    f.year = 2011;
    b = content::career_box_pick(f, seasons, lookup);
    std::printf("      %s\n", b.line.c_str());
    check(b.package == "swofty.swofty-seasons-2011/sw_2011" && b.runs.empty() && has(b.line, "no rule a career runs"),
          "2011's box has no rules: nothing to run");
    f.year = 2012;
    b = content::career_box_pick(f, seasons, lookup);
    check(b.package.empty() && has(b.line, "is no content.package (a stock box, or its addon is off); the game's rules"),
          "a season whose year pack is off falls back to a stock box: the game's rules");
    f.year = 2013;
    b = content::career_box_pick(f, seasons, lookup);
    check(b.package.empty() && has(b.line, "no career.season record of data group f1_career_progressive is for 2013"),
          "a year the data group has no season for: the game's rules");
    f.year = 2014;
    b = content::career_box_pick(f, seasons, lookup);
    check(b.package.empty() && has(b.line, "the season names no box"), "a season without a box: the game's rules");
    f.year = 0;
    b = content::career_box_pick(f, seasons, lookup);
    check(b.package.empty() && has(b.line, "the season's year is not known yet"),
          "before the career's year is known: the game's rules");
    b = content::career_box_pick({"", "f1_career_progressive", 2010}, seasons, lookup);
    check(b.package.empty() && b.line == "no career is loaded", "no career: nothing");
    b = content::career_box_pick({"dc:1", "", 2010}, seasons, lookup);
    check(b.package.empty() && has(b.line, "has no data group"), "a career without a data group: the game's rules");
    b = content::career_box_pick({"dc:1", "f1_career_other", 2010}, seasons, lookup);
    check(b.package.empty() && has(b.line, "box mod.o/o_2010 has no entrant list"),
          "another career's data group reads only its own seasons; a package with no entrant list is no box");
    Value twin = parse(R"({"data_group":"f1_career_progressive","year":2010,"box":"swofty.swofty-seasons-2011/sw_2011"})");
    seasons.push_back({"mod.x/2010", &twin});
    b = content::career_box_pick({"dc:1", "f1_career_progressive", 2010}, seasons, lookup);
    check(b.package.empty() && has(b.line, "are both for 2010"), "two seasons of one data group for one year: the game's rules");

    check(content::box_alternatives("a/b?|c/d|e/f?") == std::vector<std::string>{"a/b", "c/d", "e/f"} &&
              content::box_alternatives("").empty(),
          "a box reference's alternatives drop their optional marks");
}

void host_label_tests()
{
    std::map<std::string, content::DrsGroup> groups = {
        {"swofty.swofty-seasons-2010/sw10_no_drs", {"swofty_drs_sw10", {200, 200, 200}}}};
    Value p = parse(R"({"entrant_list":"sw_f1_2010","rules":{"drs":"swofty.swofty-seasons-2010/sw10_no_drs"}})");
    std::vector<std::pair<std::string, const Value *>> packages = {{"swofty.swofty-seasons-2010/sw_2010", &p}};
    auto box = content::drs_pick("sw_f1_2010", packages, groups);
    auto career = content::drs_pick("sw_f1_2010", packages, groups, "driver_career_26");
    std::printf("      %s\n", career.line.c_str());
    check(box.group && career.group && career.group->label == "swofty_drs_sw10" && has(box.line, ": swofty_f1 DRS group") &&
              has(career.line, ": driver_career_26 DRS group swofty_drs_sw10"),
          "a career season picks the DRS group a press of its box picks, named for the career's setup mode");
    check(has(content::tyre_warmers_pick("sw_f1_2010", packages, "driver_career_26").line, "driver_career_26 keeps its own") &&
              has(content::tyre_change_pick("sw_f1_2010", packages, "driver_career_26").line, "driver_career_26 keeps its own") &&
              has(content::points_pick("sw_f1_2010", packages, {}, "driver_career_26").line, "driver_career_26 keeps its own"),
          "a rule the box lacks keeps the career's own, by its setup mode's name");
}

void store_tests()
{
    Store st;
    st.set_log([](const std::string &) {});
    for (const char *name : {"career.season", "content.package"})
        st.declare(name, parse(R"({"type":"object","x-version":"1.0"})"), true);
    st.add("swofty.progressive", "career.season", "2010",
           parse(R"({"data_group":"f1_career_progressive","year":2010,"box":"swofty.swofty-seasons-2010/sw_2010|game/f1_2025"})"),
           nullptr);
    st.add("swofty.progressive", "career.season", "2011",
           parse(R"({"data_group":"f1_career_progressive","year":2011,"box":"swofty.swofty-seasons-2011/sw_2011|game/f1_2025"})"),
           nullptr);
    st.add("swofty.swofty-seasons-2010", "content.package", "sw_2010",
           parse(R"({"entrant_list":"sw_f1_2010","rules":{"drs":"swofty.swofty-seasons-2010/sw10_no_drs"}})"), nullptr);
    Context ctx = st.context();
    ctx.career = "f1_career_progressive_save:7";
    ctx.mode = "driver_career";
    ctx.data_group = "f1_career_progressive";
    content::CareerBox b = content::career_box(st, ctx);
    check(b.package.empty() && has(b.line, "not known yet"), "the store's career with no year yet: the game's rules");
    ctx.year = 2010;
    ctx.has_year = true;
    b = content::career_box(st, ctx);
    std::printf("      %s\n", b.line.c_str());
    check(b.package == "swofty.swofty-seasons-2010/sw_2010" && b.entrant_list == "sw_f1_2010" &&
              b.runs == std::vector<std::string>{"drs"},
          "the store's effective 2010 season of the career's data group names the 2010 box and its DRS rule");
    ctx.year = 2011;
    b = content::career_box(st, ctx);
    check(b.package.empty() && has(b.line, "is no content.package"),
          "2011 with its year pack off: the stock fallback, the game's rules");
    ctx.data_group = "f1_career_2025";
    ctx.year = 2010;
    b = content::career_box(st, ctx);
    check(b.package.empty() && has(b.line, "no career.season record of data group f1_career_2025"),
          "a stock career never takes Progressive's seasons");
}

}

void career_rules_tests()
{
    pick_tests();
    host_label_tests();
    store_tests();
}
