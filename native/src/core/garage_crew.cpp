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
// SWOFTY_GARAGE_CREW_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (garage crew guard)";
constexpr const char *SITE = "site_Game_GarageCrew_SpawnVector";
constexpr size_t ENTRY_SIZE = 0x10;
constexpr size_t ENTRY_SLOT = 0x8;
constexpr size_t MAX_ENTRIES = 256;
constexpr uint32_t CREW_SLOTS = 2;
constexpr size_t REC_DRIVER = 0x52;
constexpr size_t REC_TEAM = 0x5a;
constexpr size_t REC_CAR_NUMBER = 0x83;
constexpr int LOG_FIRST = 8;

bool offered = false, settled = false, wanted = false;
std::atomic<int> dropped{0};
std::vector<uint32_t> seen;

void log_dropped(uint32_t garage, uintptr_t record, uint32_t slot)
{
    uint16_t driver = 0xffff, team = 0xffff;
    uint8_t number = 0;
    if (record) {
        mem::read(record + REC_DRIVER, driver);
        mem::read(record + REC_TEAM, team);
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
    swofty::log("[garage] garage %u: team %u car #%u (driver %u) is the %s car of its garage; its crew is not spawned, "
                "the car still races%s",
                unsigned(garage), unsigned(team), unsigned(number), unsigned(driver), slot == 2 ? "third" : "extra",
                n == LOG_FIRST ? " (further drops are not logged)" : "");
}

void spawn_vector(SwoftyContext *c)
{
    uintptr_t begin = c->rcx, end = c->r13;
    if (!begin || end <= begin || (end - begin) % ENTRY_SIZE)
        return;
    size_t n = (end - begin) / ENTRY_SIZE;
    if (n > MAX_ENTRIES)
        return;
    uint8_t *e = reinterpret_cast<uint8_t *>(begin);
    size_t kept = 0;
    bool changed = false;
    for (size_t i = 0; i < n; i++) {
        uint8_t *src = e + i * ENTRY_SIZE;
        uint32_t slot = 0;
        std::memcpy(&slot, src + ENTRY_SLOT, sizeof slot);
        if (slot >= CREW_SLOTS || kept >= CREW_SLOTS) {
            uintptr_t record = 0;
            std::memcpy(&record, src, sizeof record);
            log_dropped(uint32_t(c->rbp), record, slot);
            changed = true;
            continue;
        }
        if (kept != i)
            std::memcpy(e + kept * ENTRY_SIZE, src, ENTRY_SIZE);
        kept++;
    }
    if (changed)
        c->r13 = begin + kept * ENTRY_SIZE;
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
