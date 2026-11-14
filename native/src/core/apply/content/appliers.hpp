#pragma once

#include <optional>
#include <string>

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

// The mandatory tyre change rule the box with this entrant list runs: its content.package's
// rules.mandatory_tyre_change (none when the package has no rule), and the line to log.
struct TyreChangeBox {
    std::optional<bool> required;
    std::string line;
};
TyreChangeBox tyre_change_for_box(const std::string &entrant_list);

}
