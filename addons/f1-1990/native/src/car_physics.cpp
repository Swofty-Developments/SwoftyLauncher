// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the physics of every car built in a session (its own DLL).
//
// Car_Load_Vtf builds one vehicle asset per handling VTF resource. The wrap searches the VTF bytes for the 1990
// tyre group marker (TyrePhysicsGroup F1_1990 / F1_1990P) and counts the GearRatioNth keys, logs both with the
// asset, and remembers the assets of the 1990 handling files. Car_Assets_CreateSystemForPerformanceMode builds or
// rebuilds a car's systems from its VTF; a car in equal-performance mode (CarLive +0x12c) takes every section from
// the base asset ([asset+0x48]+0x28) instead, so a Time Trial car runs a stock handling. With option
// own_handling_1990 on, the wrap clears that flag for a car built from a 1990 asset that has a base asset, so its
// own sections load. Each build logs the car, its asset, the flag, the base asset and, when the car has its
// gearbox, the gear ratios; the car-built site logs the finished car's gears. Every line starts
// "[f1-1990] physics:".

#include <atomic>
#include <cstdio>
#include <mutex>
#include <set>
#include <string>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "car_physics.hpp"

SWOFTY_SETTING(bool, physics_log_1990, true);
SWOFTY_SETTING(bool, own_handling_1990, true);

namespace {

using f1990::at;
namespace ph = f1990::physics;

std::mutex g_lock;
std::set<const void *> g_1990;
std::atomic<uint32_t> g_loads, g_builds, g_cleared, g_built;

constexpr uint32_t kMaxLines = 400;

bool is_1990(const void *asset)
{
    std::lock_guard guard(g_lock);
    return g_1990.count(asset) != 0;
}

std::string gear_text(const uint8_t *gearbox)
{
    if (!gearbox || !swofty::mem::readable(gearbox, 0x100))
        return "no gearbox";
    ph::Gears g = ph::gears(gearbox);
    char buf[256];
    int n = std::snprintf(buf, sizeof buf, "%d gears, final drive %.3f, ratios", g.count, double(g.final_drive));
    for (int i = 0; i < ph::kGears && n > 0 && size_t(n) < sizeof buf; i++)
        n += std::snprintf(buf + n, sizeof buf - size_t(n), " %.3f", double(g.ratio[i]));
    return buf;
}

}

SWOFTY_FN_(SWOFTY_MIXIN_WRAP, func, 0, Car_Load_Vtf, void *, void *loader, void *resource, uint32_t version, const uint64_t *stream,
           uint64_t flag)
{
    void *asset = orig(loader, resource, version, stream, flag);
    if (!asset || !stream || !swofty::mem::readable(stream, 0x30))
        return asset;
    auto *bytes = at<const uint8_t *>(stream, 0);
    size_t size = size_t(at<uint64_t>(stream, 8));
    if (!bytes || size == 0 || size > (64u << 20) || !swofty::mem::readable(bytes, size)) {
        bytes = at<const uint8_t *>(stream, 0x20);
        size = size_t(at<uint64_t>(stream, 0x28));
        if (!bytes || size == 0 || size > (64u << 20) || !swofty::mem::readable(bytes, size)) {
            bytes = nullptr;
            size = 0;
        }
    }
    bool vtf_1990 = ph::is_1990_vtf(bytes, size);
    if (vtf_1990) {
        std::lock_guard guard(g_lock);
        g_1990.insert(asset);
    }
    if (physics_log_1990 && g_loads++ < kMaxLines)
        swofty::log("[f1-1990] physics: handling VTF loaded: asset %p, %zu bytes, %s, %d of 9 GearRatioNth keys, version %u",
                    asset, size, !bytes ? "bytes not readable" : vtf_1990 ? "1990 marker found" : "no 1990 marker",
                    ph::gear_names(bytes, size), version);
    return asset;
}

SWOFTY_FN_(SWOFTY_MIXIN_WRAP, func, 0, Car_Assets_CreateSystemForPerformanceMode, uint64_t, void *asset, void *systems)
{
    auto *car = systems && swofty::mem::readable(static_cast<uint8_t *>(systems) + 0x10, 8)
                    ? at<uint8_t *>(systems, 0x10)
                    : nullptr;
    if (!car || !asset || !swofty::mem::readable(car + ph::kEqualPerformance, 4) ||
        !swofty::mem::readable(static_cast<uint8_t *>(asset) + ph::kBaseAsset, 8))
        return orig(asset, systems);
    uint32_t flag = at<uint32_t>(car, ph::kEqualPerformance);
    auto *base = at<uint8_t *>(asset, ph::kBaseAsset);
    auto *base_asset = base && swofty::mem::readable(base + ph::kAssetResource, 8) ? at<uint8_t *>(base, ph::kAssetResource)
                                                                                  : nullptr;
    bool mine = is_1990(asset);
    bool clear = ph::clears_equal_performance(own_handling_1990, mine, flag, base_asset != nullptr);
    if (clear) {
        *reinterpret_cast<uint32_t *>(car + ph::kEqualPerformance) = 0;
        g_cleared++;
    }
    uint64_t r = orig(asset, systems);
    if (physics_log_1990 && g_builds++ < kMaxLines) {
        auto *gearbox = at<uint8_t *>(car, 0x538);
        swofty::log("[f1-1990] physics: car %u build: asset %p (%s), equal-performance flag %u%s, base asset %p (resource %p), %s",
                    unsigned(at<uint8_t>(car, 0xe8)), asset, mine ? "1990" : "not 1990", flag,
                    clear ? " cleared, own handling" : "", base, base_asset, gear_text(gearbox).c_str());
    }
    return r;
}

SWOFTY_SITE(site_Car_LiveCarBuilt, swofty::Context &c)
{
    if (!physics_log_1990 || g_built++ >= kMaxLines)
        return;
    auto *car = reinterpret_cast<const uint8_t *>(c.rdi);
    if (!car || !swofty::mem::readable(car, 0x600))
        return;
    auto *asset = at<const uint8_t *>(car, 0x4a8);
    swofty::log("[f1-1990] physics: car %u built: asset %p (%s), equal-performance flag %u, handling version %u, %s",
                unsigned(at<uint8_t>(car, 0xe8)), asset, is_1990(asset) ? "1990" : "not 1990",
                at<uint32_t>(car, ph::kEqualPerformance), at<uint32_t>(car, 0x9f40),
                gear_text(at<const uint8_t *>(car, 0x538)).c_str());
}

SWOFTY_ADDON()
