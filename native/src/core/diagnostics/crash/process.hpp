#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <string>

namespace crash {

using ThreadInfoFn = LONG(NTAPI *)(HANDLE, int, void *, ULONG, ULONG *);
using DescriptionFn = HRESULT(WINAPI *)(HANDLE, PWSTR *);

// The game process as the crash code sees it from attach on: the exe image, the core module, the clock the
// reports measure from, the swofty folder, and the system functions it looks up once.
struct ProcessInfo {
    HMODULE exe, self;
    uintptr_t exe_base;
    size_t exe_size;
    uint32_t exe_stamp;
    bool kb_build, ea_build;
    DWORD start_tick;
    ULONGLONG start_ms;
    volatile LONG window_reached;
    std::wstring swofty_dir;
    ThreadInfoFn query_thread;
    DescriptionFn thread_description;
    const char *(*wine_version)();
};

extern ProcessInfo process;

}
