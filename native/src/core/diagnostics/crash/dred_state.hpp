#pragma once

#include "core/diagnostics/crash/dred.hpp"

#include <d3d12.h>

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/diagnostics/crash/dred_logic.hpp"
#include "core/runtime/pages.hpp"

// What the DRED arming, device hook and report section (dred.cpp), the removal markers (dred_markers.cpp) and the
// removed device's DRED data (dred_nodes.cpp) share.
namespace runtime::dred {

extern std::vector<std::wstring> marker_folders;
extern void (*log_fn)(const char *, ...);
extern std::vector<BuildStop> stops;

inline bool readable(const void *p, size_t n)
{
    auto a = reinterpret_cast<uintptr_t>(p);
    return a && runtime::pages::readable_now(a, n);
}

// Whether a removal marker or the newest crash report says the last session lost its device; how names which.
bool prior_session_removed(const char **how);
// Leaves dred.flag for the next session.
void write_flag();
// The DRED breadcrumbs, contexts and page fault allocations of a removed device, with the build stops it read.
std::string removed_device(ID3D12Device *dev, HRESULT reason);

}
