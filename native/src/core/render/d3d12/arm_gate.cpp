#include "core/diagnostics/steps.hpp"
#include "core/hud/overlay/settings.hpp"
#include "core/render/arm_gate_logic.hpp"
#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/runtime/files.hpp"

#include <swofty/mixin.hpp>

#include <winternl.h>

#include <algorithm>
#include <atomic>
#include <cstring>
#include <string>

namespace render::d3d12 {

namespace {

constexpr size_t PATCH_KEPT = 32;

// The arm gate's set: the creation exports it holds the game's calls at while the arm runs on its worker.
struct ArmHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook device, factory, factory1, factory2;
};

ArmHooks hooks;

// A creation export the gate holds. The device export stays hooked until its first call from outside the arm,
// which it times; the factory exports are unhooked as soon as the worker has finished.
struct Export {
    const char *dll, *name;
    bool kept;
    safetyhook::InlineHook *hook;
    uint8_t patch[PATCH_KEPT] = {};
    size_t patch_len = 0;
    std::atomic<bool> first{true};
};

enum { DEVICE, FACTORY, FACTORY1, FACTORY2, EXPORTS };

constinit Export exports[EXPORTS] = {
    {"d3d12.dll", "D3D12CreateDevice", true, &hooks.device},
    {"dxgi.dll", "CreateDXGIFactory", false, &hooks.factory},
    {"dxgi.dll", "CreateDXGIFactory1", false, &hooks.factory1},
    {"dxgi.dll", "CreateDXGIFactory2", false, &hooks.factory2},
};

constinit arm_gate::Gate gate;
constinit void (*block_fn)() = nullptr;
constinit HMODULE core_module = nullptr;
std::wstring windows_dir, d3d12_dir;

// The caller of the creation export this thread is in, for a hook further down the chain (DRED's) that asks who
// called it: through the gate its return address is the gate's.
constinit thread_local void *gate_caller = nullptr;

constexpr int PASSES_TOLD = 8;
constinit std::atomic<int> passes_told{0};

struct CallerScope {
    void *was;
    explicit CallerScope(void *caller) : was(gate_caller) { gate_caller = caller; }
    ~CallerScope() { gate_caller = was; }
};

bool holds_loader_lock()
{
#if defined(__x86_64__) || defined(_M_X64)
    auto *peb = reinterpret_cast<const uint8_t *>(NtCurrentTeb()->ProcessEnvironmentBlock);
    auto *cs = peb ? *reinterpret_cast<RTL_CRITICAL_SECTION *const *>(peb + 0x110) : nullptr;
    return cs && reinterpret_cast<uintptr_t>(cs->OwningThread) == GetCurrentThreadId();
#else
    return false;
#endif
}

std::wstring module_path(const void *addr, HMODULE *mod)
{
    HMODULE m = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(addr), &m) ||
        !m)
        return {};
    *mod = m;
    wchar_t path[MAX_PATH * 2] = {};
    DWORD n = GetModuleFileNameW(m, path, MAX_PATH * 2);
    return n && n < MAX_PATH * 2 ? std::wstring(path, n) : std::wstring();
}

arm_gate::Exempt exempt_of(void *caller)
{
    if (holds_loader_lock())
        return arm_gate::Exempt::loader_lock;
    HMODULE m = nullptr;
    std::wstring path = module_path(caller, &m);
    if (m && m == core_module)
        return arm_gate::Exempt::core;
    if (arm_gate::system_module(path, windows_dir, d3d12_dir))
        return arm_gate::Exempt::system;
    return arm_gate::Exempt::none;
}

void unhook(Export &e)
{
    if (!e.hook->enabled())
        return;
    if (e.patch_len && std::memcmp(e.hook->target(), e.patch, e.patch_len) != 0) {
        swofty::log("[d3d12] %s!%s was patched over after the D3D12 arm hooked it; its hook stays in", e.dll, e.name);
        return;
    }
    if (!e.hook->disable())
        swofty::log("[d3d12] %s!%s: the D3D12 arm's hook did not come out", e.dll, e.name);
}

template <class Call> HRESULT gated(int which, void *caller, Call call)
{
    Export &e = exports[which];
    CallerScope scope(caller);
    double waited = 0;
    arm_gate::Pass p = arm_gate::Pass::open;
    arm_gate::Exempt who = arm_gate::Exempt::none;
    if (gate.running()) {
        uint32_t me = GetCurrentThreadId();
        who = gate.is_worker(me) ? arm_gate::Exempt::none : exempt_of(caller);
        p = gate.wait(me, who, arm_gate::WAIT_CAP, &waited);
    }
    if (who == arm_gate::Exempt::core) {
        if (p != arm_gate::Pass::open && passes_told.fetch_add(1) < PASSES_TOLD)
            swofty::log("[d3d12] %s from the core's own code (thread %lu) while the D3D12 arm runs: %s, %.0f ms", e.name,
                        GetCurrentThreadId(), arm_gate::pass_name(p), waited);
        return call();
    }
    if (p == arm_gate::Pass::timed_out)
        swofty::log("[d3d12] %s from %s (thread %lu) waited %.0f ms for the D3D12 arm on its worker and goes on without it",
                    e.name, module_of(caller).c_str(), GetCurrentThreadId(), waited);
    if (p == arm_gate::Pass::worker)
        return call();
    if (p != arm_gate::Pass::open && p != arm_gate::Pass::waited && p != arm_gate::Pass::timed_out) {
        if (passes_told.fetch_add(1) < PASSES_TOLD)
            swofty::log("[d3d12] %s from %s (thread %lu) while the D3D12 arm runs: %s", e.name, module_of(caller).c_str(),
                        GetCurrentThreadId(), arm_gate::pass_name(p));
        return call();
    }
    if (!e.first.exchange(false))
        return call();
    perf::Stopwatch took;
    HRESULT hr = call();
    double ms = took.total_ms();
    swofty::log("[d3d12] the first %s from outside the D3D12 arm, from %s (thread %lu): %s, %.1f ms waited, %.1f ms in the "
                "call (0x%08lx)",
                e.name, module_of(caller).c_str(), GetCurrentThreadId(), arm_gate::pass_name(p), waited, ms,
                static_cast<unsigned long>(hr));
    swofty::log(perf::STEP_LINE, waited, (std::string("d3d12 first ") + e.name + " outside the arm, waiting for it").c_str());
    swofty::log(perf::STEP_LINE, ms, (std::string("d3d12 first ") + e.name + " outside the arm, the call").c_str());
    if (e.kept)
        unhook(e);
    return hr;
}

using FactoryFn = HRESULT(WINAPI *)(REFIID, void **);
using Factory2Fn = HRESULT(WINAPI *)(UINT, REFIID, void **);

HRESULT WINAPI device_hook(IUnknown *adapter, D3D_FEATURE_LEVEL level, REFIID riid, void **out)
{
    return gated(DEVICE, __builtin_return_address(0),
                 [&] { return hooks.device.original<PFN_D3D12_CREATE_DEVICE>()(adapter, level, riid, out); });
}

HRESULT WINAPI factory_hook(REFIID riid, void **out)
{
    return gated(FACTORY, __builtin_return_address(0), [&] { return hooks.factory.original<FactoryFn>()(riid, out); });
}

HRESULT WINAPI factory1_hook(REFIID riid, void **out)
{
    return gated(FACTORY1, __builtin_return_address(0), [&] { return hooks.factory1.original<FactoryFn>()(riid, out); });
}

HRESULT WINAPI factory2_hook(UINT flags, REFIID riid, void **out)
{
    return gated(FACTORY2, __builtin_return_address(0), [&] { return hooks.factory2.original<Factory2Fn>()(flags, riid, out); });
}

// Every export the process has takes its hook; D3D12CreateDevice and CreateDXGIFactory1, which the arm itself
// calls, must exist. False when one of them is missing or any hook did not install.
bool hook_exports()
{
    void *detours[EXPORTS] = {reinterpret_cast<void *>(&device_hook), reinterpret_cast<void *>(&factory_hook),
                              reinterpret_cast<void *>(&factory1_hook), reinterpret_cast<void *>(&factory2_hook)};
    void *targets[EXPORTS] = {};
    for (int i = 0; i < EXPORTS; i++) {
        Export &e = exports[i];
        HMODULE m = LoadLibraryA(e.dll);
        targets[i] = m ? reinterpret_cast<void *>(GetProcAddress(m, e.name)) : nullptr;
        if (!targets[i]) {
            if (i == DEVICE || i == FACTORY1) {
                swofty::log("[d3d12] %s!%s is unavailable", e.dll, e.name);
                return false;
            }
            continue;
        }
        bool shared = false;
        for (int j = 0; j < i; j++)
            shared = shared || targets[j] == targets[i];
        if (shared) {
            swofty::log("[d3d12] %s!%s shares its code with another creation export; not hooked twice", e.dll, e.name);
            continue;
        }
        if (!install(*e.hook, targets[i], detours[i], e.name))
            return false;
        e.patch_len = std::min(e.hook->original_bytes().size(), PATCH_KEPT);
        std::memcpy(e.patch, e.hook->target(), e.patch_len);
    }
    return true;
}

void name_thread(const wchar_t *name)
{
    using SetFn = HRESULT(WINAPI *)(HANDLE, PCWSTR);
    HMODULE kb = GetModuleHandleW(L"KernelBase.dll");
    auto set = reinterpret_cast<SetFn>(
        reinterpret_cast<void *>(GetProcAddress(kb ? kb : GetModuleHandleW(L"kernel32.dll"), "SetThreadDescription")));
    if (set)
        set(GetCurrentThread(), name);
}

struct Platform {
    HANDLE thread = nullptr;

    bool hook() { return hook_exports(); }

    bool make_thread(uint32_t &id);

    void resume()
    {
        ResumeThread(thread);
        CloseHandle(thread);
        thread = nullptr;
    }

    void unhook()
    {
        for (auto &e : exports)
            d3d12::unhook(e);
    }

    void unhook_factories()
    {
        for (int i = FACTORY; i < EXPORTS; i++)
            d3d12::unhook(exports[i]);
    }
};

constinit Platform platform;

DWORD WINAPI worker(void *)
{
    name_thread(L"swofty d3d12 arm");
    perf::Stopwatch took;
    arm_gate::run_worker(gate, block_fn, platform);
    swofty::log(perf::STEP_LINE, took.total_ms(), "d3d12 arm on its worker");
    return 0;
}

bool Platform::make_thread(uint32_t &id)
{
    DWORD tid = 0;
    thread = CreateThread(nullptr, 0, worker, nullptr, CREATE_SUSPENDED, &tid);
    id = tid;
    return thread != nullptr;
}

}

void *creation_caller(void *ret) { return gate_caller ? gate_caller : ret; }

void wait_arm_before_exit()
{
    uint32_t me = GetCurrentThreadId();
    if (!gate.working() || gate.is_worker(me) || holds_loader_lock())
        return;
    double waited = 0;
    arm_gate::Pass p = gate.wait_retired(me, arm_gate::WAIT_CAP, &waited);
    swofty::log("[d3d12] the game exits while the D3D12 arm runs on its worker; %s after %.0f ms",
                p == arm_gate::Pass::timed_out ? "it exits without it" : "the arm finished", waited);
}

bool arm_on_worker(void (*block)(), bool early)
{
    if (!hooks.gate.offer())
        return false;
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA(arm_gate::SYNC_ENV, v, sizeof v);
    std::string sync = n && n < sizeof v ? v : "";
    std::string rival = sync == "1" ? std::string() : hud::overlay::device_hook_rival();
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&creation_caller), &core_module);
    wchar_t w[MAX_PATH] = {};
    UINT wn = GetSystemWindowsDirectoryW(w, MAX_PATH);
    windows_dir = wn && wn < MAX_PATH ? std::wstring(w, wn) : std::wstring();
    d3d12_dir = runtime::game_root().empty() ? std::wstring() : runtime::game_root() + L"D3D12\\";
    block_fn = block;
    arm_gate::Inputs in;
    in.sync_env = sync;
    in.rival = rival;
    in.early = early;
    in.loader_lock = holds_loader_lock();
    arm_gate::Choice c = arm_gate::begin(in, gate, platform);
    if (!c.worker) {
        swofty::log("[d3d12] the D3D12 arm runs in the stage: %s", c.why.c_str());
        return settle(hooks, false, c.why.c_str());
    }
    swofty::log("[d3d12] the D3D12 arm runs on a worker; the game's D3D12CreateDevice and CreateDXGIFactory calls wait "
                "for it (%s=1 runs it in the stage)",
                arm_gate::SYNC_ENV);
    return settle(hooks, true);
}

}
