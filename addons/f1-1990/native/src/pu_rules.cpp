// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - no power-unit wear and no power-unit grid penalties for the cars of the f1_1990 and
// f1_1990b vehicle packages; every other package keeps the stock rules. Its own DLL.
//
// The career's EngineManagementComponent keeps one record per driver with seven component slots.
// Game_EngineMgmt_ApplySessionWear adds, after a session, amount x a random value in each fitted
// component's wear range to its wear (it fits pending components first, then copies the wear into
// the car); Game_EngineMgmt_AddGridPenalty is the only place a power-unit grid penalty is filed
// (a new component past the allocation, a parc ferme change). For a driver whose career entrant row
// (matched by the driver key, as the engine management matches it) has a vehicle of a 1990
// package, the wear wrap passes amount 0 and the penalty wrap files nothing. With no wear the
// components never reach the replacement threshold, so the game fits no new ones on its own. The
// 1990 vehicles are the entrants.vehicles rows whose vehicle_package is f1_1990 or f1_1990b, read
// on career.loaded.

#include <atomic>
#include <cstring>
#include <string>

#include <swofty/career.hpp>
#include <swofty/db.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "pu_scope.hpp"

SWOFTY_SETTING(bool, pu_rules_1990, true);

namespace {

f1990::VehicleSet g_vehicles;
std::atomic<bool> g_ready{false};
std::atomic<uint32_t> g_wear_logged, g_penalty_logged;

void read_vehicles()
{
    if (g_ready.load() || !swofty::db::available())
        return;
    int64_t count = 0;
    if (swofty::db::int_at("entrants.vehicles._vehicle_instancecount_", &count) != swofty::db::Result::ok || count <= 0 ||
        count > 4096) {
        swofty::log("[f1-1990] power unit: entrants.vehicles is not readable; stock rules for every car");
        return;
    }
    for (int64_t i = 0; i < count; i++) {
        std::string row = "entrants.vehicles.vehicle_" + std::to_string(i);
        swofty::db::Value package{};
        int64_t stat_id = -1;
        if (swofty::db::get(row + ".vehicle_package", &package) == swofty::db::Result::ok &&
            f1990::is_1990_package(package.s) && swofty::db::int_at(row + ".stat_id", &stat_id) == swofty::db::Result::ok)
            g_vehicles.add(stat_id);
    }
    g_ready = true;
    swofty::log("[f1-1990] power unit: %zu vehicle(s) of the f1_1990 and f1_1990b packages run without power-unit wear "
                "or grid penalties",
                g_vehicles.count);
}

bool car_is_1990(uint32_t driver, uint32_t *vehicle)
{
    *vehicle = f1990::kNoVehicle;
    if (!pu_rules_1990 || !g_ready.load() || !g_vehicles.count)
        return false;
    auto *career = static_cast<swofty::game::CareerEntity *>(swofty::career::entity());
    if (!career || !swofty::mem::readable(career, sizeof(swofty::game::CareerEntity)))
        return false;
    auto *b = reinterpret_cast<const uint8_t *>(career->entrants_begin);
    auto *e = reinterpret_cast<const uint8_t *>(career->entrants_end);
    if (!b || e <= b || !swofty::mem::readable(b, size_t(e - b)))
        return false;
    *vehicle = f1990::driver_vehicle(b, e, driver);
    return g_vehicles.has(*vehicle);
}

void on_career(const SwoftyEvent *, void *)
{
    read_vehicles();
}

}

SWOFTY_WRAP(Game_EngineMgmt_ApplySessionWear, void, void *engine_management, uint32_t driver, float amount, uint8_t flag)
{
    uint32_t vehicle;
    if (amount > 0.f && car_is_1990(driver, &vehicle)) {
        if (g_wear_logged++ < 8)
            swofty::log("[f1-1990] power unit: driver 0x%08x (vehicle %u): session wear %.3f skipped", driver, vehicle,
                        double(amount));
        amount = 0.f;
    }
    orig(engine_management, driver, amount, flag);
}

SWOFTY_WRAP(Game_EngineMgmt_AddGridPenalty, void, void *engine_management, const uint32_t *driver, uint32_t places,
            uint32_t reason)
{
    uint32_t vehicle;
    if (driver && car_is_1990(*driver, &vehicle)) {
        if (g_penalty_logged++ < 16)
            swofty::log("[f1-1990] power unit: driver 0x%08x (vehicle %u): %u-place grid penalty (reason %u) dropped",
                        *driver, vehicle, places, reason);
        return;
    }
    orig(engine_management, driver, places, reason);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] power unit: 1990 rules %s", pu_rules_1990 ? "on" : "off (pu_rules_1990)");
    uint32_t token = 0;
    if (swofty::events::subscribe(SWOFTY_EVENT_CAREER_LOADED, on_career, nullptr, &token) != swofty::events::Result::ok)
        swofty::log("[f1-1990] power unit: no career.loaded event; stock rules for every car");
}

SWOFTY_ADDON()
