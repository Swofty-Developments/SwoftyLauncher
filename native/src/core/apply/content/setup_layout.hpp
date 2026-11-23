#pragma once

#include <array>
#include <cstdint>
#include <optional>
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
inline constexpr int LAYOUT_STEP_FIRST = 37;
inline constexpr int LAYOUT_STEP_LAST = 40;
inline constexpr int LAYOUT_STEPS_MAX = 100;

struct LayoutRow {
    int slider = -1;
    std::string name;
    std::string desc;
    std::optional<float> min, max;
    std::optional<int> steps;
    std::string unit;
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
            if (r.steps) {
                if (r.slider < LAYOUT_STEP_FIRST || r.slider > LAYOUT_STEP_LAST)
                    return {false, "only the brake sliders 37 to 40 take their own steps"};
                if (*r.steps < 1 || *r.steps > LAYOUT_STEPS_MAX)
                    return {false, "a row's steps are 1 to 100"};
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
            if ((r.min && r.max) || r.steps)
                out.push_back({r.slider, r.min.value_or(0), r.max.value_or(0), r.steps.value_or(0), bool(r.min && r.max)});
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

// The displayed number of a step over a row's range.
inline float display_value(float min, float max, int step, int steps)
{
    if (steps <= 0)
        return min;
    int s = step < 0 ? 0 : (step > steps ? steps : step);
    return min + (max - min) * float(s) / float(steps);
}

}
