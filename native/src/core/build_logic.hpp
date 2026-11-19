#pragma once

#include <cstdint>

// The game builds the core's layout gates name, by the running exe's PE TimeDateStamp and SizeOfImage. The Steam KB
// build's stamp takes any image larger than 0x6000000: its packed and unpacked images differ in size.
namespace runtime::build {

constexpr bool kb(uint32_t stamp, uint32_t size) { return stamp == 1789118616u && size > 0x6000000; }
constexpr bool ea(uint32_t stamp, uint32_t size) { return stamp == 1789128892u && size == 0x155dc000; }
constexpr bool steam_1_0_145(uint32_t stamp, uint32_t size) { return stamp == 1794312636u && size == 0x18434000; }
constexpr bool ea_1_0_145(uint32_t stamp, uint32_t size) { return stamp == 1794315435u && size == 0x1615e000; }

// runtime::verified_build(): the KB build, its EA App link and Steam 1.0.145.
constexpr bool verified(uint32_t stamp, uint32_t size)
{
    return kb(stamp, size) || ea(stamp, size) || steam_1_0_145(stamp, size);
}

// runtime::ported_build(): the verified builds and EA App 1.0.145.
constexpr bool ported(uint32_t stamp, uint32_t size) { return verified(stamp, size) || ea_1_0_145(stamp, size); }

static_assert(verified(1789118616u, 0x6a00000) && verified(1789128892u, 0x155dc000));
static_assert(verified(1794312636u, 0x18434000) && !verified(1789118616u, 0x5000000));
static_assert(!verified(1794315435u, 0x1615e000) && ported(1794315435u, 0x1615e000));
static_assert(!ported(1794315435u, 0x18434000) && !ported(1794312636u, 0x1615e000) && !ported(1794000000u, 0x1615e000));

}
