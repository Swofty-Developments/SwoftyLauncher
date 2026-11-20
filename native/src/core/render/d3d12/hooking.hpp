#pragma once
#include "core/render/com.hpp"
#include "core/runtime/loader/hook_set.hpp"

#include <windows.h>

#include <d3d12.h>

#include <cstdint>

#include <safetyhook.hpp>

namespace render::d3d12 {

using com::release;

// The hook is made disabled and moved into its slot before it goes live, so a detour never runs while the
// slot it calls the original through is still empty.
bool install(safetyhook::InlineHook &h, void *target, void *detour, const char *what);

// Why a d3d12 hook set is off when its arm has no reason of its own.
inline constexpr const char *NOT_ARMED = "its probe objects or hooks did not take";

// A d3d12 hook set arms once: the first arm offers its gate, and every later one returns whether the set went live.
// The end of that first arm settles the set, live when it armed and off with `why` when not.
template <class Set> bool settle(Set &set, bool armed, const char *why = NOT_ARMED)
{
    set.gate.settle();
    if (armed)
        set.set_live();
    else
        set.turn_off(why);
    return armed;
}

// A device to read vtables from: the one a ProbeScope holds, or a new one.
ID3D12Device *probe_device();

// The frames the game has presented so far, counted by the present hook.
uint64_t frames_presented();

}
