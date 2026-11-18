#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "swofty_api.h"

// The addon settings (settings.cpp): swofty/settings.json's options, the addon API's settings calls and the MFD rows.
void settings_attach(SwoftyApi *a, const std::wstring &dir);
size_t settings_mfd_rows();
bool settings_mfd_step(size_t row, int dir);
// The id of the addon whose code holds `from` (empty when none), and in `dll` that module's name.
std::string settings_addon(const void *from, std::string &dll);
int settings_get(const char *id, uint32_t type, SwoftySettingValue *out);
int settings_watch(const char *id, SwoftySettingFn fn, void *user);
bool settings_command(uint32_t hash);
// An addon option's current state index and, for each state, its text, command and value (each list when asked);
// false when the addon has no such option. The hub tab picks a state by running its command, the path ADDON SETTINGS
// takes, so the profile, the watchers and the requests file follow.
bool settings_state(const std::string &addon, const std::string &option, size_t &current, std::vector<std::string> *texts,
                    std::vector<std::string> *commands, std::vector<std::string> *values);
