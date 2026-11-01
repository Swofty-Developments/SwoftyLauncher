#pragma once
#include "hud_drawlist.hpp"

namespace runtime::room_cover {

bool setup();
bool active();
void paint(hud::Frame &frame, uint64_t now);

}
