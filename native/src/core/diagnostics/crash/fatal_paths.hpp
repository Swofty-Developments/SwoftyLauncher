#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstdint>

#include "core/career/save_heap_logic.hpp"
#include "core/diagnostics/crash/crash_heap.hpp"
#include "safetyhook.hpp"

namespace crash {

struct HeapError {
    bool set, fatal, stats;
    int code;
    DWORD tid;
    ULONGLONG ms;
    uintptr_t heap;
    uint64_t capacity, used, free_bytes, largest;
    bool pool;
    career::save_heap::Pool pool_info;
    char name[64];
    char msg[768];
};

// The game's own fatal paths: Ne_ReportMemoryError (a heap error the game reports), the CRT's fast-fail paths and
// RaiseFailFastException, hooked once the code reads decrypted, and the last heap error they saw.
struct GameFatalPaths {
    safetyhook::InlineHook heap_error_hook, fail_fast_hook;
    safetyhook::MidHook gs_hook, invalid_parameter_hook, abort_hook;
    uintptr_t heap_error_fn, report_memory_error, gs_failure_fn, fast_fail_terminate_fn, abort_fn;
    HeapError heap_error;
};

extern GameFatalPaths game_paths;

bool heap_vtable(uintptr_t vt);
bool ne_heap(uintptr_t obj, neheap::Heap &h);
void WINAPI hooked_raise_fail_fast(PEXCEPTION_RECORD rec, PCONTEXT given, DWORD flags);
void hook_heap_errors();
void hook_fast_fails();

}
