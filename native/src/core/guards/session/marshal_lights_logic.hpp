#pragma once

#include <cstddef>
#include <cstdint>

namespace runtime::marshal_lights {

constexpr int LOG_FIRST = 4;

// MarshalLightsSystem: the light list's begin pointer (end at +0x70).
constexpr uint64_t LIST_BEGIN = 0x68;

// vmovss [rax+0x34],xmm7, the store that faults, from site_Sess_MarshalLights_PrevLightLoad.
constexpr uint64_t STORE_FROM_SITE = 0xb;

using Read = bool(uint64_t address, void *out, size_t n, void *ctx);

// The light walk's slot at its step is one past a light that got a zone, or the slot of a light it erased. The walk
// then loads the light before the slot, so a slot at the list's start has no light before it to load.
inline bool at_list_start(uint64_t slot, uint64_t begin) { return begin != 0 && slot == begin; }

inline bool list_start(uint64_t system, Read *read, void *ctx, uint64_t &begin)
{
    begin = 0;
    return system != 0 && read(system + LIST_BEGIN, &begin, sizeof begin, ctx) && begin != 0;
}

inline bool walk_ends(uint64_t system, uint64_t slot, Read *read, void *ctx)
{
    uint64_t begin = 0;
    return list_start(system, read, ctx, begin) && at_list_start(slot, begin);
}

}
