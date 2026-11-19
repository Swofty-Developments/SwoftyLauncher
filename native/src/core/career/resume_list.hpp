#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include "core/career/resume_list_logic.hpp"

namespace career::resume_list {

struct Target {
    Selection sel;
    bool my_team = false, ask = false;
    size_t row = 0;
    std::string body;
};

bool wanted();
bool armed();
void set_armed(bool on);
size_t rows();
bool pressed(size_t index);
bool target_set();
bool take_target(bool my_team, Target &t);

}
