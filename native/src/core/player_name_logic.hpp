#pragma once

#include <cstdint>
#include <cstring>
#include "presence_id_logic.hpp"
#include <string>

// The account display name shown for the player in Time Trial. The game's presence name cache stores names in 0x81-byte
// slots (128 characters plus NUL); the HUD and the leaderboard show about 16 to 24 characters. A name is trimmed, cut at
// 24 characters on a character boundary, and only used when every character is one the game's HUD font draws (ASCII and the
// Latin-1 letters); otherwise the stock name stays.
namespace runtime::player_name {

constexpr size_t MAX_CHARS = 24, MAX_BYTES = 80;

inline bool decode(const std::string &s, size_t &i, uint32_t &cp)
{
    unsigned char c = (unsigned char)s[i];
    size_t n = c < 0x80 ? 1 : (c >> 5) == 6 ? 2 : (c >> 4) == 14 ? 3 : (c >> 3) == 30 ? 4 : 0;
    if (!n || i + n > s.size())
        return false;
    cp = n == 1 ? c : c & (0xff >> (n + 1));
    for (size_t k = 1; k < n; k++) {
        unsigned char d = (unsigned char)s[i + k];
        if ((d & 0xc0) != 0x80)
            return false;
        cp = (cp << 6) | (d & 0x3f);
    }
    i += n;
    return true;
}

inline bool drawable(uint32_t cp)
{
    return (cp >= 0x20 && cp < 0x7f) || (cp >= 0xc0 && cp <= 0xff && cp != 0xd7 && cp != 0xf7);
}

// True with the name to show in `out`; false when the name is empty or has a character the HUD font lacks.
inline bool sanitize(const std::string &name, std::string &out)
{
    out.clear();
    std::string trimmed;
    bool space = true;
    for (char c : name) {
        bool blank = c == ' ' || c == '\t' || c == '\n' || c == '\r';
        if (blank && space)
            continue;
        space = blank;
        trimmed += blank ? ' ' : c;
    }
    while (!trimmed.empty() && trimmed.back() == ' ')
        trimmed.pop_back();
    size_t i = 0, chars = 0;
    while (i < trimmed.size()) {
        size_t start = i;
        uint32_t cp = 0;
        if (!decode(trimmed, i, cp) || !drawable(cp)) {
            out.clear();
            return false;
        }
        if (chars == MAX_CHARS || out.size() + (i - start) > MAX_BYTES)
            break;
        out.append(trimmed, start, i - start);
        chars++;
    }
    while (!out.empty() && out.back() == ' ')
        out.pop_back();
    return !out.empty();
}


inline uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    while (*s)
        h = h * 33 + uint8_t(*s++);
    return h;
}

// The name to show: SWOFTY_TT_PLAYER_NAME holds the text itself ("0" turns the feature off, empty or unset falls back to the launcher
// account name read from home_menu.json).
inline bool display_source(const char *env, const std::string &account, std::string &out)
{
    out.clear();
    if (env && *env) {
        if (!strcmp(env, "0"))
            return false;
        out = env;
        return true;
    }
    out = account;
    return !out.empty();
}

// Whether the user id struct handed to the name cache lookup (kind at +0x4c, type at +0, the id as value ^ mask at +8/+0x10, flag at +0x39)
// is the local presence's id. Only kind 0 keys of the presence's own type are compared; type 3 and 7 keys use other fields and never match.
inline bool is_local_id(const uint8_t id[0x50], const presence_id::Id &me)
{
    uint32_t kind;
    std::memcpy(&kind, id + 0x4c, 4);
    if (kind != 0 || me.type == 0 || me.type == 3 || me.type == 7 || id[0] != me.type)
        return false;
    uint64_t a, b;
    std::memcpy(&a, id + 8, 8);
    std::memcpy(&b, id + 0x10, 8);
    return (a ^ b) == me.value[0] && me.value[0] != 0 && (id[0x39] != 0) == me.flag39;
}

inline bool root_allowed(uint32_t h)
{
    static const uint32_t roots[] = {djb2("network_drivers"), djb2("players"), djb2("view_model_data")};
    for (uint32_t r : roots)
        if (r == h)
            return true;
    return false;
}

inline const char *leaf_name(uint32_t h)
{
    static const char *names[] = {"display_name", "full_name", "full_name_tv_style", "second_name", "second_name_standard", "gamertag",
                                  "second_line_name", "driver_full_name_tv_style"};
    for (const char *n : names)
        if (djb2(n) == h)
            return n;
    return nullptr;
}

}
