// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstddef>
#include <cstdint>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include "weekend.hpp"

namespace f1990::cars {

constexpr size_t kEntrantSize = 0x88;
constexpr size_t kEntrantMax = 256;

// The tyre component of vehicle slot v of a car list (car list +0x28 slots, entity +0x10), found the
// way Sess_Tyres_ClosestAvailableCompound finds it.
inline uint8_t *tyre_component(void *car_list, uint8_t v)
{
    namespace g = swofty::game;
    if (!car_list || !swofty::mem::readable(static_cast<uint8_t *>(car_list) + 0x28, 0x10))
        return nullptr;
    auto **b = at<uint8_t **>(car_list, 0x28), **e = at<uint8_t **>(car_list, 0x30);
    if (!b || e <= b || size_t(e - b) <= v || !swofty::mem::readable(b + v, sizeof *b))
        return nullptr;
    uint8_t *slot = b[v];
    if (!slot || !swofty::mem::readable(slot + 0x10, 8))
        return nullptr;
    void *obj = at<void *>(slot, 0x10);
    auto type = reinterpret_cast<const uint8_t *>(g::k_Sess_TyreComponentType.address());
    auto base = reinterpret_cast<const uint8_t *>(g::k_Ne_TypeRoot.address());
    auto end = reinterpret_cast<const uint8_t *>(g::k_Ne_TypeChainEnd.address());
    bool component = type == base || type == end;
    for (const uint8_t *n = type + 8; !component && n && swofty::mem::readable(n + 0x10, 8);) {
        n = at<const uint8_t *>(n, 0x10);
        component = n == base + 8;
        if (n == end + 8)
            break;
    }
    if (component)
        obj = obj ? g::Ne_Entity_FindComponent(obj, type) : nullptr;
    return obj && swofty::mem::readable(static_cast<uint8_t *>(obj) + 0x9c68, 8) ? static_cast<uint8_t *>(obj)
                                                                               : nullptr;
}

struct Entrants {
    const uint8_t *b = nullptr, *e = nullptr;

    explicit Entrants(void *component)
    {
        if (!swofty::mem::readable(static_cast<uint8_t *>(component) + 0x278, 0x08))
            return;
        auto *career = at<uint8_t *>(component, 0x278);
        if (!career || !swofty::mem::readable(career + 0x2e0, 0x10))
            return;
        auto *pb = at<const uint8_t *>(career, 0x2e0), *pe = at<const uint8_t *>(career, 0x2e8);
        if (pb && pe > pb && size_t(pe - pb) % kEntrantSize == 0 && size_t(pe - pb) / kEntrantSize <= kEntrantMax &&
            swofty::mem::readable(pb, size_t(pe - pb)))
            b = pb, e = pe;
    }

    uint32_t id(const uint8_t *r) const { return at<uint8_t>(r, 0x50) | uint32_t(at<uint16_t>(r, 0x52)) << 16; }

    const uint8_t *vehicle(uint8_t v) const
    {
        for (const uint8_t *r = b; r < e; r += kEntrantSize)
            if (at<uint8_t>(r, 0x82) == v)
                return r;
        return nullptr;
    }
};

}
