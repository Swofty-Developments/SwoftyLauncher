#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace swofty_core::apply::content {

void register_appliers();
void undress_teams(const char *why);

// The DRS group the box with this entrant list runs: the label of its content.package's rules.drs group if the
// boot apply added it (empty otherwise), and the line to log.
struct DrsBox {
    std::string label, line;
};
DrsBox drs_for_box(const std::string &entrant_list);

// The tyre warmer setting the box with this entrant list runs: its content.package's rules.tyre_warmers (none when the
// package has no rule), and the line to log.
struct WarmersBox {
    std::optional<bool> warmers;
    std::string line;
};
WarmersBox tyre_warmers_for_box(const std::string &entrant_list);

// The points system the box with this entrant list runs: its content.package's rules.points system if the boot apply
// added it (found false otherwise) as the game holds it (26 places, the 75 / 50 / 25 % levels, the fastest lap rule
// and bonus), and the line to log.
struct PointsBox {
    bool found = false;
    std::string label;
    std::vector<int32_t> full;
    std::vector<std::vector<int32_t>> reduced;
    int32_t bonus = 0;
    uint8_t rule = 0;
    std::string line;
};
PointsBox points_for_box(const std::string &entrant_list);

}
