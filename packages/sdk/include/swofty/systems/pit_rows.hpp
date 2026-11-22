// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstdint>
#include <optional>

#include <swofty_api.h>

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

// The addon's rows on the race MFD's pit page (standard library 1.12), through the core's pit row operations
// (SwoftyCorePitRows). The manifest declares them (hud.pit_rows); each shows NO / YES after the front wing repair row.

namespace swofty::detail {

inline std::atomic<const SwoftyPitRowOps *> &pit_rows_override()
{
    static std::atomic<const SwoftyPitRowOps *> t{nullptr};
    return t;
}

inline void set_pit_rows_table(const SwoftyPitRowOps *t) { pit_rows_override() = t; }

inline const SwoftyPitRowOps *pit_rows_table()
{
    if (auto t = pit_rows_override().load())
        return t->version >= 1 ? t : nullptr;
#if defined(_WIN32)
    static std::atomic<const SwoftyPitRowOps *> found{nullptr};
    if (auto t = found.load())
        return t;
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    auto get = core ? reinterpret_cast<SwoftyCorePitRowsFn>(GetProcAddress(core, "SwoftyCorePitRows")) : nullptr;
    const SwoftyPitRowOps *t = get ? get() : nullptr;
    if (t && t->version < 1)
        t = nullptr;
    found = t;
    return t;
#else
    return nullptr;
#endif
}

}

namespace swofty::hud {

// One of the calling addon's pit rows, by the id its manifest gives it. find is empty when the profile's build has no
// such row: the addon is off, it is past the second row of the session, or the launcher is older.
class PitRow {
public:
    static std::optional<PitRow> find(const char *id)
    {
        auto t = swofty::detail::pit_rows_table();
        uint32_t h = 0;
        if (!t || !t->find || t->find(id, &h) != SWOFTY_ASSET_OK)
            return std::nullopt;
        return PitRow(h);
    }

    // true for YES.
    bool yes() const
    {
        auto t = swofty::detail::pit_rows_table();
        uint32_t v = 0;
        return t && t->value && t->value(handle_, &v) == SWOFTY_ASSET_OK && v == 1;
    }

    bool set(bool yes) const
    {
        auto t = swofty::detail::pit_rows_table();
        return t && t->set_value && t->set_value(handle_, yes ? 1u : 0u) == SWOFTY_ASSET_OK;
    }

    // Greys the row out and holds it on NO while locked.
    bool lock(bool locked) const
    {
        auto t = swofty::detail::pit_rows_table();
        return t && t->set_locked && t->set_locked(handle_, locked ? 1 : 0) == SWOFTY_ASSET_OK;
    }

    // The car whose pit page shows the row: the player's car, also while the AI drives it down the pit lane. Empty
    // while no pit page is built (menus, loading).
    std::optional<uint32_t> car() const
    {
        auto t = swofty::detail::pit_rows_table();
        uint32_t c = 0;
        if (!t || !t->vehicle || t->vehicle(handle_, &c) != SWOFTY_ASSET_OK)
            return std::nullopt;
        return c;
    }

    uint32_t handle() const { return handle_; }

private:
    explicit PitRow(uint32_t h) : handle_(h) {}
    uint32_t handle_;
};

}
