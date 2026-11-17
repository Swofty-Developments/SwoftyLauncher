#pragma once
#include "core/render/com.hpp"

#include <windows.h>

#include <d3d12.h>

#include <safetyhook.hpp>

namespace d3d12hooks {

using com::release;

// The hook is made disabled and moved into its slot before it goes live, so a detour never runs while the
// slot it calls the original through is still empty.
bool install(safetyhook::InlineHook &h, void *target, void *detour, const char *what);

// A device to read vtables from: the one a ProbeScope holds, or a new one.
ID3D12Device *probe_device();

}
