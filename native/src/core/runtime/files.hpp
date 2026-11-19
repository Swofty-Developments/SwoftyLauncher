#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

#include "core/runtime/json.hpp"

namespace runtime {

// The core's configuration (swofty/assets.json), the folders it reads and writes, and whole-file IO.
const json::Value &config();
const std::string &config_source();
const std::wstring &game_root();
const std::wstring &swofty_root();
const std::wstring &state_root();
// Where the core writes the files the launcher collects (laps, the Time Trial mailbox, replay copies): the launcher's
// data folder (state_root), or swofty\ when the core can't write there. The game can't create files in swofty\ of an
// EA app copy under Program Files.
const std::wstring &files_root();
// 0 when the game can create files in `dir` (a folder path ending in a backslash, made if it is missing), else the
// Win32 error. The probe file (storage_logic::PROBE) is not kept.
uint32_t write_refusal(const std::wstring &dir);
std::string read_file(const std::wstring &path, size_t limit = size_t(512) << 20);
bool write_file(const std::wstring &path, const std::string &data);
bool file_size(const std::wstring &path, uint64_t *size);
std::wstring wide(const std::string &s);
std::string narrow(const std::wstring &w);
std::wstring under_swofty(const std::string &rel);
std::string lower(std::string s);

}
