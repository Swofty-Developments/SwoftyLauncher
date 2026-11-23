#pragma once

#include <array>
#include <cstdint>
#include <optional>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#include "core/apply/content/setup_steps.hpp"

// A per-scope layout of the car setup screen (docs/reference/std/setup-layout.md). The screen is a list the VM builds
// (0x140c17b80, 0x140c14220): an entry node per car_setup.sliders index and up to 8 tabs, each a group of slider
// indexes the VM publishes through Game_CarSetupVm_BuildSetupGroupList 0x140c14a30. A slider that is in no group has no
// row, so a layout hides a slider by leaving it out and orders and groups the rest by listing them. Each entry node's
// text (name, desc, range_desc, value_format) and numbers (min, max, step) are VM data the core can write; the number
// the screen converts to a step is lerp(min, max, step / steps) over the global car_setup.sliders cells
// (0x14605a428 max, 0x14605a578 min, 0x14605a6c8 steps), read by the VM and by Car_Setup_SetSliderValue alike, so a
// layout's own range is a swap of the min and max cells while its scope is active. A row may also swap its steps cell, but
// only the four brake sliders (indexes 37 to 40): ApplyTuning divides the step by the same cell, so a layout that gives them their own
// step count must pin their F1Tuning min == max (the 1990 handling does) or the physics would follow the new fraction.
// min above max is allowed (a descending row). Pure, for the host test.

namespace swofty_core::apply::content {

inline constexpr int LAYOUT_TABS = 8;
inline constexpr int LAYOUT_TAB_ROWS = 10;
inline constexpr int LAYOUT_STEPS_MAX = 100;

struct LayoutRow {
    int slider = -1;
    std::string name;
    std::string desc;
    std::optional<float> min, max;
    std::optional<int> steps;
    std::string unit;
    std::string format;
    std::optional<int> decimals;
    bool group = false;
    std::vector<std::string> labels;
    std::optional<std::string> range_text;
};

struct LayoutSection {
    int tab = -1;
    std::string title;
    std::vector<LayoutRow> rows;
};

struct Layout {
    std::string id;
    std::vector<LayoutSection> sections;
};

struct LayoutVerdict {
    bool ok = false;
    std::string why;
};

inline LayoutVerdict check_layout(const Layout &l)
{
    if (l.sections.empty() || int(l.sections.size()) > LAYOUT_TABS)
        return {false, "a layout has 1 to 8 sections"};
    std::array<bool, SETUP_SLIDER_COUNT> used{};
    std::array<bool, LAYOUT_TABS> tabs{};
    for (const auto &s : l.sections) {
        if (s.tab < 0 || s.tab >= LAYOUT_TABS)
            return {false, "tab " + std::to_string(s.tab) + " is not one of the 8"};
        if (tabs[s.tab])
            return {false, "tab " + std::to_string(s.tab) + " is used by two sections"};
        tabs[s.tab] = true;
        if (s.title.empty())
            return {false, "a section needs a title"};
        if (s.rows.empty() || int(s.rows.size()) > LAYOUT_TAB_ROWS)
            return {false, "a section has 1 to 10 rows"};
        for (const auto &r : s.rows) {
            if (r.slider < 0 || r.slider >= SETUP_SLIDER_COUNT)
                return {false, "slider " + std::to_string(r.slider) + " is not one of the 42"};
            if (used[r.slider])
                return {false, "slider " + std::to_string(r.slider) + " is listed twice"};
            used[r.slider] = true;
            if (r.name.empty())
                return {false, "a row needs a name"};
            if (r.min && r.max && *r.min == *r.max)
                return {false, "a row's min and max must differ"};
            if (r.steps && (*r.steps < 1 || *r.steps > LAYOUT_STEPS_MAX))
                return {false, "a row's steps are 1 to 100"};
            if (r.decimals && (*r.decimals < 0 || *r.decimals > 3))
                return {false, "a row's decimals are 0 to 3"};
            if (!r.format.empty() && r.format.find("{v}") == std::string::npos)
                return {false, "a row's format needs {v}"};
            if (!r.labels.empty()) {
                if (r.labels.size() < 2 || int(r.labels.size()) > LAYOUT_STEPS_MAX + 1)
                    return {false, "a row's labels are 2 to 101"};
                for (const auto &t : r.labels)
                    if (t.empty())
                        return {false, "a label is empty"};
                if (r.steps && *r.steps != int(r.labels.size()) - 1)
                    return {false, "a row's steps must be one fewer than its labels"};
            }
            if (bool(r.min) != bool(r.max))
                return {false, "a row gives both min and max or neither"};
        }
    }
    return {true, {}};
}

inline std::array<bool, SETUP_SLIDER_COUNT> shown_sliders(const Layout &l)
{
    std::array<bool, SETUP_SLIDER_COUNT> shown{};
    for (const auto &s : l.sections)
        for (const auto &r : s.rows)
            if (r.slider >= 0 && r.slider < SETUP_SLIDER_COUNT)
                shown[r.slider] = true;
    return shown;
}

// The slider index of a layout's `slider` name: the SDK's SetupSlider names, and the car_setup.sliders labels the
// stock database uses for the same index. -1 for a name that is none of them.
inline int slider_index(const std::string &name)
{
    static const char *const names[SETUP_SLIDER_COUNT] = {"front_wing",
                                                          "rear_wing",
                                                          "diff_on_throttle",
                                                          "diff_off_throttle",
                                                          "diff_mid_corner",
                                                          "diff_high_speed",
                                                          "diff_preload",
                                                          "front_camber",
                                                          "rear_camber",
                                                          "front_toe",
                                                          "rear_toe",
                                                          "front_suspension",
                                                          "rear_suspension",
                                                          "front_ride_height",
                                                          "rear_ride_height",
                                                          "front_anti_roll_bar",
                                                          "rear_anti_roll_bar",
                                                          "brake_pressure",
                                                          "brake_bias",
                                                          "brake_migration_a",
                                                          "brake_migration_b",
                                                          "tyre_pressure_0",
                                                          "tyre_pressure_1",
                                                          "tyre_pressure_2",
                                                          "tyre_pressure_3",
                                                          "ballast",
                                                          "fuel_load",
                                                          "engine_braking",
                                                          "ramp_differential",
                                                          "inter_wet_pressure_0",
                                                          "inter_wet_pressure_1",
                                                          "inter_wet_pressure_2",
                                                          "inter_wet_pressure_3",
                                                          "inter_wet_pressure_4",
                                                          "inter_wet_pressure_5",
                                                          "inter_wet_pressure_6",
                                                          "inter_wet_pressure_7",
                                                          "brake_cooling_front",
                                                          "brake_cooling_rear",
                                                          "brake_duct_front",
                                                          "brake_duct_rear",
                                                          "brake_migration_c"};
    static const struct {
        const char *label;
        int index;
    } labels[] = {{"on_throttle", 2},
                  {"off_throttle", 3},
                  {"mid_corner", 4},
                  {"front_suspension_height", 13},
                  {"rear_suspension_height", 14},
                  {"front_antiroll_bar", 15},
                  {"rear_antiroll_bar", 16},
                  {"brake_bm", 19},
                  {"brake_bm_input_start", 20},
                  {"front_right_tyre_pressure", 21},
                  {"front_left_tyre_pressure", 22},
                  {"rear_right_tyre_pressure", 23},
                  {"rear_left_tyre_pressure", 24},
                  {"engine_braking_fraction", 27},
                  {"ramp_differential", 28},
                  {"brake_ch_front", 37},
                  {"brake_ch_rear", 38},
                  {"brake_bd_factor_front", 39},
                  {"brake_bd_factor_rear", 40},
                  {"brake_bm_input_end", 41}};
    for (int i = 0; i < SETUP_SLIDER_COUNT; i++)
        if (name == names[i])
            return i;
    for (const auto &l : labels)
        if (name == l.label)
            return l.index;
    return -1;
}

// The game's own text key of a slider's name (Game_CarSetupVm_PopulateGarageMonitorSettings' switch), "" where the
// switch shares its body (the diff and the inter and wet rows are not keyed one to one).
inline const char *name_key(int slider)
{
    static const char *const keys[SETUP_SLIDER_COUNT] = {"lng_cs_frong_wing_angle",
                                                         "lng_cs_rear_wing_angle",
                                                         "",
                                                         "",
                                                         "lng_cs_diff_adj_mid_corner",
                                                         "lng_cs_diff_adj_high_speed",
                                                         "lng_cs_diff_adj_preload",
                                                         "lng_cs_front_camber",
                                                         "lng_cs_rear_camber",
                                                         "lng_cs_front_toe",
                                                         "lng_cs_rear_toe",
                                                         "lng_cs_front_suspension",
                                                         "lng_cs_rear_suspension",
                                                         "lng_cs_front_suspension_height",
                                                         "lng_cs_rear_suspension_height",
                                                         "lng_cs_front_anti_roll_bar",
                                                         "lng_cs_rear_anti_roll_bar",
                                                         "lng_cs_brake_pressure",
                                                         "lng_cs_brake_bias",
                                                         "lng_cs_brake_bm",
                                                         "lng_cs_brake_bm_input_range_start",
                                                         "lng_cs_front_right_tyre_pressure",
                                                         "lng_cs_front_left_tyre_pressure",
                                                         "lng_cs_rear_right_tyre_pressure",
                                                         "lng_cs_rear_left_tyre_pressure",
                                                         "lng_cs_weight_balance",
                                                         "lng_cs_fuel_load",
                                                         "lng_cs_engine_braking",
                                                         "lng_cs_diff_adj_ramp_diff",
                                                         "lng_cs_front_right_tyre_pressure_inter",
                                                         "lng_cs_front_left_tyre_pressure_inter",
                                                         "lng_cs_rear_right_tyre_pressure_inter",
                                                         "lng_cs_rear_left_tyre_pressure_inter",
                                                         "lng_cs_front_right_tyre_pressure_wet",
                                                         "lng_cs_front_left_tyre_pressure_wet",
                                                         "lng_cs_rear_right_tyre_pressure_wet",
                                                         "lng_cs_rear_left_tyre_pressure_wet",
                                                         "lng_cs_brake_ch_front",
                                                         "lng_cs_brake_ch_rear",
                                                         "lng_cs_brake_bd_factor_front",
                                                         "lng_cs_brake_bd_factor_rear",
                                                         "lng_cs_brake_bm_input_range_end"};
    return slider >= 0 && slider < SETUP_SLIDER_COUNT ? keys[slider] : "";
}

// The text key of the unit string a slider's group shows ("lng_cs_*_range"), shared by the sliders of a group, "" where
// the switch has none for it.
inline const char *unit_key(int slider)
{
    if (slider == 37 || slider == 38)
        return "lng_cs_brake_ch_range";
    if (slider == 39 || slider == 40)
        return "lng_cs_brake_cd_range";
    if (slider == 26)
        return "lng_cs_fuel_range";
    if (slider == 25)
        return "lng_cs_weight_distribution_range";
    if (slider == 17)
        return "lng_cs_brakes_pressure_range";
    if (slider == 18)
        return "lng_cs_brakes_bias_range";
    return "";
}

// The game's own name of each tab slot (the group "name" BuildSetupGroupList sets, read from its table); a layout may
// give `tab` as the number or as this name. -1 for a name that is none of them.
inline constexpr const char *TAB_NAMES[LAYOUT_TABS] = {"fuel_load",  "aerodynamics", "transmission", "suspension_geometry",
                                                       "suspension", "brakes",       "tyres",        "weight_distribution"};

inline int tab_index(const std::string &name)
{
    for (int i = 0; i < LAYOUT_TABS; i++)
        if (name == TAB_NAMES[i])
            return i;
    return -1;
}

// The group array of a tab: the slider indexes the layout lists for it, in order, empty when it has no section.
inline std::vector<uint32_t> tab_indexes(const Layout &l, int tab)
{
    std::vector<uint32_t> out;
    for (const auto &s : l.sections)
        if (s.tab == tab)
            for (const auto &r : s.rows)
                out.push_back(uint32_t(r.slider));
    return out;
}

struct CellSwap {
    int slider;
    float min, max;
    int steps;
    bool range;
};

inline std::vector<CellSwap> cell_swaps(const Layout &l)
{
    std::vector<CellSwap> out;
    for (const auto &s : l.sections)
        for (const auto &r : s.rows)
            if ((r.min && r.max) || r.steps || !r.labels.empty())
                out.push_back({r.slider, r.min.value_or(0), r.max.value_or(0),
                               r.steps            ? *r.steps
                               : r.labels.empty() ? 0
                                                  : int(r.labels.size()) - 1,
                               bool(r.min && r.max)});
    return out;
}

struct LangSet {
    std::string key, text;
};

// The text the scope sets: tab titles on the game's own title keys (tab_title_keys), in tab order.
inline std::vector<LangSet> title_overrides(const Layout &l, const std::array<const char *, LAYOUT_TABS> &tab_title_keys)
{
    std::vector<LangSet> out;
    for (const auto &s : l.sections)
        if (s.tab >= 0 && s.tab < LAYOUT_TABS && tab_title_keys[s.tab] && *tab_title_keys[s.tab])
            out.push_back({tab_title_keys[s.tab], s.title});
    return out;
}

// The scope switch: the cells it changed, with the values to put back. enter() while active restores first, so a layout
// change or a second setup screen never stacks swaps; leave() with nothing active is a no-op.
struct CellSnapshot {
    int slider;
    float min, max;
    int steps;
};

class ScopeSwitch {
public:
    template <class Read, class Write> void enter(const Layout &l, Read &&read, Write &&write)
    {
        leave(write);
        for (const auto &c : cell_swaps(l)) {
            float cur_min = 0, cur_max = 0;
            int cur_steps = 0;
            if (!read(c.slider, cur_min, cur_max, cur_steps))
                continue;
            saved_.push_back({c.slider, cur_min, cur_max, cur_steps});
            write(c.slider, c.range ? c.min : cur_min, c.range ? c.max : cur_max, c.steps ? c.steps : cur_steps);
        }
        active_ = l.id;
    }
    template <class Write> void leave(Write &&write)
    {
        for (auto it = saved_.rbegin(); it != saved_.rend(); ++it)
            write(it->slider, it->min, it->max, it->steps);
        saved_.clear();
        active_.clear();
    }
    const std::string &active() const { return active_; }
    size_t saved() const { return saved_.size(); }

private:
    std::vector<CellSnapshot> saved_;
    std::string active_;
};

// The number text of a row: decimals as given, else the fewest (0 to 3) that show the row's step size exactly; a
// thousands separator when `group`.
inline int row_decimals(const LayoutRow &r, int steps)
{
    if (r.decimals)
        return *r.decimals;
    if (!r.min || !r.max || steps <= 0)
        return 0;
    double size = std::fabs(double(*r.max) - double(*r.min)) / double(steps);
    for (int d = 0; d < 3; d++) {
        double k = std::pow(10.0, d);
        if (std::fabs(size * k - std::round(size * k)) < 1e-3 &&
            std::fabs(double(*r.min) * k - std::round(double(*r.min) * k)) < 1e-3)
            return d;
    }
    return 3;
}

inline std::string group_digits(const std::string &number)
{
    size_t start = (!number.empty() && number[0] == '-') ? 1 : 0, dot = number.find('.');
    size_t end = dot == std::string::npos ? number.size() : dot;
    std::string out = number.substr(0, start);
    for (size_t i = start; i < end; i++) {
        out += number[i];
        size_t left = end - i - 1;
        if (left && left % 3 == 0)
            out += ',';
    }
    return out + number.substr(end);
}

inline std::string number_text(const LayoutRow &r, double v, int steps)
{
    char buf[64];
    std::snprintf(buf, sizeof buf, "%.*f", row_decimals(r, steps), v);
    std::string t = buf;
    if (t.rfind("-0", 0) == 0 && t.find_first_not_of("-0.") == std::string::npos)
        t.erase(0, 1);
    return r.group ? group_digits(t) : t;
}

inline std::string fill_format(const std::string &format, const std::string &value)
{
    std::string f = format.empty() ? std::string("{v}") : format, out;
    size_t at = f.find("{v}");
    if (at == std::string::npos)
        return f;
    return f.substr(0, at) + value + f.substr(at + 3);
}

inline int row_steps(const LayoutRow &r, int game_steps)
{
    return r.steps ? *r.steps : r.labels.empty() ? game_steps : int(r.labels.size()) - 1;
}

// The text beside the bar at a step: its label, else format over the number at that step.
inline std::string value_text(const LayoutRow &r, int step, int steps)
{
    int s = step < 0 ? 0 : step > steps ? steps : step;
    if (!r.labels.empty())
        return r.labels[size_t(s)];
    if (!r.min || !r.max)
        return {};
    double v = double(*r.min) + (double(*r.max) - double(*r.min)) * double(s) / double(steps > 0 ? steps : 1);
    return fill_format(r.format, number_text(r, v, steps));
}

// The right-hand text: range_text (empty hides it), else the row's own min to max through format.
inline std::string range_text_of(const LayoutRow &r, int steps)
{
    if (r.range_text)
        return *r.range_text;
    if (!r.labels.empty())
        return r.labels.front() + " - " + r.labels.back();
    if (!r.min || !r.max)
        return {};
    return fill_format(r.format, number_text(r, double(*r.min), steps) + " - " + number_text(r, double(*r.max), steps));
}

// Whether the row takes over the value or range text (the game's own text stays otherwise).
inline bool controls_text(const LayoutRow &r) { return !r.format.empty() || !r.labels.empty() || r.range_text || r.min; }

// The displayed number of a step over a row's range.
inline float display_value(float min, float max, int step, int steps)
{
    if (steps <= 0)
        return min;
    int s = step < 0 ? 0 : (step > steps ? steps : step);
    return min + (max - min) * float(s) / float(steps);
}

}
