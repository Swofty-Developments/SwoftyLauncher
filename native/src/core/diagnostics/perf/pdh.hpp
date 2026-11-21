#pragma once

#include <windows.h>

#include <pdh.h>

#include <cstdint>

// The GPU Engine counter calls of the profiler's 3D engine use (system.cpp). pinned() loads a system DLL and pins it,
// so it stays mapped when another module in the game frees its own copy: AMD's atiadlxx.dll loads pdh.dll while the
// game makes its first D3D12 device and frees it again. Each call runs in an SEH frame: a fault ends the call, Fault
// keeps the call's name, the exception code and address, and the call returns the exception code. pdh.cpp is built
// with -fms-extensions.

namespace perf::pdh {

using OpenFn = PDH_STATUS(WINAPI *)(LPCWSTR, DWORD_PTR, PDH_HQUERY *);
using AddFn = PDH_STATUS(WINAPI *)(PDH_HQUERY, LPCWSTR, DWORD_PTR, PDH_HCOUNTER *);
using CollectFn = PDH_STATUS(WINAPI *)(PDH_HQUERY);
using ArrayFn = PDH_STATUS(WINAPI *)(PDH_HCOUNTER, DWORD, LPDWORD, LPDWORD, PPDH_FMT_COUNTERVALUE_ITEM_W);

struct Api {
    OpenFn open = nullptr;
    AddFn add = nullptr;
    CollectFn collect = nullptr;
    ArrayFn array = nullptr;
};

struct Fault {
    const char *call = nullptr;
    unsigned long code = 0;
    uintptr_t at = 0;
};

HMODULE pinned(const wchar_t *dll);
bool load(Api &api);

PDH_STATUS open(const Api &api, PDH_HQUERY *query, Fault &fault);
PDH_STATUS add(const Api &api, PDH_HQUERY query, const wchar_t *path, PDH_HCOUNTER *counter, Fault &fault);
PDH_STATUS collect(const Api &api, PDH_HQUERY query, Fault &fault);
PDH_STATUS array(const Api &api, PDH_HCOUNTER counter, DWORD *size, DWORD *count, PPDH_FMT_COUNTERVALUE_ITEM_W items,
                 Fault &fault);

}
