// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
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

// The game's own resources, read from its archives: a resource's decompressed bytes by its eaid:// URI, a texture or
// texture-array layer decoded to RGBA8, and the texture array and layer of a UI image catalogue entry. The table comes
// from the core export SwoftyCoreGameAssets, looked up in swofty_core.dll on the first call; on a core without it every
// call returns unavailable. Every call blocks on file reads: call it from a thread of your own, never from a paint
// callback, a frame tick or a hook. The first call for a package opens the index of each of its archives (the
// ui_package folder holds thousands), so cache what you build from the results.

namespace swofty::game_assets {

enum class Result { ok, late, unavailable, invalid };

using Texture = SwoftyGameTexture;

constexpr uint32_t all_fragments = SWOFTY_GAME_ALL_FRAGMENTS;

struct Image {
    uint32_t width = 0, height = 0;
    std::vector<uint8_t> rgba;
};

struct Slot {
    std::string texture;
    uint32_t layer = 0;
};

namespace detail {

inline std::atomic<const SwoftyGameAssets *> &override_table()
{
    static std::atomic<const SwoftyGameAssets *> t{nullptr};
    return t;
}

// Host tests answer from a table of their own.
inline void set_table(const SwoftyGameAssets *t) { override_table() = t; }

inline const SwoftyGameAssets *table()
{
    if (auto t = override_table().load())
        return t->version >= 1 ? t : nullptr;
#if defined(_WIN32)
    static std::atomic<uintptr_t> found{0};
    uintptr_t v = found.load(std::memory_order_acquire);
    if (!v) {
        HMODULE core = GetModuleHandleW(L"swofty_core.dll");
        auto get = core ? reinterpret_cast<SwoftyCoreGameAssetsFn>(
                              reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreGameAssets")))
                        : nullptr;
        const SwoftyGameAssets *t = get ? get() : nullptr;
        v = t && t->version >= 1 ? reinterpret_cast<uintptr_t>(t) : 1;
        found.store(v, std::memory_order_release);
    }
    return v == 1 ? nullptr : reinterpret_cast<const SwoftyGameAssets *>(v);
#else
    return nullptr;
#endif
}

inline Result result(int r)
{
    return r == SWOFTY_ASSET_OK ? Result::ok
           : r == SWOFTY_ASSET_LATE ? Result::late
           : r == SWOFTY_ASSET_UNAVAILABLE ? Result::unavailable
                                           : Result::invalid;
}

}

// The core has the table and knows the game folder.
inline bool available()
{
    auto t = detail::table();
    return t && t->available();
}

// A resource's decompressed bytes: one fragment, or every fragment one after another with all_fragments (the
// default). eaid://ui_package/art/fonts/formula1-tv-600_bold_web.font, eaid://ui_package/databases/overrides.uidb.
inline Result read(std::string_view uri, std::string &out, uint32_t fragment = all_fragments)
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    std::string u(uri);
    size_t len = 0;
    int r = t->read(u.c_str(), fragment, nullptr, 0, &len);
    if (r != SWOFTY_TOO_SMALL)
        return r == SWOFTY_ASSET_OK ? (out.clear(), Result::ok) : detail::result(r);
    std::string tmp(len, '\0');
    r = t->read(u.c_str(), fragment, tmp.data(), tmp.size(), &len);
    if (r != SWOFTY_ASSET_OK)
        return detail::result(r);
    tmp.resize(len);
    out = std::move(tmp);
    return Result::ok;
}

// The size of a resource (or one fragment) without copying it.
inline Result size(std::string_view uri, size_t *out, uint32_t fragment = all_fragments)
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    size_t len = 0;
    int r = t->read(std::string(uri).c_str(), fragment, nullptr, 0, &len);
    if (r == SWOFTY_TOO_SMALL || r == SWOFTY_ASSET_OK) {
        *out = len;
        return Result::ok;
    }
    return detail::result(r);
}

// A texture's header: engine format id, top-level size, level and layer counts.
inline Result texture(std::string_view uri, Texture *out)
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    return detail::result(t->texture(std::string(uri).c_str(), out));
}

// One level of one layer of a texture or texture array as straight RGBA8, top row first.
inline Result rgba(std::string_view uri, uint32_t layer, Image &out, uint32_t level = 0)
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    std::string u(uri);
    Texture tex{};
    if (int r = t->texture(u.c_str(), &tex); r != SWOFTY_ASSET_OK)
        return detail::result(r);
    uint32_t w = tex.width >> level ? tex.width >> level : 1, h = tex.height >> level ? tex.height >> level : 1;
    std::vector<uint8_t> px(size_t(w) * h * 4);
    int r = t->texture_rgba(u.c_str(), layer, level, px.data(), px.size(), &w, &h);
    if (r != SWOFTY_ASSET_OK)
        return detail::result(r);
    out.width = w;
    out.height = h;
    out.rgba = std::move(px);
    return Result::ok;
}

// The texture array and layer of a UI image catalogue entry:
// find("overrides.team_logos.driver_career_2024_team_select", "redbull").
inline std::optional<Slot> find(std::string_view catalogue, std::string_view name)
{
    auto t = detail::table();
    if (!t)
        return std::nullopt;
    std::string c(catalogue), n(name);
    char buf[512];
    size_t len = 0;
    uint32_t layer = 0;
    if (t->catalogue_find(c.c_str(), n.c_str(), buf, sizeof buf, &len, &layer) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return Slot{std::string(buf, len), layer};
}

// Every entry's Name in a catalogue, in layer order (an entry without a name gives an empty string).
inline Result names(std::string_view catalogue, std::vector<std::string> &out)
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    std::string c(catalogue);
    std::vector<std::string> list;
    char buf[512];
    for (uint32_t i = 0;; i++) {
        size_t len = 0;
        int r = t->catalogue_name(c.c_str(), i, buf, sizeof buf, &len);
        if (r == SWOFTY_ASSET_LATE)
            break;
        if (r != SWOFTY_ASSET_OK)
            return detail::result(r);
        list.emplace_back(buf, len);
    }
    out = std::move(list);
    return Result::ok;
}

inline const char *name(Result r)
{
    return r == Result::ok ? "ok" : r == Result::late ? "late" : r == Result::unavailable ? "unavailable" : "invalid";
}

}
