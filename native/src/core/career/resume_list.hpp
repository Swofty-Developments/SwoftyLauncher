#pragma once

#include <cstdint>
#include <vector>

#include "core/career/resume_list_logic.hpp"

namespace career::resume_list {

bool wanted();
bool armed();
void set_armed(bool on);
bool target_set();
void set_target(const Selection &s, bool my_team);
bool take_target(bool my_team, Selection &s);
void begin_list(std::vector<Row> rows);
bool take_list();
std::vector<Row> shown();

}
