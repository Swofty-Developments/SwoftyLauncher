#pragma once

#include <cstdint>
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

}
