// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstdint>
#include <optional>
#include <vector>
#include <swofty_api.h>
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

// The loaded career's championships: the drivers' and the constructors' standings of its current season in the game's
// order, each row with its points, wins, podiums and points per round, and where the season is (round, rounds, how
// many rounds have run). The core reads them about once a second while a Driver Career or My Team career is loaded and
// every call copies from that snapshot, so the calls are cheap and work from any thread, a paint callback included.
// Every call answers empty on a core without the standings, with no career loaded (Grand Prix and the other modes
// included) and on a build the read is not proven on.

namespace swofty::standings {

using Row = SwoftyStandingsRow;
using Season = SwoftyStandingsSeason;

constexpr uint32_t version = SWOFTY_STANDINGS_VERSION;
constexpr uint32_t max_rounds = SWOFTY_STANDINGS_MAX_ROUNDS;
constexpr uint32_t none = SWOFTY_STANDINGS_NONE;

namespace detail {

inline std::atomic<const SwoftyStandings *> &override_table()
{
    static std::atomic<const SwoftyStandings *> t{nullptr};
    return t;
}

inline void set_table(const SwoftyStandings *t) { override_table() = t; }

inline const SwoftyStandings *table()
{
    if (auto t = override_table().load())
        return t->version >= 1 ? t : nullptr;
#if defined(_WIN32)
    static std::atomic<const SwoftyStandings *> found{nullptr};
    if (auto t = found.load())
        return t;
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    auto get = core ? reinterpret_cast<SwoftyCoreStandingsFn>(GetProcAddress(core, "SwoftyCoreStandings")) : nullptr;
    const SwoftyStandings *t = get ? get() : nullptr;
    if (t && t->version < 1)
        t = nullptr;
    found = t;
    return t;
#else
    return nullptr;
#endif
}

inline std::vector<Row> rows(int (*fn)(Row *, uint32_t, uint32_t *))
{
    uint32_t count = 0;
    if (!fn || fn(nullptr, 0, &count) != SWOFTY_ASSET_OK || !count)
        return {};
    std::vector<Row> out(count + 4);
    if (fn(out.data(), static_cast<uint32_t>(out.size()), &count) != SWOFTY_ASSET_OK)
        return {};
    out.resize(count < out.size() ? count : out.size());
    return out;
}

}

// The core serves the standings on this build.
inline bool available()
{
    auto t = detail::table();
    return t && t->available && t->available();
}

// Where the season is, or nothing while no career's standings are read.
inline std::optional<Season> season()
{
    auto t = detail::table();
    Season s{};
    if (!t || !t->season || t->season(&s) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return s;
}

// The drivers' championship, leader first. Empty before the season's first scored session.
inline std::vector<Row> drivers()
{
    auto t = detail::table();
    return t ? detail::rows(t->drivers) : std::vector<Row>{};
}

// The constructors' championship, leader first: the game's season-start order on 0 points until a session is scored.
inline std::vector<Row> constructors()
{
    auto t = detail::table();
    return t ? detail::rows(t->constructors) : std::vector<Row>{};
}

inline bool is_player(const Row &r) { return (r.flags & SWOFTY_STANDINGS_PLAYER) != 0; }
inline bool has_team(const Row &r) { return r.team != none; }

// The last round's main race has results: the champions are decided.
inline bool complete(const Season &s) { return (s.flags & SWOFTY_STANDINGS_COMPLETE) != 0; }

// The row's points in round `round` (1-based); 0 outside the rounds the row holds.
inline int32_t points_in(const Row &r, int round)
{
    return round >= 1 && round <= static_cast<int>(max_rounds) ? r.round_points[round - 1] : 0;
}

// The row's total after round `round` (1-based), for a points-over-the-season chart.
inline int32_t points_after(const Row &r, int round)
{
    int32_t sum = 0;
    for (int i = 1; i <= round && i <= static_cast<int>(max_rounds); i++)
        sum += r.round_points[i - 1];
    return sum;
}

inline const Row *find_driver(const std::vector<Row> &rows, uint32_t driver)
{
    for (const Row &r : rows)
        if (r.driver == driver)
            return &r;
    return nullptr;
}

inline const Row *find_team(const std::vector<Row> &rows, uint32_t team)
{
    for (const Row &r : rows)
        if (r.team == team)
            return &r;
    return nullptr;
}

}
