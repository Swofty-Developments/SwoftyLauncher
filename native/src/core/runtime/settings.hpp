#pragma once
#include "swofty_api.h"

#include <cstddef>
#include <cstdint>
#include <string>

// The addons' settings (runtime/settings.cpp): the options settings.json lists for each addon, the commands that set
// them from the ADDON SETTINGS page and the MFD, and which addon a code address belongs to.
void settings_attach(SwoftyApi *a, const std::wstring &dir);
// The settings.json id of the addon whose module holds `from`, or "" when settings.json names none; its dll in `dll`.
std::string settings_addon(const void *from, std::string &dll);
int settings_get(const char *id, uint32_t type, SwoftySettingValue *out);
int settings_watch(const char *id, SwoftySettingFn fn, void *user);
// Runs the settings command whose name hashes (djb2) to `hash`; false when no command has that name.
bool settings_command(uint32_t hash);
size_t settings_mfd_rows();
bool settings_mfd_step(size_t row, int dir);
