#include "dred.hpp"

#include <d3d12.h>

#include <vector>

#include "dred_logic.hpp"
#include "safetyhook.hpp"

namespace runtime::dred {
namespace {

using CreateDeviceFn = HRESULT(WINAPI *)(IUnknown *, D3D_FEATURE_LEVEL, REFIID, void **);
using DebugInterfaceFn = HRESULT(WINAPI *)(REFIID, void **);

constexpr int kDevices = 8;

bool armed;
HMODULE core_module;
std::wstring flag_path;
ID3D12Device *devices[kDevices];
LONG ndevices;
safetyhook::InlineHook create_hook;

bool readable(const void *p, size_t n)
{
    auto a = reinterpret_cast<uintptr_t>(p);
    if (!a)
        return false;
    for (uintptr_t q = a & ~0xfffull; q < a + n; q += 0x1000) {
        MEMORY_BASIC_INFORMATION m;
        if (!VirtualQuery(reinterpret_cast<void *>(q), &m, sizeof m) || m.State != MEM_COMMIT ||
            (m.Protect & (PAGE_GUARD | PAGE_NOACCESS)) || !(m.Protect & 0xEE))
            return false;
    }
    return true;
}

std::string name_a(const char *p)
{
    std::string s;
    for (int i = 0; i < 128; i++) {
        if ((i == 0 || ((reinterpret_cast<uintptr_t>(p) + i) & 0xfff) == 0) && !readable(p + i, 1))
            break;
        if (!p[i])
            break;
        s += (unsigned char)p[i] >= 0x20 && (unsigned char)p[i] < 0x7f ? p[i] : '?';
    }
    return s;
}

std::string name_w(const wchar_t *p)
{
    std::string s;
    for (int i = 0; i < 128; i++) {
        if ((i == 0 || ((reinterpret_cast<uintptr_t>(p + i)) & 0xfff) == 0) && !readable(p + i, 2))
            break;
        if (!p[i])
            break;
        s += p[i] >= 0x20 && p[i] < 0x7f ? char(p[i]) : '?';
    }
    return s;
}

std::string pick(const char *a, const wchar_t *w)
{
    std::string s = a ? name_a(a) : std::string();
    return s.empty() && w ? name_w(w) : s;
}

bool caller_is_core(void *ret)
{
    HMODULE m = nullptr;
    return GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                              static_cast<LPCWSTR>(ret), &m) &&
           m == core_module;
}

HRESULT WINAPI hooked_create_device(IUnknown *adapter, D3D_FEATURE_LEVEL level, REFIID riid, void **out)
{
    void *ret = __builtin_return_address(0);
    HRESULT hr = create_hook.unsafe_call<HRESULT>(adapter, level, riid, out);
    if (SUCCEEDED(hr) && out && readable(out, sizeof *out) && *out && !caller_is_core(ret)) {
        ID3D12Device *dev = nullptr;
        if (SUCCEEDED(static_cast<IUnknown *>(*out)->QueryInterface(__uuidof(ID3D12Device), reinterpret_cast<void **>(&dev)))) {
            LONG i = InterlockedIncrement(&ndevices) - 1;
            if (i < kDevices)
                devices[i] = dev;
            else
                dev->Release();
        }
    }
    return hr;
}

bool prior_session_removed()
{
    DWORD a = flag_path.empty() ? INVALID_FILE_ATTRIBUTES : GetFileAttributesW(flag_path.c_str());
    if (a == INVALID_FILE_ATTRIBUTES)
        return false;
    DeleteFileW(flag_path.c_str());
    return true;
}

void write_flag()
{
    if (flag_path.empty())
        return;
    HANDLE f = CreateFileW(flag_path.c_str(), GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return;
    DWORD w;
    const char text[] = "removed\n";
    WriteFile(f, text, sizeof text - 1, &w, nullptr);
    CloseHandle(f);
}

template <typename Node> std::string breadcrumbs(const Node *head)
{
    std::vector<const Node *> unfinished, finished;
    unsigned total = 0;
    for (const Node *n = head; n && total < kMaxChain && readable(n, sizeof *n); n = n->pNext, total++) {
        uint32_t done = n->pLastBreadcrumbValue && readable(n->pLastBreadcrumbValue, 4) ? *n->pLastBreadcrumbValue : 0;
        (node_incomplete(n->BreadcrumbCount, done) ? unfinished : finished).push_back(n);
    }
    std::string s;
    char b[160];
    std::snprintf(b, sizeof b, "\n  auto-breadcrumbs: %u command list node(s), %zu not finished; showing %s\n", total,
                  unfinished.size(), "unfinished first");
    s += b;
    if (!total)
        return s + "    none recorded\n";
    unsigned shown = 0;
    for (const auto *list : {&unfinished, &finished})
        for (const Node *n : *list) {
            if (shown == kMaxNodes) {
                s += "    (more nodes not shown)\n";
                return s;
            }
            uint32_t count = n->BreadcrumbCount;
            uint32_t done = n->pLastBreadcrumbValue && readable(n->pLastBreadcrumbValue, 4) ? *n->pLastBreadcrumbValue : 0;
            s += node_header(shown++, pick(n->pCommandListDebugNameA, n->pCommandListDebugNameW).c_str(),
                             pick(n->pCommandQueueDebugNameA, n->pCommandQueueDebugNameW).c_str(), count, done);
            Window w = op_window(count, done);
            if (w.from)
                s += "      ... " + std::to_string(w.from) + " earlier op(s) not shown\n";
            if (!n->pCommandHistory || !readable(n->pCommandHistory + w.from, (w.to - w.from) * sizeof(uint32_t))) {
                s += "      command history not readable\n";
                continue;
            }
            for (uint32_t i = w.from; i < w.to; i++)
                s += op_line(i, done, n->pCommandHistory[i]);
            if (w.to < count)
                s += "      ... " + std::to_string(count - w.to) + " later op(s) not shown\n";
        }
    return s;
}

template <typename Node> std::string contexts(const Node *head)
{
    std::string s;
    unsigned seen = 0, shown = 0;
    for (const Node *n = head; n && seen < kMaxChain && readable(n, sizeof *n); n = n->pNext, seen++) {
        if (!n->BreadcrumbContextsCount || !readable(n->pBreadcrumbContexts, n->BreadcrumbContextsCount * sizeof *n->pBreadcrumbContexts))
            continue;
        for (UINT i = 0; i < n->BreadcrumbContextsCount && shown < kMaxOps; i++, shown++) {
            char b[300];
            std::snprintf(b, sizeof b, "    op %u of %s: %s\n", n->pBreadcrumbContexts[i].BreadcrumbIndex,
                          pick(n->pCommandListDebugNameA, n->pCommandListDebugNameW).c_str(),
                          name_w(n->pBreadcrumbContexts[i].pContextString).c_str());
            s += b;
        }
    }
    return shown ? "\n  breadcrumb contexts:\n" + s : s;
}

template <typename Alloc> std::string allocations(const char *title, const Alloc *head, uint64_t object_of(const Alloc *))
{
    std::string s;
    unsigned n = 0, seen = 0;
    for (const Alloc *a = head; a && seen < kMaxChain && readable(a, sizeof *a); a = a->pNext, seen++)
        if (n++ < kMaxAllocations)
            s += allocation_line(pick(a->ObjectNameA, a->ObjectNameW).c_str(), uint32_t(a->AllocationType), object_of(a));
    char b[120];
    std::snprintf(b, sizeof b, "  %s: %u%s\n", title, n, n > kMaxAllocations ? " (first 32 shown)" : "");
    return b + s;
}

uint64_t no_object(const D3D12_DRED_ALLOCATION_NODE *)
{
    return 0;
}

uint64_t object_of(const D3D12_DRED_ALLOCATION_NODE1 *a)
{
    return reinterpret_cast<uint64_t>(a->pObject);
}

std::string removed_device(ID3D12Device *dev, HRESULT reason)
{
    char b[200];
    std::snprintf(b, sizeof b, "  device %p: GetDeviceRemovedReason 0x%08lx (%s)\n", static_cast<void *>(dev), (unsigned long)reason,
                  removal_name(uint32_t(reason)));
    std::string s = b;
    ID3D12DeviceRemovedExtendedData1 *d1 = nullptr;
    if (SUCCEEDED(dev->QueryInterface(__uuidof(ID3D12DeviceRemovedExtendedData1), reinterpret_cast<void **>(&d1)))) {
        D3D12_DRED_AUTO_BREADCRUMBS_OUTPUT1 ab{};
        D3D12_DRED_PAGE_FAULT_OUTPUT1 pf{};
        HRESULT ha = d1->GetAutoBreadcrumbsOutput1(&ab), hp = d1->GetPageFaultAllocationOutput1(&pf);
        if (SUCCEEDED(ha)) {
            s += breadcrumbs(ab.pHeadAutoBreadcrumbNode);
            s += contexts(ab.pHeadAutoBreadcrumbNode);
        } else {
            std::snprintf(b, sizeof b, "\n  auto-breadcrumbs: unavailable (0x%08lx; DRED was not on for this device)\n", (unsigned long)ha);
            s += b;
        }
        if (SUCCEEDED(hp)) {
            s += "\n" + page_fault_line(pf.PageFaultVA);
            s += allocations("existing allocations", pf.pHeadExistingAllocationNode, object_of);
            s += allocations("recently freed allocations", pf.pHeadRecentFreedAllocationNode, object_of);
        } else {
            std::snprintf(b, sizeof b, "\n  page fault output: unavailable (0x%08lx)\n", (unsigned long)hp);
            s += b;
        }
        d1->Release();
        return s;
    }
    ID3D12DeviceRemovedExtendedData *d0 = nullptr;
    if (SUCCEEDED(dev->QueryInterface(__uuidof(ID3D12DeviceRemovedExtendedData), reinterpret_cast<void **>(&d0)))) {
        D3D12_DRED_AUTO_BREADCRUMBS_OUTPUT ab{};
        D3D12_DRED_PAGE_FAULT_OUTPUT pf{};
        if (SUCCEEDED(d0->GetAutoBreadcrumbsOutput(&ab)))
            s += breadcrumbs(ab.pHeadAutoBreadcrumbNode);
        if (SUCCEEDED(d0->GetPageFaultAllocationOutput(&pf))) {
            s += "\n" + page_fault_line(pf.PageFaultVA);
            s += allocations("existing allocations", pf.pHeadExistingAllocationNode, no_object);
            s += allocations("recently freed allocations", pf.pHeadRecentFreedAllocationNode, no_object);
        }
        d0->Release();
        return s;
    }
    return s + "  the device has no ID3D12DeviceRemovedExtendedData (an older Windows, Wine or vkd3d)\n";
}

}

void arm(const std::wstring &swofty_dir, void (*log)(const char *, ...), HMODULE self)
{
    core_module = self;
    flag_path = swofty_dir + L"dred.flag";
    char env[16] = {0};
    GetEnvironmentVariableA("SWOFTY_DRED", env, sizeof env);
    Decision d = decide(env, prior_session_removed());
    if (!d.on) {
        log("[dred] off: %s (SWOFTY_DRED=1 turns it on)", d.why);
        return;
    }
    bool loaded = GetModuleHandleW(L"d3d12.dll") != nullptr;
    HMODULE d3d12 = LoadLibraryW(L"d3d12.dll");
    auto get = d3d12 ? reinterpret_cast<DebugInterfaceFn>(reinterpret_cast<void *>(GetProcAddress(d3d12, "D3D12GetDebugInterface"))) : nullptr;
    if (!get) {
        log("[dred] on (%s), but d3d12.dll has no D3D12GetDebugInterface; carrying on without it", d.why);
        return;
    }
    const char *how = nullptr;
    ID3D12DeviceRemovedExtendedDataSettings1 *s1 = nullptr;
    ID3D12DeviceRemovedExtendedDataSettings *s0 = nullptr;
    HRESULT h1 = get(__uuidof(ID3D12DeviceRemovedExtendedDataSettings1), reinterpret_cast<void **>(&s1));
    if (SUCCEEDED(h1) && s1) {
        s1->SetAutoBreadcrumbsEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
        s1->SetPageFaultEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
        s1->SetBreadcrumbContextEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
        s1->Release();
        how = "breadcrumbs, page faults and breadcrumb contexts";
    } else {
        HRESULT h0 = get(__uuidof(ID3D12DeviceRemovedExtendedDataSettings), reinterpret_cast<void **>(&s0));
        if (SUCCEEDED(h0) && s0) {
            s0->SetAutoBreadcrumbsEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
            s0->SetPageFaultEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
            s0->Release();
            how = "breadcrumbs and page faults";
        } else {
            log("[dred] on (%s), but the DRED settings interface is missing (0x%08lx, 0x%08lx; Wine/vkd3d or an old Windows); "
                "carrying on without it",
                d.why, (unsigned long)h1, (unsigned long)h0);
            return;
        }
    }
    auto create = reinterpret_cast<void *>(GetProcAddress(d3d12, "D3D12CreateDevice"));
    bool hooked = false;
    if (create) {
        auto h = safetyhook::InlineHook::create(create, reinterpret_cast<void *>(&hooked_create_device));
        if (h) {
            create_hook = std::move(*h);
            hooked = true;
        }
    }
    armed = true;
    log("[dred] on (%s): forced on %s%s; D3D12CreateDevice %s, so a crash report names the removed device's GPU operation", d.why, how,
        loaded ? " (d3d12.dll was already loaded; a device created before this point is not covered)" : "",
        hooked ? "hooked" : "NOT hooked");
}

std::string section(bool registers_show_removal)
{
    std::string head = "\n== GPU device removal (D3D12 DRED)\n";
    int removed = 0;
    std::string body;
    for (LONG i = 0; i < ndevices && i < kDevices; i++) {
        ID3D12Device *dev = devices[i];
        HRESULT reason = dev ? dev->GetDeviceRemovedReason() : S_OK;
        if (FAILED(reason)) {
            removed++;
            body += removed_device(dev, reason);
        }
    }
    if (removed) {
        write_flag();
        return head + body;
    }
    if (!registers_show_removal)
        return armed ? head + "  DRED is on and no device the game created reports removal\n" : std::string();
    write_flag();
    if (armed)
        return head + "  a register holds a DXGI device-removed HRESULT, but no device the game created reports removal\n";
    return head + "  a register holds a DXGI device-removed HRESULT. DRED was off this session, so there is no breadcrumb data.\n"
                  "  The next session turns it on by itself; SWOFTY_DRED=1 forces it on, SWOFTY_DRED=0 off.\n";
}

}
