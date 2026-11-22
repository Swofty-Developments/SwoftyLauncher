#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime {

// Writes the load-stall report's session loader section (session_loader_logic.hpp) into out, naming code addresses
// with name, and returns its length. Only the load-stall watchdog's report calls it, one report at a time.
size_t session_loader_report(char *out, size_t cap, void (*name)(char *out, size_t cap, uint64_t at));

}
