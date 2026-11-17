#pragma once

#include "core/apply/content/lower.hpp"

#include <string>
#include <vector>

namespace swofty_core::apply::content {

struct LogoReport {
    std::vector<std::string> lines;
    int built = 0, refused = 0;
};

void deliver_logos(const std::vector<LogoImage> &logos, LogoReport &report);

// Starts encoding, on a thread of its own, the slices the last build used whose PNGs changed since; deliver_logos waits
// for it before it builds. SWOFTY_LOGO_AHEAD=0 turns it off.
void encode_logos_ahead();

}
