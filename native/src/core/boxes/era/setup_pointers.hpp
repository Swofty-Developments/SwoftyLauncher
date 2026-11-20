#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "core/boxes/era/drs.hpp"

namespace runtime::era {

inline const char *holder_text(const void *holder)
{
    return holder ? *reinterpret_cast<const char *const *>(static_cast<const uint8_t *>(holder) + HOLDER_TEXT) : nullptr;
}

// Every parsed game setup mode: Sess_Apply_ContentAvailability calls the thunk 0x142a223f0 (-> 0x14d83fa30) with
// g_Game_Scenarios, which passes the registry's +0x40 to Sess_Load_GameSetupModes (0x142a148f0). That loader parses each
// game_setup.game_setup_modes row with available=true and stores it by its stat id (the parsed byte +0x80) in a vector
// {allocator +0x40, begin +0x48, end +0x50, capacity +0x58} of 16-byte entries {allocator, GameSetupMode *}; ids
// without a row hold a null mode. A game mode's own setup mode list (the vfunc +0x1d0 list the era picker walks) holds
// only the modes its game_setup_modes field names, so a weekend helper mode that no game mode lists (it must not: a
// listed mode is offered by the picker) is only found here. Seen in game 2026-11-15: f50_wk (stat id 21) was not in
// the Grand Prix mode's list.
constexpr size_t STORE_SETUP_MODES_BEGIN = 0x48;
constexpr size_t STORE_SETUP_MODES_END = 0x50;
constexpr size_t STORE_SETUP_MODE_ENTRY = 0x10;
constexpr size_t STORE_SETUP_MODE_PTR = 8;
constexpr size_t STORE_SETUP_MODES_MAX = 256; // the stat id is a byte

// Finds the stored setup mode labelled want. readable(p, n) guards every read (the game passes mem::readable). Counts
// the stored modes in *seen and collects their labels (at most 63 characters each, "?" when unreadable) in *labels
// when given, for the log line of a helper that is not there.
template <class Readable>
uint8_t *find_stored_mode(const uint8_t *registry, const std::string &want, Readable readable, size_t *seen = nullptr,
                          std::vector<std::string> *labels = nullptr)
{
    if (seen)
        *seen = 0;
    if (labels)
        labels->clear();
    if (!registry || want.empty() || !readable(registry + STORE_SETUP_MODES_BEGIN, 16))
        return nullptr;
    auto *begin = *reinterpret_cast<const uint8_t *const *>(registry + STORE_SETUP_MODES_BEGIN);
    auto *end = *reinterpret_cast<const uint8_t *const *>(registry + STORE_SETUP_MODES_END);
    if (!begin || end < begin || size_t(end - begin) % STORE_SETUP_MODE_ENTRY ||
        size_t(end - begin) / STORE_SETUP_MODE_ENTRY > STORE_SETUP_MODES_MAX || !readable(begin, size_t(end - begin)))
        return nullptr;
    for (auto *e = begin; e != end; e += STORE_SETUP_MODE_ENTRY) {
        auto *mode = *reinterpret_cast<uint8_t *const *>(e + STORE_SETUP_MODE_PTR);
        if (!mode || !readable(mode + MODE_NAME, 8))
            continue;
        if (seen)
            ++*seen;
        auto *holder = *reinterpret_cast<const uint8_t *const *>(mode + MODE_NAME);
        const char *text = holder && readable(holder + HOLDER_TEXT, 8) ? holder_text(holder) : nullptr;
        if (labels) {
            std::string l;
            for (size_t i = 0; text && i < 63 && readable(text + i, 1) && text[i]; i++)
                l += text[i];
            labels->push_back(l.empty() ? std::string("?") : l);
        }
        if (!text || !readable(text, want.size() + 1))
            continue;
        if (!strncmp(text, want.c_str(), want.size() + 1))
            return mode;
    }
    return nullptr;
}

// Sess_Apply_ContentAvailability calls Sess_Load_GameSetupModes (through the thunk 0x142a223f0), which frees every
// GameSetupMode object before it parses the rows again. The pointers the picker keeps into the old objects (its
// narrowed entrant setups, its DRS group swaps, the Time Trial hold) are dropped at that function's entry and never
// written back; the next press or picker build applies the box to the new objects.
struct SetupPointers {
    size_t narrowed = 0, drs = 0;
    bool held = false;
};

inline std::string availability_drop_line(const SetupPointers &p)
{
    std::string what;
    auto add = [&](const std::string &part) { what += (what.empty() ? "" : ", ") + part; };
    if (p.narrowed)
        add(std::to_string(p.narrowed) + " narrowed entrant setup mode(s)");
    if (p.drs)
        add(std::to_string(p.drs) + " DRS group swap(s)");
    if (p.held)
        add("the Time Trial hold");
    if (what.empty())
        return {};
    return "the game rebuilds its setup modes (Sess_Apply_ContentAvailability): dropped " + what +
           " without writing them back; the next box press or picker build applies the box again";
}

}
