#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include "core/apply/content/setup_steps.hpp"

// An addon's own rows in the car setup screen (docs/reference/std/setup-rows.md). The screen is a list the VM builds
// (garage_monitor_car_settings.setup_data, 0x140c17b80): one entry node per car_setup.sliders index, shown when the
// slider's index is in a group the VM publishes (Game_CarSetupVm_PublishSetupCounts 0x140c14220 calls
// Game_CarSetupVm_BuildSetupGroupList 0x140c14a30 with an index array per group; the group tabs are a list of up to 8,
// setup_group_types viewWidth 8). The stock groups hold only the sliders the game labels; sliders 37-40 (brake cooling
// and ducts) are in none and the game has no label for them. This header is the pure part of the feature: which rows
// are on in a session, which slider indexes a group gets, and the check of a record. The hooks that apply the result
// to the VM are not written yet. Pure, for the host test.

namespace swofty_core::apply::content {

inline constexpr int SETUP_GROUPS_MAX = 8;
inline constexpr int SETUP_GROUP_ENTRIES_MAX = 10;

struct SetupRowRecord {
    std::string id;
    int slider = -1;
    int group = -1;
    std::string name;
    std::string desc;
    bool in_scope = false;
};

struct SetupRowVerdict {
    bool ok = false;
    std::string why;
};

inline SetupRowVerdict check_setup_row(const SetupRowRecord &r, const std::vector<SetupRowRecord> &others)
{
    if (r.slider < 0 || r.slider >= SETUP_SLIDER_COUNT)
        return {false, "slider " + std::to_string(r.slider) + " is not one of the 42"};
    if (r.group < 0 || r.group >= SETUP_GROUPS_MAX)
        return {false, "group " + std::to_string(r.group) + " is not one of the 8 tabs"};
    if (r.name.empty())
        return {false, "a row needs a name"};
    for (const auto &o : others)
        if (o.id != r.id && o.slider == r.slider && o.in_scope && r.in_scope)
            return {false, "slider " + std::to_string(r.slider) + " is already taken by " + o.id};
    return {true, {}};
}

struct GroupIndexes {
    std::array<uint32_t, SETUP_GROUP_ENTRIES_MAX> idx{};
    uint32_t count = 0;
};

inline GroupIndexes merge_group(const GroupIndexes &stock, int group, const std::vector<SetupRowRecord> &rows)
{
    GroupIndexes out = stock;
    for (const auto &r : rows) {
        if (!r.in_scope || r.group != group || r.slider < 0 || r.slider >= SETUP_SLIDER_COUNT)
            continue;
        bool have = false;
        for (uint32_t i = 0; i < out.count; i++)
            have = have || out.idx[i] == uint32_t(r.slider);
        if (!have && out.count < SETUP_GROUP_ENTRIES_MAX)
            out.idx[out.count++] = uint32_t(r.slider);
    }
    return out;
}

// The row of a slider, or null: the VM's entry node of that slider takes the row's name and description.
inline const SetupRowRecord *row_of_slider(const std::vector<SetupRowRecord> &rows, int slider)
{
    for (const auto &r : rows)
        if (r.in_scope && r.slider == slider)
            return &r;
    return nullptr;
}

// A step in the row's own unit: min + (max - min) * step / steps, the number the screen shows for the slider.
inline float display_value(float min, float max, int step, int steps)
{
    if (steps <= 0)
        return min;
    int s = step < 0 ? 0 : (step > steps ? steps : step);
    return min + (max - min) * float(s) / float(steps);
}

}
