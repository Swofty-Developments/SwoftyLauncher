#include "core/diagnostics/perf/sysmem.hpp"

#include <swofty/mixin.hpp>

#include <windows.h>

#include <dxgi1_4.h>
#include <psapi.h>

#include <string>

namespace perf::sysmem {

namespace {

bool adapters_refused = false, probed = false;

bool under_wine()
{
    HMODULE nt = GetModuleHandleW(L"ntdll.dll");
    return nt && GetProcAddress(nt, "wine_get_version") != nullptr;
}

std::string narrow(const wchar_t *w)
{
    std::string out;
    for (; *w && out.size() < 128; w++)
        out += *w < 0x80 ? char(*w) : '?';
    return out;
}

// Opens a DXGI factory, hands each hardware adapter's IDXGIAdapter3 to `each`, and releases everything before it
// returns, so no DXGI object outlives the call. With `boot` set it also logs every adapter's description.
template <typename F> void walk_adapters(bool boot, F each)
{
    if (adapters_refused || under_wine())
        return;
    HMODULE dxgi = LoadLibraryW(L"dxgi.dll");
    using CreateFn = HRESULT(WINAPI *)(REFIID, void **);
    auto create =
        dxgi ? reinterpret_cast<CreateFn>(reinterpret_cast<void *>(GetProcAddress(dxgi, "CreateDXGIFactory1"))) : nullptr;
    IDXGIFactory1 *factory = nullptr;
    if (!create || FAILED(create(__uuidof(IDXGIFactory1), reinterpret_cast<void **>(&factory))) || !factory) {
        adapters_refused = true;
        if (boot)
            swofty::log("[system] gpu: dxgi.dll gave no factory; per-minute VRAM is unavailable");
        return;
    }
    IDXGIAdapter1 *a = nullptr;
    for (UINT i = 0; i < 8 && factory->EnumAdapters1(i, &a) != DXGI_ERROR_NOT_FOUND; i++) {
        DXGI_ADAPTER_DESC1 d{};
        if (!a || FAILED(a->GetDesc1(&d))) {
            if (a)
                a->Release();
            a = nullptr;
            continue;
        }
        bool software = (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) != 0;
        if (boot) {
            LARGE_INTEGER ver{};
            std::string driver;
            if (SUCCEEDED(a->CheckInterfaceSupport(__uuidof(IDXGIDevice), &ver)))
                driver = logic::driver_text(uint64_t(ver.QuadPart));
            swofty::log("%s",
                        logic::gpu_boot_line(i, narrow(d.Description), d.VendorId, d.DeviceId, driver, d.DedicatedVideoMemory,
                                             d.DedicatedSystemMemory, d.SharedSystemMemory, software)
                            .c_str());
        }
        IDXGIAdapter3 *a3 = nullptr;
        if (!software && SUCCEEDED(a->QueryInterface(__uuidof(IDXGIAdapter3), reinterpret_cast<void **>(&a3))) && a3) {
            each(a3);
            a3->Release();
        }
        a->Release();
        a = nullptr;
    }
    factory->Release();
}

}

void probe()
{
    if (probed)
        return;
    probed = true;
    MEMORYSTATUSEX ms{};
    ms.dwLength = sizeof ms;
    if (GlobalMemoryStatusEx(&ms))
        swofty::log("%s", logic::ram_boot_line(ms.ullTotalPhys, ms.ullTotalPageFile).c_str());
    if (under_wine())
        swofty::log("[system] gpu: not queried under Wine");
    walk_adapters(true, [](IDXGIAdapter3 *) {});
}

logic::MemoryMinute sample(double secs)
{
    logic::MemoryMinute m;
    m.secs = secs;
    MEMORYSTATUSEX ms{};
    ms.dwLength = sizeof ms;
    if (GlobalMemoryStatusEx(&ms)) {
        m.system_known = true;
        m.ram_total = ms.ullTotalPhys, m.ram_free = ms.ullAvailPhys;
        m.commit_limit = ms.ullTotalPageFile, m.commit_free = ms.ullAvailPageFile;
    }
    PROCESS_MEMORY_COUNTERS_EX pm{};
    pm.cb = sizeof pm;
    if (GetProcessMemoryInfo(GetCurrentProcess(), reinterpret_cast<PROCESS_MEMORY_COUNTERS *>(&pm), sizeof pm)) {
        m.process_known = true;
        m.working_set = pm.WorkingSetSize, m.private_bytes = pm.PrivateUsage;
    }
    walk_adapters(false, [&](IDXGIAdapter3 *a) {
        logic::GpuMinute g;
        DXGI_QUERY_VIDEO_MEMORY_INFO local{}, non_local{};
        if (SUCCEEDED(a->QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &local)) &&
            SUCCEEDED(a->QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, &non_local))) {
            g.known = true;
            g.local_used = local.CurrentUsage, g.local_budget = local.Budget;
            g.non_local_used = non_local.CurrentUsage, g.non_local_budget = non_local.Budget;
        }
        m.gpus.push_back(g);
    });
    return m;
}

}
