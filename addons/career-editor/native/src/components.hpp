// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: find career components through the game (protocol/economy.md, "Components
// helper"). Flow thread only.

#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

namespace ce::comp {

enum class Id : int {
    Career,
    CareerSetup,
    CareerTime,
    Schedule,
    Championship,
    CareerBank,
    CostCap,
    CareerSponsors,
    MyTeamRF,
    MyTeamFacilities,
    MyTeamStats,
    MyTeamAttributes,
    OwnerPerks,
    OwnerSetup,
    CareerFacilities,
    ResourcePoints,
    Upgrades,
    EngineSuppliers,
    DriverStatistics,
    DriverCareerPlayerDriverStatistics,
    DriverContractsMyTeam,
    DriverCareerContractsManager,
    CareerDriverMarket,
    CareerDriverContractRenewal,
    TeamHQController,
    WeatherAndTimeOfDay,
    Count
};

uintptr_t entity();
uintptr_t get(Id id);
const char *name(Id id);

constexpr size_t kMaxEntrants = 512;

struct Entrant {
    uintptr_t at = 0;
    uint32_t driver = 0;
    uint32_t team = 0;
    bool human = false;
};

std::vector<Entrant> entrants();
bool player(Entrant &out);
int humans();

bool readable(uintptr_t p, size_t n);
bool in_image(uintptr_t p);
bool object_like(uintptr_t obj);

template <class S> const S *view(uintptr_t p)
{
    return p && readable(p, sizeof(S)) ? reinterpret_cast<const S *>(p) : nullptr;
}

template <class S> const S *view(const void *p)
{
    return view<S>(reinterpret_cast<uintptr_t>(p));
}

template <class S> bool array(const S *b, const S *e, size_t max, size_t &n)
{
    auto lo = reinterpret_cast<uintptr_t>(b);
    auto hi = reinterpret_cast<uintptr_t>(e);
    n = 0;
    if (!lo && !hi)
        return true;
    if (!lo || hi < lo || (hi - lo) % sizeof(S) || (hi - lo) / sizeof(S) > max)
        return false;
    n = (hi - lo) / sizeof(S);
    return n == 0 || readable(lo, hi - lo);
}

template <class T> bool read(uintptr_t a, T &out)
{
    if (!a || !readable(a, sizeof(T)))
        return false;
    std::memcpy(&out, reinterpret_cast<const void *>(a), sizeof(T));
    return true;
}

} // namespace ce::comp
