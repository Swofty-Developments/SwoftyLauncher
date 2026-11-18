#pragma once

#include <cstdint>
#include <string>

// The database first-read probe (db_first_read_logic.hpp). gamedb.cpp arms it once its Gdb_Load_GameDatabase wrap is
// in, tells it when the load returned and when the core's edits ended, and tells it why when the wrap is not installed.
namespace runtime::db_first_read {

void arm(uintptr_t lookup);
void cannot(const std::string &why);
void loaded();
void edited();

}
