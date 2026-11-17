#include "core/render/d3d12/hooking.hpp"

#include "core/hud/hud_guard.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <swofty/mixin.hpp>

#include <mutex>

namespace d3d12hooks {

namespace {

std::mutex probe_lock;
ID3D12Device *probe_held;
int probe_scopes;

}

bool install(safetyhook::InlineHook &h, void *target, void *detour, const char *what)
{
    if (!target || !detour) {
        swofty::log("[d3d12] hooking %s skipped: no target", what);
        return false;
    }
    auto made = safetyhook::InlineHook::create(target, detour, safetyhook::InlineHook::StartDisabled);
    if (!made) {
        swofty::log("[d3d12] hooking %s at %p failed", what, target);
        return false;
    }
    if (!made->original<void *>()) {
        swofty::log("[d3d12] hooking %s at %p failed: no trampoline", what, target);
        return false;
    }
    h = std::move(*made);
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
    if (probe_held) {
        probe_held->AddRef();
        return probe_held;
    }
    HMODULE d3d12 = LoadLibraryA("d3d12.dll");
    auto create = d3d12 ? reinterpret_cast<PFN_D3D12_CREATE_DEVICE>(GetProcAddress(d3d12, "D3D12CreateDevice")) : nullptr;
    ID3D12Device *dev = nullptr;
    if (!create || FAILED(create(nullptr, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&dev))))
        return nullptr;
    if (probe_scopes) {
        dev->AddRef();
        probe_held = dev;
    }
    return dev;
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
    return std::string(hud::guard::base_name(std::string_view(path, n)));
}

ProbeScope::ProbeScope()
{
    std::lock_guard<std::mutex> g(probe_lock);
    ++probe_scopes;
}

ProbeScope::~ProbeScope()
{
    std::lock_guard<std::mutex> g(probe_lock);
    if (--probe_scopes == 0)
        release(probe_held);
}

}
