// Host-native unit test for core/apply/content/setup_layout.hpp.

#include "core/apply/content/setup_layout.hpp"

#include <cmath>
#include <cstdio>
#include <map>

namespace c = swofty_core::apply::content;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static c::LayoutRow row(int slider, const char *name)
{
    c::LayoutRow r;
    r.slider = slider;
    r.name = name;
    return r;
}

int main()
{
    c::Layout l;
    l.id = "f1_1990";
    c::LayoutSection fuel{7, "Fuel mix", {}};
    for (int i = 0; i < 4; i++) {
        auto r = row(37 + i, "Mix");
        r.min = 0.8f + 0.04f * float(i);
        r.max = r.min.value() + 0.2f;
        fuel.rows.push_back(r);
    }
    c::LayoutSection aero{0, "Aero", {row(0, "Front wing"), row(1, "Rear wing")}};
    l.sections = {aero, fuel};
    expect(c::check_layout(l).ok, "a layout of aero and a fuel mix section is fine");

    auto shown = c::shown_sliders(l);
    expect(shown[0] && shown[1] && shown[37] && shown[40] && !shown[2] && !shown[25], "a slider left out is hidden");

    auto bad = l;
    bad.sections[0].rows.push_back(row(37, "again"));
    expect(!c::check_layout(bad).ok, "a slider listed twice is refused");
    bad = l;
    bad.sections[1].tab = 0;
    expect(!c::check_layout(bad).ok, "two sections on one tab are refused");
    bad = l;
    bad.sections[1].tab = 8;
    expect(!c::check_layout(bad).ok, "there are 8 tabs");
    bad = l;
    bad.sections[0].rows[0].slider = 42;
    expect(!c::check_layout(bad).ok, "slider 42 does not exist");
    bad = l;
    bad.sections[0].rows[0].name.clear();
    expect(!c::check_layout(bad).ok, "a row needs a name");
    bad = l;
    bad.sections[0].rows[0].min = 1.0f;
    expect(!c::check_layout(bad).ok, "min without max is refused");
    bad = l;
    bad.sections[0].rows[0].min = 2.0f;
    bad.sections[0].rows[0].max = 1.0f;
    expect(c::check_layout(bad).ok, "min above max is a descending row and is fine");
    bad.sections[0].rows[0].max = 2.0f;
    expect(!c::check_layout(bad).ok, "min equal to max is refused");
    bad = l;
    bad.sections[0].rows[0].steps = 10;
    expect(c::check_layout(bad).ok, "steps on any slider are fine");
    bad = l;
    bad.sections[1].rows[0].steps = 0;
    expect(!c::check_layout(bad).ok, "zero steps are refused");
    bad.sections[1].rows[0].steps = 101;
    expect(!c::check_layout(bad).ok, "more than 100 steps are refused");
    bad = l;
    bad.sections[0].title.clear();
    expect(!c::check_layout(bad).ok, "a section needs a title");
    bad = l;
    bad.sections.clear();
    expect(!c::check_layout(bad).ok, "an empty layout is refused");

    std::array<const char *, c::LAYOUT_TABS> keys{
        "lng_cs_aerodynamics_title",       "lng_cs_transmission_title", "", "", "lng_cs_brakes_title", "", "",
        "lng_cs_weight_distribution_title"};
    auto titles = c::title_overrides(l, keys);
    expect(titles.size() == 2 && titles[0].key == "lng_cs_aerodynamics_title" && titles[1].text == "Fuel mix",
           "titles go on the game's tab title keys");

    std::map<int, std::pair<float, float>> cells;
    std::map<int, int> steps_cells;
    for (int i = 0; i < c::SETUP_SLIDER_COUNT; i++) {
        cells[i] = {0.0f, 100.0f};
        steps_cells[i] = 5;
    }
    auto rd = [&](int s, float &mn, float &mx, int &st) {
        mn = cells[s].first;
        mx = cells[s].second;
        st = steps_cells[s];
        return true;
    };
    auto wr = [&](int s, float mn, float mx, int st) {
        cells[s] = {mn, mx};
        steps_cells[s] = st;
    };
    for (auto &r : l.sections[1].rows)
        r.steps = 20;
    c::ScopeSwitch sw;
    sw.enter(l, rd, wr);
    expect(sw.active() == "f1_1990" && sw.saved() == 4 && std::fabs(cells[38].first - 0.84f) < 1e-4f && cells[0].second == 100.0f,
           "entering swaps only the cells of rows with a range");
    sw.enter(l, rd, wr);
    expect(sw.saved() == 4 && std::fabs(cells[38].first - 0.84f) < 1e-4f, "entering again does not stack swaps");
    expect(steps_cells[37] == 20 && steps_cells[0] == 5, "a row's steps swap its steps cell only");
    sw.leave(wr);
    expect(sw.saved() == 0 && sw.active().empty() && cells[38].first == 0.0f && cells[40].second == 100.0f &&
               steps_cells[39] == 5,
           "leaving puts every cell back, steps too");
    sw.leave(wr);
    expect(cells[37].first == 0.0f, "leaving twice is harmless");

    expect(std::fabs(c::display_value(0.84f, 1.0f, 8, 16) - 0.92f) < 1e-4f, "step 8 of 16 on 0.84..1.00 shows 0.92");
    expect(c::display_value(1, 2, 99, 10) == 2 && c::display_value(1, 2, -3, 10) == 1, "a step outside the range clamps");
    expect(c::slider_index("brake_duct_front") == 39 && c::slider_index("brake_bd_factor_front") == 39 &&
               c::slider_index("front_antiroll_bar") == 15 && c::slider_index("ballast") == 25 && c::slider_index("nope") == -1,
           "slider names resolve by the SDK name and by the database label");
    expect(c::tab_index("aerodynamics") == 1 && c::tab_index("fuel_load") == 0 && c::tab_index("wings") == -1,
           "tab names resolve");
    auto t7 = c::tab_indexes(l, 7);
    expect(t7.size() == 4 && t7[0] == 37 && t7[3] == 40 && c::tab_indexes(l, 3).empty(),
           "a tab's group array is its rows in order");
    expect(std::string(c::name_key(37)) == "lng_cs_brake_ch_front" && std::string(c::name_key(2)).empty() &&
               std::string(c::unit_key(39)) == "lng_cs_brake_cd_range",
           "the text keys of the fuel rows are the game's");
    c::LayoutRow tr;
    tr.slider = 11;
    tr.name = "Spring";
    tr.min = 1050.0f;
    tr.max = 1350.0f;
    tr.steps = 6;
    tr.format = "{v} lb/in";
    tr.group = true;
    expect(c::value_text(tr, 0, 6) == "1,050 lb/in" && c::value_text(tr, 6, 6) == "1,350 lb/in" &&
               c::value_text(tr, 3, 6) == "1,200 lb/in",
           "format with a thousands separator over lerp(min, max, step / steps)");
    expect(c::range_text_of(tr, 6) == "1,050 - 1,350 lb/in", "an absent range_text is min to max through format");
    tr.range_text = "";
    expect(c::range_text_of(tr, 6).empty(), "an empty range_text hides the range");
    tr.range_text = "Soft to firm";
    expect(c::range_text_of(tr, 6) == "Soft to firm", "a string range_text replaces it");
    c::LayoutRow dr;
    dr.slider = 0;
    dr.name = "Flap";
    dr.min = 0.0f;
    dr.max = 1.0f;
    dr.steps = 4;
    dr.decimals = 2;
    dr.format = "Blade {v}";
    expect(c::value_text(dr, 1, 4) == "Blade 0.25", "decimals fix the digits");
    dr.decimals.reset();
    expect(c::value_text(dr, 1, 4) == "Blade 0.25" && c::value_text(dr, 2, 4) == "Blade 0.50",
           "absent decimals show the step size exactly");
    c::LayoutRow lr;
    lr.slider = 2;
    lr.name = "Gear";
    lr.labels = {"Short", "Medium", "Long", "Very long"};
    expect(c::row_steps(lr, 5) == 3 && c::value_text(lr, 2, 3) == "Long" && c::value_text(lr, 9, 3) == "Very long",
           "labels set the steps and replace the number");
    expect(c::range_text_of(lr, 3) == "Short - Very long", "labelled rows range from the first to the last label");
    c::Layout ll = l;
    ll.sections[0].rows[0].labels = {"A", "B", "C"};
    ll.sections[0].rows[0].steps = 5;
    expect(!c::check_layout(ll).ok, "steps that disagree with the labels are refused");
    ll.sections[0].rows[0].steps = 2;
    expect(c::check_layout(ll).ok, "steps that agree with the labels are fine");
    ll.sections[0].rows[0].labels = {"A"};
    ll.sections[0].rows[0].steps.reset();
    expect(!c::check_layout(ll).ok, "one label is refused");
    ll = l;
    ll.sections[0].rows[0].decimals = 4;
    expect(!c::check_layout(ll).ok, "decimals above 3 are refused");
    ll = l;
    ll.sections[0].rows[0].format = "no number";
    expect(!c::check_layout(ll).ok, "a format needs {v}");
    std::printf(failed ? "%d failed\n" : "all passed\n", failed);
    return failed ? 1 : 0;
}
