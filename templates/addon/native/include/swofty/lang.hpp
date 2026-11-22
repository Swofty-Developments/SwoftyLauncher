// SPDX-License-Identifier: MIT
#pragma once
#include <atomic>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <swofty/mixin.hpp>
#if defined(_WIN32)
#include <windows.h>
#endif

// Runtime string overrides: change the text a lng_* key shows while the game runs, for every
// language or one language code (eng, fre, ...). The core copies the text; a change applies the
// next time the game looks the key up. text() and language() read the current language's text and
// code; they need a core that exports SwoftyCoreLangVersion 2, and older cores report unavailable.
// Needs a core with API v8; older cores report unavailable.

namespace swofty::lang {

enum class Result { ok, late, unavailable, invalid };

namespace detail {

inline const SwoftyLang *table()
{
    return api && api->version >= 8 ? api->lang : nullptr;
}

inline std::atomic<uint32_t> &forced_version()
{
    static std::atomic<uint32_t> v{0};
    return v;
}

// The SwoftyLang version the running core fills: 2 from a core that exports SwoftyCoreLangVersion,
// 1 from an older v8 core, 0 without the table. A host test sets it with set_version.
inline uint32_t version()
{
    if (!table())
        return 0;
    if (uint32_t v = forced_version().load())
        return v;
#if defined(_WIN32)
    static std::atomic<uint32_t> exported{0};
    if (!exported.load()) {
        HMODULE core = GetModuleHandleW(L"swofty_core.dll");
        auto fn = core ? reinterpret_cast<SwoftyCoreLangVersionFn>(GetProcAddress(core, "SwoftyCoreLangVersion")) : nullptr;
        exported = fn ? fn() : 1;
    }
    return exported.load();
#else
    return 1;
#endif
}

inline void set_version(uint32_t v) { forced_version() = v; }

inline Result result(int r)
{
    return r == SWOFTY_ASSET_OK ? Result::ok
           : r == SWOFTY_ASSET_LATE ? Result::late
           : r == SWOFTY_ASSET_UNAVAILABLE ? Result::unavailable
                                           : Result::invalid;
}

}

// The SwoftyLang version the running core fills: 2 with text() and language(), 1 from an older v8 core, 0 without
// the table.
inline uint32_t version() { return detail::version(); }

// The override library is usable (core v8, the lang feature on, the game's lookup found).
inline bool available()
{
    auto t = detail::table();
    return t && t->available();
}

// Make key show text in every language ("*") or in one language code.
inline Result set(std::string_view key, std::string_view text, std::string_view language = "*")
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    return detail::result(t->override_set(std::string(language).c_str(), std::string(key).c_str(), std::string(text).c_str()));
}

// Drop key's override for one language code, or for every language ("*").
inline Result clear(std::string_view key, std::string_view language = "*")
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    return detail::result(t->override_clear(std::string(language).c_str(), std::string(key).c_str()));
}

// Drop every override (of every addon).
inline Result clear_all()
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    return detail::result(t->override_clear_all());
}

// Send the current language to the game's language listeners, the notification the game sends
// after it loads a language. Call it on the flow thread, at most once per batch.
inline Result refresh()
{
    auto t = detail::table();
    if (!t)
        return Result::unavailable;
    return detail::result(t->refresh());
}

// The current language's text for key: the override when one applies, else the game's text (UTF-8).
// Empty when no table has the key, before the game's locale module is up, or on a core older than
// SwoftyCoreLangVersion 2.
inline std::optional<std::string> text(std::string_view key)
{
    auto t = detail::table();
    if (!t || detail::version() < 2 || !t->text || key.empty())
        return std::nullopt;
    std::string k(key), out(256, '\0');
    size_t len = 0;
    int r = t->text(k.c_str(), out.data(), out.size(), &len);
    if (r == SWOFTY_TOO_SMALL) {
        out.assign(len + 1, '\0');
        r = t->text(k.c_str(), out.data(), out.size(), &len);
    }
    if (r != SWOFTY_ASSET_OK)
        return std::nullopt;
    out.resize(len);
    return out;
}

// The text of a voice line's subtitle: the key "lng_subtitle_" + stem (swofty/systems/speech.hpp).
inline std::optional<std::string> subtitle(std::string_view stem)
{
    if (stem.empty())
        return std::nullopt;
    return text(std::string("lng_subtitle_").append(stem));
}

// The game's current language code (eng, fre, ...); empty before the locale module is up or on a core
// older than SwoftyCoreLangVersion 2.
inline std::optional<std::string> language()
{
    auto t = detail::table();
    if (!t || detail::version() < 2 || !t->language)
        return std::nullopt;
    char out[16] = {};
    size_t len = 0;
    if (t->language(out, sizeof out, &len) != SWOFTY_ASSET_OK || len >= sizeof out)
        return std::nullopt;
    return std::string(out, len);
}

inline const char *name(Result r)
{
    static const char *names[] = {"ok", "late", "unavailable", "invalid"};
    return names[static_cast<int>(r)];
}

}
