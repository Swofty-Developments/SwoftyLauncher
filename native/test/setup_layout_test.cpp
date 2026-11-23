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
    expect(!c::check_layout(bad).ok, "steps on a slider outside 37 to 40 are refused");
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

    std::array<const char *, c::LAYOUT_TABS> keys{"lng_cs_aerodynamics_title", "lng_cs_transmission_title", "", "", "lng_cs_brakes_title", "", "",
                                                  "lng_cs_weight_distribution_title"};
    auto titles = c::title_overrides(l, keys);
    expect(titles.size() == 2 && titles[0].key == "lng_cs_aerodynamics_title" && titles[1].text == "Fuel mix", "titles go on the game's tab title keys");

    std::map<int, std::pair<float, float>> cells;
    std::map<int, int> steps_cells;
    for (int i = 0; i < c::SETUP_SLIDER_COUNT; i++) {
        cells[i] = {0.0f, 100.0f};
        steps_cells[i] = 5;
    }
    auto rd = [&](int s, float &mn, float &mx, int &st) { mn = cells[s].first; mx = cells[s].second; st = steps_cells[s]; return true; };
    auto wr = [&](int s, float mn, float mx, int st) { cells[s] = {mn, mx}; steps_cells[s] = st; };
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
    expect(sw.saved() == 0 && sw.active().empty() && cells[38].first == 0.0f && cells[40].second == 100.0f && steps_cells[39] == 5, "leaving puts every cell back, steps too");
    sw.leave(wr);
    expect(cells[37].first == 0.0f, "leaving twice is harmless");

    expect(std::fabs(c::display_value(0.84f, 1.0f, 8, 16) - 0.92f) < 1e-4f, "step 8 of 16 on 0.84..1.00 shows 0.92");
    expect(c::display_value(1, 2, 99, 10) == 2 && c::display_value(1, 2, -3, 10) == 1, "a step outside the range clamps");
    std::printf(failed ? "%d failed\n" : "all passed\n", failed);
    return failed ? 1 : 0;
}
