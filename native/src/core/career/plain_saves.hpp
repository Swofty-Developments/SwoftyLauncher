#pragma once

#include <cstdint>
#include <memory>
#include <set>
#include <string>
#include <vector>

#include "core/career/plain_saves_logic.hpp"

// The plain save store (plain_saves.cpp), written from the save splice's hooks. With no plain_saves in settings.json
// every call here does nothing.
namespace career::plain_saves {

bool enabled();

// A container to inject at this load: put back from the newest save generation, or restored by a launcher request.
struct Added {
    std::string name;
    std::vector<uint8_t> bytes;
    bool replace = false;
    bool injected_by_request = false;
};

// Rule 2 and rule 4 at the end of a profile load. `registered` holds the root children the game matched to a mode, `kept`
// the unclaimed ones the splice kept. Reads the store on the calling thread (small files, once a load).
std::vector<Added> at_load(const std::set<std::string> &registered, const std::set<std::string> &kept);

// Writes a generation on a worker thread; the caller's bytes are shared, never copied. Never blocks the save.
void write_async(const char *event, std::vector<Body> bodies);

}
