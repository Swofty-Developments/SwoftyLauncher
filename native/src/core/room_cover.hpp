#pragma once
#include "hud_drawlist.hpp"

namespace runtime::room_cover {

bool setup();
bool active();
void step(const char *what, float fraction, float ceiling = -1.0f);
void set_subtitle(const char *text);
void paint(hud::Frame &frame, uint64_t now);

}
