#pragma once

// Candidate pattern for the EA App executable documented in
// re/research/ea-fuel-binding-candidate.md. This is NOT part of the default
// launcher build. It must be independently verified and promoted through the
// KB before shipping. The input to this helper is an already resolved,
// unique match, never an unchecked hard-coded EA global address.
#include <cstddef>
#include <cstdint>
#include <cstring>

namespace swofty_core::hooks::ea_fuel_binding {
inline constexpr uint32_t timestamp = 1789128892u;
inline constexpr uint32_t image_size = 0x155dc000u;
inline constexpr const char *pattern =
    "48 8B 3D ?? ?? ?? ?? 49 8B 9C 24 A8 05 00 00 48 85 DB 0F 84";

// The seven-byte instruction is mov rdi, [rip+disp32].
inline uintptr_t rip_target(uintptr_t match, const uint8_t *bytes, size_t length)
{
    if (!bytes || length < 7 || bytes[0] != 0x48 || bytes[1] != 0x8b || bytes[2] != 0x3d)
        return 0;
    int32_t displacement = 0;
    std::memcpy(&displacement, bytes + 3, sizeof displacement);
    return uintptr_t(intptr_t(match) + 7 + displacement);
}
}
