// Run under Wine: arms the watch on a static array and checks that writes from old and new threads are logged.
#include <windows.h>

#include <cstdarg>
#include <cstdio>
#include <cstdint>

namespace swofty {
namespace mem {
struct Img {
    uintptr_t lo, hi;
};
namespace detail {
inline Img image()
{
    auto base = reinterpret_cast<uintptr_t>(GetModuleHandleA(nullptr));
    auto *nt = reinterpret_cast<IMAGE_NT_HEADERS *>(base + reinterpret_cast<IMAGE_DOS_HEADER *>(base)->e_lfanew);
    return {base, base + nt->OptionalHeader.SizeOfImage};
}
}
inline bool readable(const void *p, size_t n)
{
    MEMORY_BASIC_INFORMATION m;
    if (!VirtualQuery(p, &m, sizeof m) || m.State != MEM_COMMIT || (m.Protect & PAGE_NOACCESS) || (m.Protect & PAGE_GUARD))
        return false;
    return reinterpret_cast<uintptr_t>(p) + n <= reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
}
}
inline void log(const char *fmt, ...)
{
    va_list a;
    va_start(a, fmt);
    std::vprintf(fmt, a);
    std::printf("\n");
    std::fflush(stdout);
    va_end(a);
}
}

#include "heap_watch_win.hpp"

alignas(64) volatile uint64_t g_block[16];

static DWORD WINAPI writer(LPVOID arg)
{
    Sleep(DWORD(reinterpret_cast<uintptr_t>(arg)));
    g_block[2] = 0x1111;
    g_block[1] = 0;
    Sleep(50);
    g_block[3] = 0x3333;
    g_block[4] = 0x4444;
    return 0;
}

int main()
{
    char env[128];
    std::snprintf(env, sizeof env, "F1_1990_HEAP_WATCH=%llx,%llx,%llx", (unsigned long long)&g_block[1],
                  (unsigned long long)&g_block[2], (unsigned long long)&g_block[3]);
    _putenv(env);
    HANDLE early = CreateThread(nullptr, 0, writer, reinterpret_cast<LPVOID>(1500), 0, nullptr);
    Sleep(100);
    f1990::watch::start_watch();
    Sleep(1000);
    HANDLE late = CreateThread(nullptr, 0, writer, reinterpret_cast<LPVOID>(1000), 0, nullptr);
    WaitForSingleObject(early, 5000);
    WaitForSingleObject(late, 5000);
    Sleep(500);
    std::printf("hits %u armed %d failed %d heap %u other %u\n", f1990::watch::g_watch.hits.load(), f1990::watch::g_watch.armed,
                f1990::watch::g_watch.failed, f1990::watch::g_watch.heap_total, f1990::watch::g_watch.other_total);
}
