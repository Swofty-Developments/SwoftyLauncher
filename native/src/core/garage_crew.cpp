#include "garage_crew_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// Garage crew guard (re/db/globals.tsv site_Game_GarageCrew_SpawnVector).
//
// Game_GarageCrewModule_SpawnGarageCrew collects the session's assignment records for one garage into a vector of
// 16-byte entries {record, slot}, then creates one crew instance per entry and stores it at [rsp + index*8 + 0x290]
// with no bound. The array holds the two cars a garage has; the slot after it holds the vector's own pointer, which
// the epilogue frees through GAME_DEFAULT. A team that fields a third car puts a third record (slot 2) in its garage,
// the third crew pointer overwrites the vector pointer, and the free fails with "Invalid address for this heap" as the
// session loads. Sess_Set_GarageSlotEntrant already ignores slot >= 2, so only the crew spawn trips over it.
//
// At the site (rcx = vector begin, r13 = vector end, ebp = garage id) the guard keeps the first two entries whose slot
// is below 2 and lowers r13 over the rest. The extra car still races; its garage crew is not spawned.
//
// The crew instance of a garage is built from the team's crew template, found through the session's team tables
// ([[module + 0x48] + 0x2c8 + kind * 0x10], row team * 0x10 + 8, the record's kind at +0x58 and team at +0x5a); the
// function reads [template + 0x560] with no null test, so a car whose team has no template faults the session load
// at SpawnGarageCrew+0xce0. The guard drops those entries from the vector as well; the module is at [rsp + 0xc0].
//
// SWOFTY_GARAGE_CREW_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace gc = runtime::garage_crew;

constexpr const char *MIXIN = "swofty_core.dll (garage crew guard)";
constexpr const char *SITE = "site_Game_GarageCrew_SpawnVector";
constexpr size_t MAX_ENTRIES = 256;
constexpr size_t REC_DRIVER = 0x52;
constexpr size_t REC_CAR_NUMBER = 0x83;
constexpr int LOG_FIRST = 8;

bool offered = false, settled = false, wanted = false;
std::atomic<int> dropped{0};
std::vector<uint32_t> seen;

struct Log {
    uint32_t garage;
};

bool read_memory(uint64_t address, void *out, size_t n, void *)
{
    if (!runtime::readable(reinterpret_cast<const void *>(address), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

void log_dropped(void *ctx, uint64_t record, uint32_t slot, gc::Drop why)
{
    uint32_t garage = static_cast<Log *>(ctx)->garage;
    uint16_t driver = 0xffff, team = 0xffff;
    uint8_t number = 0;
    if (record) {
        mem::read(record + REC_DRIVER, driver);
        mem::read(record + gc::REC_TEAM, team);
        mem::read(record + REC_CAR_NUMBER, number);
    }
    uint32_t key = uint32_t(driver) << 16 | team;
    for (uint32_t k : seen)
        if (k == key)
            return;
    seen.push_back(key);
    int n = ++dropped;
    if (n > LOG_FIRST)
        return;
    const char *tail = n == LOG_FIRST ? " (further drops are not logged)" : "";
    if (why == gc::Drop::no_template)
        swofty::log("[garage] garage %u: team %u car #%u (driver %u) has no crew template in the session; its crew is "
                    "not spawned, the car still races%s",
                    unsigned(garage), unsigned(team), unsigned(number), unsigned(driver), tail);
    else
        swofty::log("[garage] garage %u: team %u car #%u (driver %u) is the %s car of its garage; its crew is not "
                    "spawned, the car still races%s",
                    unsigned(garage), unsigned(team), unsigned(number), unsigned(driver),
                    slot == 2 ? "third" : "extra", tail);
}

void spawn_vector(SwoftyContext *c)
{
    uintptr_t begin = c->rcx, end = c->r13;
    if (!begin || end <= begin || (end - begin) % gc::ENTRY_SIZE)
        return;
    size_t n = (end - begin) / gc::ENTRY_SIZE;
    if (n > MAX_ENTRIES)
        return;
    uint64_t session = 0;
    bool known = gc::session_of(c->rsp, read_memory, nullptr, &session);
    Log log{uint32_t(c->rbp)};
    size_t kept = gc::compact(reinterpret_cast<uint8_t *>(begin), n, session, known, read_memory, nullptr, log_dropped,
                              &log);
    if (kept != n)
        c->r13 = begin + kept * gc::ENTRY_SIZE;
}

}

namespace runtime {

std::vector<CoreMixins> garage_crew_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_GARAGE_CREW_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[garage] crew guard off (SWOFTY_GARAGE_CREW_GUARD=%s)", v);
        return out;
    }
    if (!swofty::api->resolve(SITE)) {
        swofty::log("[garage] crew guard off: needs %s, which this build does not resolve", SITE);
        return out;
    }
    wanted = true;
    out.push_back({MIXIN, {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&spawn_vector, nullptr}}});
    return out;
}

void garage_crew_commit()
{
    if (!offered || settled || !wanted)
        return;
    settled = true;
    swofty::log("[garage] crew guard %s",
                core_mixins_active(MIXIN)
                    ? "armed: a garage's crew is spawned for its first two cars; a third car is left without one"
                    : "did not install (see the [core] lines)");
}

}
