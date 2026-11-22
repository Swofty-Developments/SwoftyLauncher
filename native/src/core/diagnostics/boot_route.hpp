#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime {

// Writes the load-stall report's front end boot route section (boot_route_logic.hpp) into out, naming code addresses
// with name, and returns its length. Only the load-stall watchdog's report calls it, one report at a time.
size_t boot_route_report(char *out, size_t cap, void (*name)(char *out, size_t cap, uint64_t at));

}
