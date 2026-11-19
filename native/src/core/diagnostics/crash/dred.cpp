#include "core/diagnostics/crash/dred.hpp"

#include <d3d12.h>

#include <vector>

#include "core/diagnostics/crash/dred_logic.hpp"
#include "core/diagnostics/crash/dred_state.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/runtime/pages.hpp"
#include "safetyhook.hpp"

namespace runtime::dred {

std::vector<std::wstring> marker_folders;
void (*log_fn)(const char *, ...);

namespace {

BuildStops stops;

using CreateDeviceFn = HRESULT(WINAPI *)(IUnknown *, D3D_FEATURE_LEVEL, REFIID, void **);
using DebugInterfaceFn = HRESULT(WINAPI *)(REFIID, void **);

constexpr int kDevices = 8;

bool armed;
HMODULE core_module;
ID3D12Device *devices[kDevices];
LONG ndevices;
safetyhook::InlineHook create_hook;
bool debug_armed;
CRITICAL_SECTION debug_lock;
DebugBudget debug_budget;
std::vector<std::string> debug_lines;
ID3D12InfoQueue *polled[kDevices];
LONG npolled;
HANDLE poll_thread;

bool caller_is_core(void *ret)
{
    HMODULE m = nullptr;
    return GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                              static_cast<LPCWSTR>(ret), &m) &&
           m == core_module;
}

void record_message(D3D12_MESSAGE_CATEGORY category, D3D12_MESSAGE_SEVERITY severity, D3D12_MESSAGE_ID id, const char *text)
{
    EnterCriticalSection(&debug_lock);
    bool take = debug_budget.take(int(severity));
    int n = debug_budget.taken;
    std::string line = take ? debug_line(n, int(severity), int(category), uint32_t(id), text) : std::string();
    if (take)
        debug_lines.push_back(line);
    LeaveCriticalSection(&debug_lock);
    if (take && log_fn)
        log_fn("%s%s", line.c_str(), n == kMaxDebugMessages ? " (50 messages logged; later ones are dropped)" : "");
}

void WINAPI on_message(D3D12_MESSAGE_CATEGORY category, D3D12_MESSAGE_SEVERITY severity, D3D12_MESSAGE_ID id, LPCSTR text, void *)
{
    record_message(category, severity, id, text);
}

void drain(ID3D12InfoQueue *q)
{
    UINT64 n = q->GetNumStoredMessagesAllowedByRetrievalFilter();
    for (UINT64 i = 0; i < n && debug_budget.taken < kMaxDebugMessages; i++) {
        SIZE_T len = 0;
        if (FAILED(q->GetMessage(i, nullptr, &len)) || !len || len > (1u << 20))
            continue;
        std::vector<char> buf(len);
        auto *m = reinterpret_cast<D3D12_MESSAGE *>(buf.data());
        if (SUCCEEDED(q->GetMessage(i, m, &len)))
            record_message(m->Category, m->Severity, m->ID, m->pDescription);
    }
    q->ClearStoredMessages();
}

void drain_all()
{
    for (LONG i = 0; i < npolled && i < kDevices; i++)
        drain(polled[i]);
}

DWORD WINAPI poll_main(void *)
{
    while (debug_budget.taken < kMaxDebugMessages) {
        Sleep(200);
        drain_all();
    }
    return 0;
}

void attach_info_queue(ID3D12Device *dev)
{
    ID3D12InfoQueue *q = nullptr;
    if (FAILED(dev->QueryInterface(__uuidof(ID3D12InfoQueue), reinterpret_cast<void **>(&q))) || !q) {
        if (log_fn)
            log_fn("[d3d-debug] the device has no ID3D12InfoQueue (the debug layer did not attach to it)");
        return;
    }
    ID3D12InfoQueue1 *q1 = nullptr;
    DWORD cookie = 0;
    if (SUCCEEDED(q->QueryInterface(__uuidof(ID3D12InfoQueue1), reinterpret_cast<void **>(&q1))) && q1 &&
        SUCCEEDED(q1->RegisterMessageCallback(on_message, D3D12_MESSAGE_CALLBACK_FLAG_NONE, nullptr, &cookie))) {
        q1->Release();
        q->Release();
        if (log_fn)
            log_fn("[d3d-debug] message callback registered on device %p; the first %d ERROR/CORRUPTION messages are logged",
                   static_cast<void *>(dev), kMaxDebugMessages);
        return;
    }
    if (q1)
        q1->Release();
    LONG i = InterlockedIncrement(&npolled) - 1;
    if (i >= kDevices) {
        q->Release();
        return;
    }
    polled[i] = q;
    if (!poll_thread)
        poll_thread = CreateThread(nullptr, 0, poll_main, nullptr, 0, nullptr);
    if (log_fn)
        log_fn("[d3d-debug] ID3D12InfoQueue1 is missing (older Windows); polling the message queue of device %p every 200 ms",
               static_cast<void *>(dev));
}

HRESULT WINAPI hooked_create_device(IUnknown *adapter, D3D_FEATURE_LEVEL level, REFIID riid, void **out)
{
    void *ret = render::d3d12::creation_caller(__builtin_return_address(0));
    HRESULT hr = create_hook.unsafe_call<HRESULT>(adapter, level, riid, out);
    if (SUCCEEDED(hr) && out && readable(out, sizeof *out) && *out && !caller_is_core(ret)) {
        ID3D12Device *dev = nullptr;
        if (SUCCEEDED(static_cast<IUnknown *>(*out)->QueryInterface(__uuidof(ID3D12Device), reinterpret_cast<void **>(&dev)))) {
            if (debug_armed)
                attach_info_queue(dev);
            LONG i = InterlockedIncrement(&ndevices) - 1;
            if (i < kDevices)
                devices[i] = dev;
            else
                dev->Release();
        }
    }
    return hr;
}

}

void arm(const std::wstring &swofty_dir, void (*log)(const char *, ...), HMODULE self)
{
    core_module = self;
    log_fn = log;
    wchar_t local[MAX_PATH];
    DWORD ln = GetEnvironmentVariableW(L"LOCALAPPDATA", local, MAX_PATH);
    marker_folders = marker_dirs(swofty_dir, ln && ln < MAX_PATH ? std::wstring(local) : std::wstring());
    char env[16] = {0}, denv[16] = {0};
    GetEnvironmentVariableA("SWOFTY_DRED", env, sizeof env);
    GetEnvironmentVariableA("SWOFTY_D3D_DEBUG", denv, sizeof denv);
    const char *how_prior = "";
    Decision d = decide(env, prior_session_removed(&how_prior));
    bool want_debug = debug_requested(denv);
    if (!d.on && !want_debug) {
        log("[dred] off: %s (SWOFTY_DRED=1 turns it on; it is on by default)", d.why);
        return;
    }
    bool loaded = GetModuleHandleW(L"d3d12.dll") != nullptr;
    HMODULE d3d12 = LoadLibraryW(L"d3d12.dll");
    auto get = d3d12
                   ? reinterpret_cast<DebugInterfaceFn>(reinterpret_cast<void *>(GetProcAddress(d3d12, "D3D12GetDebugInterface")))
                   : nullptr;
    if (!get) {
        log("[dred] d3d12.dll has no D3D12GetDebugInterface; carrying on without DRED or the debug layer");
        return;
    }
    if (want_debug) {
        InitializeCriticalSection(&debug_lock);
        ID3D12Debug *dbg = nullptr;
        HRESULT hd = get(__uuidof(ID3D12Debug), reinterpret_cast<void **>(&dbg));
        if (SUCCEEDED(hd) && dbg) {
            dbg->EnableDebugLayer();
            dbg->Release();
            debug_armed = true;
            log("[d3d-debug] on (SWOFTY_D3D_DEBUG=1): debug layer enabled%s",
                loaded ? " (d3d12.dll was already loaded; a device created before this point is not covered)" : "");
        } else {
            log("%s", debug_missing_note(uint32_t(hd)).c_str());
        }
    }
    const char *how = nullptr;
    if (d.on) {
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
                log("[dred] on (%s), but the DRED settings interface is missing (0x%08lx, 0x%08lx; Wine/vkd3d or an old "
                    "Windows); "
                    "carrying on without it",
                    d.why, (unsigned long)h1, (unsigned long)h0);
            }
        }
    }
    if (!how && !debug_armed)
        return;
    auto create = reinterpret_cast<void *>(GetProcAddress(d3d12, "D3D12CreateDevice"));
    bool hooked = false;
    if (create) {
        auto h = safetyhook::InlineHook::create(create, reinterpret_cast<void *>(&hooked_create_device));
        if (h) {
            create_hook = std::move(*h);
            hooked = true;
        }
    }
    armed = how != nullptr;
    if (how)
        log("[dred] on (%s): forced on %s%s; D3D12CreateDevice %s, so a crash report names the removed device's GPU operation",
            d.why, how, loaded ? " (d3d12.dll was already loaded; a device created before this point is not covered)" : "",
            hooked ? "hooked" : "NOT hooked");
    else
        log("[d3d-debug] D3D12CreateDevice %s", hooked ? "hooked" : "NOT hooked, so no message queue is attached");
}

static std::string debug_section()
{
    if (!debug_armed)
        return std::string();
    drain_all();
    EnterCriticalSection(&debug_lock);
    std::string body;
    for (const std::string &l : debug_lines)
        body += "  " + l + "\n";
    LeaveCriticalSection(&debug_lock);
    std::string head = "\n== D3D12 debug layer (SWOFTY_D3D_DEBUG=1)\n";
    return head + (body.empty() ? "  no ERROR or CORRUPTION message was raised\n" : body);
}

static std::string dred_section(bool registers_show_removal);

std::string section(bool registers_show_removal) { return dred_section(registers_show_removal) + debug_section(); }

std::vector<BuildStop> build_stops() { return stops.all(); }

static std::string dred_section(bool registers_show_removal)
{
    stops.clear();
    std::string head = "\n== GPU device removal (D3D12 DRED)\n";
    int removed = 0;
    std::string body;
    for (LONG i = 0; i < ndevices && i < kDevices; i++) {
        ID3D12Device *dev = devices[i];
        HRESULT reason = dev ? dev->GetDeviceRemovedReason() : S_OK;
        if (FAILED(reason)) {
            removed++;
            body += removed_device(dev, reason, stops);
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
                  "  DRED is on by default; SWOFTY_DRED=0 turned it off, or this GPU or Windows lacks the interface.\n";
}

}
