// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <type_traits>
#include <utility>
#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif
#include <swofty/mixin.hpp>
#include <swofty_api.h>

// Game memory writes (experimental): values written into the game's data bindings (the k_* and g_* tables of
// swofty/game.hpp) and memory from the game's own allocators, through the core export SwoftyCoreGameMem. The core
// checks every write against the binding set (a data binding that resolved, the bytes inside the binding's type,
// outside the exe's code, not held by another addon, optionally the bytes the addon expects there), makes the
// pages writable for the copy only and keeps the game's bytes, which restore() puts back. alloc() and free() call
// the game's allocator object (the default one, or one a game object holds) through its vtable after checking it,
// so a buffer handed to a game vector or string is one the game can grow and free. Without the export (an older
// core) every call answers Result::unavailable, alloc() nullptr.

namespace swofty::game_mem {

enum class Result { ok, late, unavailable, invalid };

inline const char *name(Result r)
{
    switch (r) {
    case Result::ok:
        return "ok";
    case Result::late:
        return "late";
    case Result::unavailable:
        return "unavailable";
    case Result::invalid:
        return "invalid";
    }
    return "?";
}

namespace detail {

inline Result result(int r)
{
    return r == SWOFTY_ASSET_OK            ? Result::ok
           : r == SWOFTY_ASSET_LATE        ? Result::late
           : r == SWOFTY_ASSET_UNAVAILABLE ? Result::unavailable
                                           : Result::invalid;
}

inline std::atomic<const SwoftyGameMem *> &override_table()
{
    static std::atomic<const SwoftyGameMem *> t{nullptr};
    return t;
}

// The core's table, or null on a core without SwoftyCoreGameMem.
inline const SwoftyGameMem *table()
{
    if (auto t = override_table().load())
        return t->version >= 1 ? t : nullptr;
#ifdef _WIN32
    static std::atomic<uintptr_t> found{0};
    uintptr_t v = found.load(std::memory_order_acquire);
    if (!v) {
        HMODULE core = GetModuleHandleW(L"swofty_core.dll");
        auto get =
            core ? reinterpret_cast<SwoftyCoreGameMemFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreGameMem")))
                 : nullptr;
        const SwoftyGameMem *t = get ? get() : nullptr;
        v = t && t->version >= 1 ? reinterpret_cast<uintptr_t>(t) : 1;
        found.store(v, std::memory_order_release);
    }
    return v == 1 ? nullptr : reinterpret_cast<const SwoftyGameMem *>(v);
#else
    return nullptr;
#endif
}

// The type a data binding of swofty/game.hpp points at (swofty::data<"k_...", uint32_t[6]> -> uint32_t[6]).
template <class B> using data_type = std::remove_reference_t<decltype(*std::declval<const B &>().get())>;
template <class B> using element_type = std::remove_all_extents_t<data_type<B>>;
template <class B> inline constexpr size_t element_count = sizeof(data_type<B>) / sizeof(element_type<B>);

}

// For tests: answer every call from t instead of the core (nullptr: the core again).
inline void set_table(const SwoftyGameMem *t) { detail::override_table() = t; }

// The core can write data bindings and allocate (a core with SwoftyCoreGameMem).
inline bool available()
{
    auto t = detail::table();
    return t && t->write_data && t->restore_data && t->alloc && t->free;
}

// n raw bytes at offset of the data binding id, whose type is size bytes. expect: the n bytes that must be there
// now (nullptr: any).
inline Result write_bytes(const char *id, size_t size, size_t offset, const void *bytes, size_t n, const void *expect = nullptr)
{
    auto t = detail::table();
    if (!t || !t->write_data)
        return Result::unavailable;
    return detail::result(t->write_data(id, size, offset, bytes, n, expect));
}

// Element index of a data binding: write(swofty::game::k_Career_RnDDeptColourTable, 4, 0xff40b0e0u) sets
// colour [4]; with expect, only when [4] is that value now. A binding that is not an array has one element, 0.
template <class B, class V> Result write(const B &, size_t index, const V &value)
{
    using E = detail::element_type<B>;
    static_assert(B::binding_kind == ::swofty::kind::data, "game_mem::write takes a data binding");
    static_assert(std::is_same_v<std::remove_cv_t<V>, E>, "the value's type differs from the binding's element type");
    static_assert(std::is_trivially_copyable_v<E>, "a data binding's element is copied bytewise");
    if (index >= detail::element_count<B>)
        return Result::invalid;
    return write_bytes(B::id, sizeof(detail::data_type<B>), index * sizeof(E), &value, sizeof(E));
}

template <class B, class V> Result write(const B &, size_t index, const V &value, const V &expect)
{
    using E = detail::element_type<B>;
    static_assert(B::binding_kind == ::swofty::kind::data, "game_mem::write takes a data binding");
    static_assert(std::is_same_v<std::remove_cv_t<V>, E>, "the value's type differs from the binding's element type");
    static_assert(std::is_trivially_copyable_v<E>, "a data binding's element is copied bytewise");
    if (index >= detail::element_count<B>)
        return Result::invalid;
    return write_bytes(B::id, sizeof(detail::data_type<B>), index * sizeof(E), &value, sizeof(E), &expect);
}

// Puts back the game's bytes this addon wrote into the binding (restore_all: into every binding).
template <class B> Result restore(const B &)
{
    static_assert(B::binding_kind == ::swofty::kind::data, "game_mem::restore takes a data binding");
    auto t = detail::table();
    return t && t->restore_data ? detail::result(t->restore_data(B::id)) : Result::unavailable;
}

inline Result restore_all()
{
    auto t = detail::table();
    return t && t->restore_data ? detail::result(t->restore_data(nullptr)) : Result::unavailable;
}

// size bytes from the game allocator object allocator (nullptr: the game's default allocator), or nullptr.
inline void *alloc(size_t size, size_t align = 16, void *allocator = nullptr)
{
    auto t = detail::table();
    return t && t->alloc ? t->alloc(allocator, size, align) : nullptr;
}

template <class T> T *alloc_array(size_t n, void *allocator = nullptr)
{
    static_assert(std::is_trivially_default_constructible_v<T>, "alloc_array hands out raw game memory");
    if (!n || n > (size_t(1) << 30) / sizeof(T))
        return nullptr;
    return static_cast<T *>(alloc(n * sizeof(T), alignof(T), allocator));
}

// Gives p back to the allocator it came from (nullptr: the game's default allocator).
inline void free(void *p, void *allocator = nullptr)
{
    auto t = detail::table();
    if (p && t && t->free)
        t->free(allocator, p);
}

}
