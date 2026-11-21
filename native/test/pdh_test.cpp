// Wine and Windows test for the profiler's GPU Engine counter calls (src/core/diagnostics/perf/pdh.cpp), built the way
// native/build.sh builds them: zig c++ -target x86_64-windows-gnu -fms-extensions. Each case runs in its own process
// (pdh_test.exe <case>) because a failing case takes the process down. Cases: pdh.dll stays loaded and callable after
// another module that loaded it first frees it twice; a call into memory that is no longer mapped (pdh.dll gone before
// the call); a call that faults inside; an exception the guard does not catch reaching the frame above; the real
// pdh.dll calls the sampler makes.
#include <windows.h>

#include <cstdio>
#include <cstring>

#include "core/diagnostics/perf/pdh.hpp"

using namespace perf::pdh;

static int failures;

static void say(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    fflush(stdout);
    failures += !ok;
}

static bool under_wine()
{
    HMODULE nt = GetModuleHandleW(L"ntdll.dll");
    return nt && GetProcAddress(nt, "wine_get_version");
}

static int pinned_case()
{
    HMODULE theirs = LoadLibraryW(L"pdh.dll");
    say(theirs != nullptr, "another module loads pdh.dll");
    if (!theirs)
        return 1;
    Api api;
    say(load(api), "load resolves the four calls from the loaded pdh.dll");
    FreeLibrary(theirs);
    FreeLibrary(theirs);
    say(GetModuleHandleW(L"pdh.dll") == theirs, "pdh.dll stays at its base after the other module frees it twice");
    Fault fault;
    PDH_HQUERY q = nullptr;
    PDH_STATUS s = open(api, &q, fault);
    say(!fault.call, "PdhOpenQueryW runs in the pinned pdh.dll");
    if (s == ERROR_SUCCESS)
        collect(api, q, fault);
    say(!fault.call, "PdhCollectQueryData runs in the pinned pdh.dll");
    return failures;
}

static int borrowed_case()
{
    HMODULE theirs = LoadLibraryW(L"pdh.dll");
    HMODULE borrowed = GetModuleHandleW(L"pdh.dll");
    FreeLibrary(theirs);
    bool gone = GetModuleHandleW(L"pdh.dll") == nullptr;
    printf("info: a handle from GetModuleHandleW (%p) is %s after the module that loaded pdh.dll frees it%s\n", (void *)borrowed,
           gone ? "unmapped" : "still mapped", under_wine() ? " (Wine)" : "");
    say(theirs && borrowed == theirs, "GetModuleHandleW returns the other module's pdh.dll");
    return failures;
}

static int unmapped_case()
{
    void *page = VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    say(page != nullptr, "an executable page");
    if (!page)
        return 1;
    memset(page, 0xc3, 4096);
    VirtualFree(page, 0, MEM_RELEASE);
    Api api;
    api.collect = reinterpret_cast<CollectFn>(page);
    Fault fault;
    PDH_STATUS s = collect(api, nullptr, fault);
    say(fault.call && !strcmp(fault.call, "PdhCollectQueryData"), "the fault names PdhCollectQueryData");
    say(fault.code == EXCEPTION_ACCESS_VIOLATION, "the fault is an access violation");
    say(fault.at == reinterpret_cast<uintptr_t>(page), "the fault is at the released page");
    say(s == PDH_STATUS(EXCEPTION_ACCESS_VIOLATION), "the call returns the exception code");
    return failures;
}

__attribute__((noinline)) static void poke(size_t n)
{
    volatile size_t *p = nullptr;
    *p = n;
}

static PDH_STATUS WINAPI faulting_add(PDH_HQUERY, LPCWSTR path, DWORD_PTR, PDH_HCOUNTER *)
{
    poke(wcslen(path));
    return ERROR_SUCCESS;
}

static int faulting_case()
{
    Api api;
    api.add = faulting_add;
    Fault fault;
    PDH_HCOUNTER c = nullptr;
    PDH_STATUS s = add(api, nullptr, L"\\GPU Engine(*)\\Utilization Percentage", &c, fault);
    say(fault.call && !strcmp(fault.call, "PdhAddEnglishCounterW"), "the fault names PdhAddEnglishCounterW");
    say(fault.code == EXCEPTION_ACCESS_VIOLATION, "the fault is an access violation");
    say(s == PDH_STATUS(EXCEPTION_ACCESS_VIOLATION), "the call returns the exception code");
    return failures;
}

constexpr DWORD OTHER = 0xE0000F17;

static PDH_STATUS WINAPI raising_collect(PDH_HQUERY)
{
    RaiseException(OTHER, 0, 0, nullptr);
    return ERROR_SUCCESS;
}

static bool outer(const Api &api, Fault &fault)
{
    __try {
        collect(api, nullptr, fault);
        return false;
    } __except (GetExceptionCode() == OTHER ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {
        return true;
    }
}

static int passthrough_case()
{
    Api api;
    api.collect = raising_collect;
    Fault fault;
    say(outer(api, fault), "an exception the guard does not recover from reaches the frame above");
    say(!fault.call, "the guard keeps no fault for it");
    return failures;
}

static int real_case()
{
    Api api;
    say(load(api), "load resolves the four calls");
    Fault fault;
    PDH_HQUERY q = nullptr;
    PDH_HCOUNTER c = nullptr;
    PDH_STATUS s = open(api, &q, fault);
    if (s == ERROR_SUCCESS)
        s = add(api, q, L"\\GPU Engine(*)\\Utilization Percentage", &c, fault);
    printf("info: open and add: 0x%08lx\n", (unsigned long)s);
    if (s == ERROR_SUCCESS) {
        collect(api, q, fault);
        Sleep(100);
        collect(api, q, fault);
        DWORD size = 0, n = 0;
        s = array(api, c, &size, &n, nullptr, fault);
        printf("info: array size query: 0x%08lx, %lu bytes\n", (unsigned long)s, (unsigned long)size);
    }
    say(!fault.call, "the sampler's calls run without a fault");
    return failures;
}

int main(int argc, char **argv)
{
    const char *c = argc > 1 ? argv[1] : "";
    if (!strcmp(c, "pinned"))
        return pinned_case();
    if (!strcmp(c, "borrowed"))
        return borrowed_case();
    if (!strcmp(c, "unmapped"))
        return unmapped_case();
    if (!strcmp(c, "faulting"))
        return faulting_case();
    if (!strcmp(c, "passthrough"))
        return passthrough_case();
    if (!strcmp(c, "real"))
        return real_case();
    printf("FAIL: unknown case '%s'\n", c);
    return 2;
}
