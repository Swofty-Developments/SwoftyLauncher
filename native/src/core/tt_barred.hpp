#pragma once
#include <cstdint>

#include "hud_drawlist.hpp"

namespace runtime::tt_barred {

bool wants_overlay();
bool drawing();
void screen(const char *name);
void tick();
void paint(hud::Frame &frame, uint64_t now);

}
