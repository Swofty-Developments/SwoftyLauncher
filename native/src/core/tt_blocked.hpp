#pragma once

#include <string>

namespace runtime::tt_blocked {

bool active();
bool refuse(const char *where);
bool refuses_command(const std::string &command);
const char *card_description();
void screen(const char *name);
void tick();

}
