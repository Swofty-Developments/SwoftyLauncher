#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>

// Stands in for the offline hook's module hiding, loaded from swofty\mods\core before swofty_core. It
// plants int3 on GetModuleFileNameW and GetModuleFileNameA and serves the breakpoints from its own
// vectored handler: the original byte runs under the trap flag, then the int3 goes back. Calls pass
// through unchanged. The crash core registers its handler after this one, in front of it, so the
// report writer's GetModuleFileNameW call reaches the core's handler first. With T_INT3_SKIP_WRITER=1
// the handler leaves the crash report writer's breakpoints alone, so one reaches the writer unowned.

namespace {

struct Point {
    uint8_t *addr;
    uint8_t original;
};

Point points[2];
int npoints;
DWORD tls = TLS_OUT_OF_INDEXES;
bool skip_writer;
HRESULT(WINAPI *describe)(HANDLE, PWSTR *);

bool on_writer()
{
    PWSTR d = nullptr;
    bool yes = describe && SUCCEEDED(describe(GetCurrentThread(), &d)) && d && !wcscmp(d, L"swofty crash report writer");
    if (d)
        LocalFree(d);
    return yes;
}

void say(const char *s)
{
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD w;
    if (out && out != INVALID_HANDLE_VALUE)
        WriteFile(out, s, DWORD(strlen(s)), &w, nullptr);
}

LONG CALLBACK on_point(EXCEPTION_POINTERS *e)
{
    DWORD code = e->ExceptionRecord->ExceptionCode;
    if (code == EXCEPTION_BREAKPOINT) {
        if (skip_writer && on_writer())
            return EXCEPTION_CONTINUE_SEARCH;
        for (int i = 0; i < npoints; i++)
            if (points[i].addr == e->ExceptionRecord->ExceptionAddress) {
                *points[i].addr = points[i].original;
                e->ContextRecord->Rip = reinterpret_cast<DWORD64>(points[i].addr);
                e->ContextRecord->EFlags |= 0x100;
                TlsSetValue(tls, reinterpret_cast<void *>(uintptr_t(i + 1)));
                return EXCEPTION_CONTINUE_EXECUTION;
            }
        return EXCEPTION_CONTINUE_SEARCH;
    }
    if (code == EXCEPTION_SINGLE_STEP && tls != TLS_OUT_OF_INDEXES) {
        if (auto i = reinterpret_cast<uintptr_t>(TlsGetValue(tls))) {
            TlsSetValue(tls, nullptr);
            *points[i - 1].addr = 0xCC;
            e->ContextRecord->EFlags &= ~0x100u;
            return EXCEPTION_CONTINUE_EXECUTION;
        }
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

void plant(const char *name)
{
    auto p = reinterpret_cast<uint8_t *>(GetProcAddress(GetModuleHandleW(L"kernel32.dll"), name));
    DWORD old;
    if (!p || *p == 0xCC || !VirtualProtect(p, 1, PAGE_EXECUTE_READWRITE, &old))
        return;
    points[npoints++] = {p, *p};
    *p = 0xCC;
    FlushInstructionCache(GetCurrentProcess(), p, 1);
}

}

BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID)
{
    if (reason != DLL_PROCESS_ATTACH)
        return TRUE;
    char v[4] = "";
    skip_writer = GetEnvironmentVariableA("T_INT3_SKIP_WRITER", v, sizeof v) && v[0] == '1';
    describe = reinterpret_cast<HRESULT(WINAPI *)(HANDLE, PWSTR *)>(
        reinterpret_cast<void *>(GetProcAddress(GetModuleHandleW(L"kernelbase.dll"), "GetThreadDescription")));
    tls = TlsAlloc();
    if (tls == TLS_OUT_OF_INDEXES || !AddVectoredExceptionHandler(1, on_point))
        return TRUE;
    plant("GetModuleFileNameW");
    plant("GetModuleFileNameA");
    char line[128];
    snprintf(line, sizeof line, "[int3] breakpoints planted on %d GetModuleFileName function(s)%s\n", npoints,
             skip_writer ? ", the writer's left unowned" : "");
    say(line);
    return TRUE;
}
