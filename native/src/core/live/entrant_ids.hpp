#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>

#include <swofty/game/entrants.hpp>

#include "core/live/live_layout.hpp"

// The session entrant behind a live car, found by the chain the HUD's team badges read (live_state.hpp, find_badge):
// the GameModeSystem the game manager's system registry holds under its key, its game mode, the game, and
// EntrantsManager's session entrant vector, where the entrant whose byte +0x82 is the car's vehicle index is the
// car's. It gives the team (table kind in the low byte of +0x58, stat id in its high word), the driver (domain in the
// low byte of +0x50, stat id at +0x52) and the vehicle (stat id in the high word of +0x5c), with 0x3fff for none.
// M reads memory: read(addr, dst, n) and get(addr, T &), both const. read_car (live_state.cpp) runs it on the game.
namespace swofty_core::live::entrant_ids {

namespace L = layout;

constexpr uint16_t NONE = 0x3fff;
constexpr unsigned TEAM_TABLE = 0, MY_TEAM = L::badge::my_team_kind;
constexpr size_t MAX_ENTRANTS = 512;

struct Ids {
    uint8_t team_kind = 0, driver_domain = 0;
    uint16_t team = NONE, driver = NONE, vehicle = NONE;
    bool driver_read = false;
};

enum class Read { ok, no_entrant, not_ready, off };

struct SystemSlot {
    uintptr_t value = 0;
    size_t slot = ~size_t(0);
};

template <class M> uintptr_t system_by_key(const M &m, uintptr_t manager_site, uintptr_t key, SystemSlot &c)
{
    uintptr_t cached = c.value;
    c.value = 0;
    uintptr_t mgr = 0, reg = 0, sl[2] = {};
    if (!key || !manager_site || !m.get(manager_site, mgr) || !mgr || !m.get(mgr + L::registry::from_manager, reg) || !reg ||
        !m.read(reg + L::registry::slots, sl, sizeof sl))
        return 0;
    uintptr_t base = sl[0], info = sl[1];
    if (!base || (base & 7) || info <= base || (info - base) % 16 || (info - base) / 16 > L::registry::max_slots)
        return 0;
    size_t n = (info - base) / 16;
    uintptr_t kv[2];
    uint8_t used = 0;
    if (cached && c.slot < n && m.read(base + c.slot * 16, kv, sizeof kv) && kv[0] == key && kv[1] == cached &&
        m.get(info + c.slot, used) && used) {
        c.value = cached;
        return cached;
    }
    c.slot = ~size_t(0);
    uintptr_t chunk[2 * 64];
    for (size_t i = 0; i < n; i += 64) {
        size_t k = n - i < 64 ? n - i : 64;
        if (!m.read(base + i * 16, chunk, k * 16))
            return 0;
        for (size_t j = 0; j < k; j++) {
            uintptr_t value = chunk[2 * j + 1];
            if (chunk[2 * j] == key && value && !(value & 7) && m.get(info + i + j, used) && used) {
                c.value = value;
                c.slot = i + j;
                return value;
            }
        }
    }
    return 0;
}

template <class M> Read entrant(const M &m, uintptr_t game_mode_system, uint32_t vehicle, bool with_driver, Ids &out)
{
    uintptr_t mode = 0, game = 0, ev[2] = {};
    if (!game_mode_system || !m.get(game_mode_system + L::badge::mode, mode) || !mode || !m.get(mode + L::badge::game, game) ||
        !game || !m.read(game + L::badge::entrants, ev, sizeof ev))
        return Read::not_ready;
    if (!ev[0] || (ev[0] & 7) || ev[1] <= ev[0] || (ev[1] - ev[0]) % L::badge::entrant_stride ||
        (ev[1] - ev[0]) / L::badge::entrant_stride > MAX_ENTRANTS)
        return Read::not_ready;
    constexpr size_t STRIDE = L::badge::entrant_stride, CHUNK = 32;
    size_t n = (ev[1] - ev[0]) / STRIDE;
    uint8_t rows[CHUNK * STRIDE];
    for (size_t i = 0; i < n; i++) {
        size_t at = i % CHUNK;
        if (!at && !m.read(ev[0] + i * STRIDE, rows, (n - i < CHUNK ? n - i : CHUNK) * STRIDE))
            return Read::not_ready;
        const uint8_t *row = rows + at * STRIDE;
        if (row[L::badge::entrant_vid] != vehicle)
            continue;
        uint32_t team = 0, driver = 0, car = 0;
        std::memcpy(&team, row + L::badge::entrant_team, sizeof team);
        std::memcpy(&driver, row + L::driver::entrant_ref, sizeof driver);
        std::memcpy(&car, row + offsetof(swofty::game::EntrantRecord, vehicle_ref), sizeof car);
        Ids ids;
        ids.team_kind = uint8_t(team & 0xff);
        ids.team = uint16_t(team >> 16);
        ids.vehicle = uint16_t(car >> 16);
        if (with_driver) {
            ids.driver_read = true;
            ids.driver_domain = uint8_t(driver & 0xff);
            ids.driver = uint16_t(driver >> 16);
        }
        out = ids;
        return Read::ok;
    }
    return Read::no_entrant;
}

// The car at this vehicle index on the running game. off (with why) where this build's entrant chain is not proven
// (the HUD team badges' group, live_rows.hpp G_BADGES) or its bindings don't resolve; the driver is read only where
// the driver names' group (G_DRIVER_NAMES) is proven too.
Read read_car(uint32_t vehicle, Ids &out, std::string *why);

}
