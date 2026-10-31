// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>

#include <swofty/db.hpp>
#include <swofty/mem.hpp>

#include "pu_scope.hpp"
#include "weekend.hpp"

// A Grand Prix or Time Trial session of the F1 1990 box: no career is loaded and every entrant of the
// session's line-up (session component +0x278, entrant rows of 0x88 bytes at +0x2e0, vehicle ref +0x5c
// with the stat id in the high word) drives a vehicle of the f1_1990 or f1_1990b package.
namespace f1990::box {

constexpr uint32_t kTimeTrial = 18;

inline bool practice(uint32_t id) { return id >= 1 && id <= 4; }
inline bool qualifying(uint32_t id) { return id >= 5 && id <= 14; }
inline bool race(uint32_t id) { return id >= 15 && id <= 17; }

inline const VehicleSet *vehicles()
{
    static std::mutex lock;
    static VehicleSet set;
    static std::atomic<bool> ready{false};
    if (ready.load())
        return &set;
    std::lock_guard guard(lock);
    if (ready.load())
        return &set;
    if (!swofty::db::available())
        return nullptr;
    int64_t count = 0;
    if (swofty::db::int_at("entrants.vehicles._vehicle_instancecount_", &count) != swofty::db::Result::ok || count <= 0 ||
        count > 4096)
        return nullptr;
    for (int64_t i = 0; i < count; i++) {
        std::string row = "entrants.vehicles.vehicle_" + std::to_string(i);
        swofty::db::Value package{};
        int64_t stat_id = -1;
        if (swofty::db::get(row + ".vehicle_package", &package) == swofty::db::Result::ok && is_1990_package(package.s) &&
            swofty::db::int_at(row + ".stat_id", &stat_id) == swofty::db::Result::ok)
            set.add(stat_id);
    }
    ready = true;
    return &set;
}

struct Lineup {
    bool readable = false;
    size_t entrants = 0, on_1990 = 0;

    bool all_1990() const { return readable && entrants && on_1990 == entrants; }
};

inline Lineup lineup(void *component)
{
    Lineup l;
    const VehicleSet *set = vehicles();
    if (!set || !component || !swofty::mem::readable(static_cast<uint8_t *>(component) + 0x278, 8))
        return l;
    auto *owner = at<uint8_t *>(component, 0x278);
    if (!owner || !swofty::mem::readable(owner + 0x2e0, 0x10))
        return l;
    auto *b = at<const uint8_t *>(owner, 0x2e0), *e = at<const uint8_t *>(owner, 0x2e8);
    if (!b || e <= b || size_t(e - b) % kEntrantSize || size_t(e - b) / kEntrantSize > kEntrantMax ||
        !swofty::mem::readable(b, size_t(e - b)))
        return l;
    l.readable = true;
    for (const uint8_t *r = b; r < e; r += kEntrantSize) {
        l.entrants++;
        l.on_1990 += set->has(at<uint32_t>(r, 0x5c) >> 16);
    }
    return l;
}

inline void *session_of(void *component)
{
    auto *provider = component && swofty::mem::readable(static_cast<uint8_t *>(component) + 0x270, 8)
                         ? at<void *>(component, 0x270)
                         : nullptr;
    auto **vt = provider && swofty::mem::readable(provider, 8) ? *static_cast<void ***>(provider) : nullptr;
    if (!vt || !swofty::mem::readable(vt + 0xa8 / 8, 8) || !vt[0xa8 / 8])
        return nullptr;
    auto *session = reinterpret_cast<void *(*)(void *)>(vt[0xa8 / 8])(provider);
    return session && swofty::mem::readable(session, 0x10) ? session : nullptr;
}

}
