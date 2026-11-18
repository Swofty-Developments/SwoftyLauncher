#pragma once
#include <cstdint>
#include <string_view>

namespace frontend::name_hash {

// The engine's name hash: djb2 with seed 5381 over the bytes as unsigned values. Screen, group and view model names,
// UI registry node names and settings commands are all keyed by it.
constexpr uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    while (*s)
        h = h * 33 + uint8_t(*s++);
    return h;
}

constexpr uint32_t djb2(std::string_view s)
{
    uint32_t h = 5381;
    for (unsigned char c : s)
        h = h * 33 + c;
    return h;
}

}
