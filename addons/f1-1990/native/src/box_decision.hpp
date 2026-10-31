// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <string_view>

namespace f1990 {

constexpr const char *kBoxGrandPrix = "f1_1990_gp", *kBoxTimeTrial = "f1_1990_tt";

enum class Play { stock, career, box };

inline bool box_list_name(std::string_view name) { return name == kBoxGrandPrix || name == kBoxTimeTrial; }

inline Play decide_play(bool career_1990, bool box_list)
{
    if (career_1990)
        return Play::career;
    return box_list ? Play::box : Play::stock;
}

}
