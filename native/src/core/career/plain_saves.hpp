#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <map>
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
    size_t careers = 0;
    uint64_t generation = 0;
};

// What a profile load works from: the newest save generation of a core session and the launcher's requests, taken at the start
// of the load (inject.json renamed to inject.applying.json). A replace of a container the tree holds as a registered mode's is
// answered when the root's loop hands that mode its bag.
struct Plan {
    bool on = false;
    std::optional<Manifest> newest;
    std::vector<Request> requests;
    std::vector<Result> results;
    std::vector<bool> handled;
    bool had_inject_file = false;
};

Plan make_plan();
const Entry *newest_entry(const Plan &plan, const std::string &name);
std::optional<std::vector<uint8_t>> fetch_newest(const Plan &plan, const Entry &entry);
// The request naming a container, or -1; fetch_request reads the generation it names (nullopt when not intact).
int request_for(const Plan &plan, const std::string &name);
std::optional<std::vector<uint8_t>> fetch_request(const Plan &plan, int index);
void request_done(Plan &plan, int index, bool ok, const std::string &detail);

// Rule 2 and rule 4 at the end of a profile load. `registered` holds the root children the game matched to a mode, `kept`
// the unclaimed ones the splice kept. Reads the store on the calling thread (small files, once a load).
// `registered` and `kept` map the root children the tree holds to the SHA-256 of their bytes ("" when not known).
std::vector<Added> at_load(Plan &plan, const std::map<std::string, std::string> &registered,
                           const std::map<std::string, std::string> &kept);
// The container and, for half of a mode's pair, its other half, as the generation holds them: every one intact or nullopt.
struct Part {
    std::string name, sha256;
    uint64_t careers = 0;
    std::vector<uint8_t> bytes;
};
std::optional<std::vector<Part>> fetch_group(uint64_t generation, const std::string &name);

// Count a StateLoadSession pass that found its session already ran. From the third pass of a game session on, write_async
// writes nothing (see LoadLoop), and one line in the loader log says so.
void note_already_ran();
// A StateLoadSession pass that did not find its session already ran: the count restarts and paused writes resume.
void note_moved_on();
bool writes_paused();

// Writes a generation on a worker thread; the caller's bytes are shared, never copied. Never blocks the save.
void write_async(const char *event, std::vector<Body> bodies);

}
