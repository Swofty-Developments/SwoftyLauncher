#include "core/render/d3d12/hooking.hpp"

#include "core/diagnostics/steps.hpp"
#include "core/hud/arm_policy.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/probe_logic.hpp"
#include "core/runtime/loader/hook_install.hpp"

#include <swofty/mixin.hpp>

#include <mutex>

namespace render::d3d12 {

namespace {

std::mutex probe_lock;
constinit ProbeHold<ID3D12Device> probe;

}

bool install(safetyhook::InlineHook &h, void *target, void *detour, const char *what)
{
    if (!target || !detour) {
        swofty::log("[d3d12] hooking %s skipped: no target", what);
        return false;
    }
    if (!runtime::loader::install(h, reinterpret_cast<uintptr_t>(target), what, detour, safetyhook::InlineHook::StartDisabled)) {
        swofty::log("[d3d12] hooking %s at %p failed", what, target);
        return false;
    }
    if (!h.original<void *>()) {
        swofty::log("[d3d12] hooking %s at %p failed: no trampoline", what, target);
        h = {};
        return false;
    }
    if (!h.enable()) {
        swofty::log("[d3d12] hooking %s at %p failed: the hook did not enable", what, target);
        h = {};
        return false;
    }
    return true;
}

ID3D12Device *probe_device()
{
    std::lock_guard<std::mutex> g(probe_lock);
    return probe.get([]() -> ID3D12Device * {
        perf::Stopwatch made;
        HMODULE d3d12 = LoadLibraryA("d3d12.dll");
        auto create = d3d12 ? reinterpret_cast<PFN_D3D12_CREATE_DEVICE>(GetProcAddress(d3d12, "D3D12CreateDevice")) : nullptr;
        ID3D12Device *dev = nullptr;
        if (!create || FAILED(create(nullptr, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&dev))))
            return nullptr;
        swofty::log(perf::STEP_LINE, made.total_ms(), "d3d12 probe device made");
        return dev;
    });
}

std::string module_of(const void *addr, HMODULE *mod)
{
    HMODULE m = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(addr), &m) ||
        !m)
        return {};
    if (mod)
        *mod = m;
    char path[MAX_PATH] = {};
    DWORD n = GetModuleFileNameA(m, path, MAX_PATH);
    return std::string(hud::arm_policy::base_name(std::string_view(path, n)));
}

ProbeScope::ProbeScope()
{
    std::lock_guard<std::mutex> g(probe_lock);
    probe.open();
}

ProbeScope::~ProbeScope()
{
    std::lock_guard<std::mutex> g(probe_lock);
    perf::Stopwatch released;
    if (probe.close())
        swofty::log(perf::STEP_LINE, released.total_ms(), "d3d12 probe device release");
}

}
