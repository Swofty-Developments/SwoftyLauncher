#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/srv_clamp_logic.hpp"
#include "core/render/srv_hook_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/d3d12.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_set>

namespace render::d3d12 {

namespace {

// ID3D12Device::CreateShaderResourceView is vtable entry 18 (vt+0x90): IUnknown 0-2, ID3D12Object 3-6, then
// GetNodeCount 7 ... CreateConstantBufferView 17, CreateShaderResourceView 18, CreateUnorderedAccessView 19,
// CreateRenderTargetView 20 (device_hooks.cpp's RTV_SLOT).
constexpr size_t RESOURCE_GETDESC_SLOT = 10, SRV_SLOT = 18;

// The core's one hook on CreateShaderResourceView: the srv clamp (SWOFTY_SRV_CLAMP=1) and the addons' view callbacks
// (swofty/d3d12.hpp) share it, so no addon patches the slot itself.
struct SrvHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook create_srv;
};

SrvHooks hooks;
constinit srv_hook::Callbacks<swofty::d3d12::SrvFn> callbacks;
constinit std::atomic<bool> clamp_on{false}, ready_passed{false};
// Set while this thread runs the callbacks: a view a callback makes itself does not call them again.
thread_local bool in_callbacks = false;

const GUID kNameW = {0x4cca5fd8, 0x921f, 0x42c8, {0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41}};

void create_original(ID3D12Device *dev, ID3D12Resource *r, const D3D12_SHADER_RESOURCE_VIEW_DESC *d, SIZE_T h)
{
    hooks.create_srv.call<void>(dev, r, d, h);
}

std::string srv_name(ID3D12Resource *r)
{
    wchar_t w[128] = {};
    UINT size = sizeof w - sizeof(wchar_t);
    char out[160] = {};
    if (SUCCEEDED(r->GetPrivateData(kNameW, &size, w)) && size >= sizeof(wchar_t)) {
        int n = WideCharToMultiByte(CP_UTF8, 0, w, int(size / sizeof(wchar_t)), out, int(sizeof out) - 1, nullptr, nullptr);
        if (n > 0)
            return std::string(out, strnlen(out, size_t(n)));
    }
    char b[48];
    snprintf(b, sizeof b, "%p", static_cast<void *>(r));
    return b;
}

// One line for each clamped or skipped view, the first time it is made.
class SrvReports {
public:
    void report(ID3D12Resource *r, const srv_clamp::View &v, const srv_clamp::Resource &res, const srv_clamp::Plan &p);

private:
    std::mutex lock;
    std::unordered_set<uint64_t> seen;
};

SrvReports reports;

void SrvReports::report(ID3D12Resource *r, const srv_clamp::View &v, const srv_clamp::Resource &res, const srv_clamp::Plan &p)
{
    using srv_clamp::Action;
    uint64_t key = (reinterpret_cast<uint64_t>(r) * 0x9e3779b97f4a7c15ull) ^ (uint64_t(v.slices) << 40) ^
                   (uint64_t(v.first_slice) << 20) ^ (uint64_t(v.mip_levels) << 8) ^ v.most_detailed_mip;
    {
        std::lock_guard<std::mutex> g(lock);
        if (seen.size() >= 4096 || !seen.insert(key).second)
            return;
    }
    std::string name = srv_name(r);
    if (p.action == Action::Skip)
        swofty::log("[gfx] srv: array view from slice %u on a %u-slice texture %s; skipped", v.first_slice, res.slices,
                    name.c_str());
    else if (p.slices_changed)
        swofty::log("[gfx] srv: array view of %u slices on a %u-slice texture %s; clamped", v.slices, res.slices, name.c_str());
    else
        swofty::log("[gfx] srv: view of mips %u+%u on a %u-mip texture %s; clamped", v.most_detailed_mip, v.mip_levels, res.mips,
                    name.c_str());
}

// The view as the clamp would make it: d itself, c filled from d with the clamp applied, or null when the view starts
// past the texture's last slice and is skipped.
const D3D12_SHADER_RESOURCE_VIEW_DESC *clamped(ID3D12Resource *r, const D3D12_RESOURCE_DESC &rd,
                                               const D3D12_SHADER_RESOURCE_VIEW_DESC *d, D3D12_SHADER_RESOURCE_VIEW_DESC &c)
{
    using namespace srv_clamp;
    View v;
    v.dimension = int(d->ViewDimension);
    switch (d->ViewDimension) {
    case D3D12_SRV_DIMENSION_TEXTURE1D:
        v.most_detailed_mip = d->Texture1D.MostDetailedMip;
        v.mip_levels = d->Texture1D.MipLevels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE1DARRAY:
        v.most_detailed_mip = d->Texture1DArray.MostDetailedMip;
        v.mip_levels = d->Texture1DArray.MipLevels;
        v.first_slice = d->Texture1DArray.FirstArraySlice;
        v.slices = d->Texture1DArray.ArraySize;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2D:
        v.most_detailed_mip = d->Texture2D.MostDetailedMip;
        v.mip_levels = d->Texture2D.MipLevels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2DARRAY:
        v.most_detailed_mip = d->Texture2DArray.MostDetailedMip;
        v.mip_levels = d->Texture2DArray.MipLevels;
        v.first_slice = d->Texture2DArray.FirstArraySlice;
        v.slices = d->Texture2DArray.ArraySize;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY:
        v.first_slice = d->Texture2DMSArray.FirstArraySlice;
        v.slices = d->Texture2DMSArray.ArraySize;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE3D:
        v.most_detailed_mip = d->Texture3D.MostDetailedMip;
        v.mip_levels = d->Texture3D.MipLevels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURECUBE:
        v.most_detailed_mip = d->TextureCube.MostDetailedMip;
        v.mip_levels = d->TextureCube.MipLevels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURECUBEARRAY:
        v.most_detailed_mip = d->TextureCubeArray.MostDetailedMip;
        v.mip_levels = d->TextureCubeArray.MipLevels;
        v.first_slice = d->TextureCubeArray.First2DArrayFace;
        v.slices = cube_slices(d->TextureCubeArray.NumCubes);
        break;
    default:
        return d;
    }
    Resource res;
    res.texture = rd.Dimension != D3D12_RESOURCE_DIMENSION_BUFFER && rd.Dimension != D3D12_RESOURCE_DIMENSION_UNKNOWN;
    res.volume = rd.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    res.slices = rd.DepthOrArraySize;
    res.mips = rd.MipLevels;
    Plan p = plan(v, res);
    if (p.action == Action::Pass)
        return d;
    reports.report(r, v, res, p);
    if (p.action == Action::Skip)
        return nullptr;
    c = *d;
    switch (d->ViewDimension) {
    case D3D12_SRV_DIMENSION_TEXTURE1D:
        c.Texture1D.MostDetailedMip = p.most_detailed_mip;
        c.Texture1D.MipLevels = p.mip_levels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE1DARRAY:
        c.Texture1DArray.MostDetailedMip = p.most_detailed_mip;
        c.Texture1DArray.MipLevels = p.mip_levels;
        c.Texture1DArray.ArraySize = p.slices;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2D:
        c.Texture2D.MostDetailedMip = p.most_detailed_mip;
        c.Texture2D.MipLevels = p.mip_levels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2DARRAY:
        c.Texture2DArray.MostDetailedMip = p.most_detailed_mip;
        c.Texture2DArray.MipLevels = p.mip_levels;
        c.Texture2DArray.ArraySize = p.slices;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY:
        c.Texture2DMSArray.ArraySize = p.slices;
        break;
    case D3D12_SRV_DIMENSION_TEXTURE3D:
        c.Texture3D.MostDetailedMip = p.most_detailed_mip;
        c.Texture3D.MipLevels = p.mip_levels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURECUBE:
        c.TextureCube.MostDetailedMip = p.most_detailed_mip;
        c.TextureCube.MipLevels = p.mip_levels;
        break;
    case D3D12_SRV_DIMENSION_TEXTURECUBEARRAY:
        c.TextureCubeArray.MostDetailedMip = p.most_detailed_mip;
        c.TextureCubeArray.MipLevels = p.mip_levels;
        c.TextureCubeArray.NumCubes = p.slices / 6;
        break;
    default:
        break;
    }
    return &c;
}

void run_callbacks(ID3D12Device *dev, ID3D12Resource *r, const D3D12_SHADER_RESOURCE_VIEW_DESC *d, SIZE_T h,
                   const D3D12_RESOURCE_DESC &rd)
{
    swofty::d3d12::SrvCreated v{};
    v.size = sizeof v;
    v.format = uint32_t(rd.Format);
    v.device = dev;
    v.resource = r;
    v.desc = d;
    v.cpu_handle = uint64_t(h);
    v.width = rd.Width;
    v.height = rd.Height;
    v.dimension = uint32_t(rd.Dimension);
    v.depth_or_array_size = rd.DepthOrArraySize;
    v.mip_levels = rd.MipLevels;
    in_callbacks = true;
    callbacks.each([&](swofty::d3d12::SrvFn fn, void *user) { fn(user, &v); });
    in_callbacks = false;
}

void STDMETHODCALLTYPE create_srv_hook(ID3D12Device *dev, ID3D12Resource *r, const D3D12_SHADER_RESOURCE_VIEW_DESC *d, SIZE_T h)
{
    bool clamp = d && clamp_on.load(std::memory_order_relaxed);
    bool notify = callbacks.any() && !in_callbacks;
    if (!r || (!clamp && !notify)) {
        create_original(dev, r, d, h);
        return;
    }
    using GetDescFn = D3D12_RESOURCE_DESC *(STDMETHODCALLTYPE *)(ID3D12Resource *, D3D12_RESOURCE_DESC *);
    D3D12_RESOURCE_DESC rd{};
    reinterpret_cast<GetDescFn>((*reinterpret_cast<void ***>(r))[RESOURCE_GETDESC_SLOT])(r, &rd);
    D3D12_SHADER_RESOURCE_VIEW_DESC c{};
    const D3D12_SHADER_RESOURCE_VIEW_DESC *use = clamp ? clamped(r, rd, d, c) : d;
    if (clamp && !use)
        return;
    create_original(dev, r, use, h);
    if (notify)
        run_callbacks(dev, r, use, h, rd);
}

// Hooks CreateShaderResourceView once, at its code (the vtable entry of the probe device), so the game's device and
// any layer in front of it reach the hook alike.
bool arm()
{
    if (!hooks.gate.offer())
        return hooks.live();
    ID3D12Device *dev = probe_device();
    void **vt = dev ? *reinterpret_cast<void ***>(dev) : nullptr;
    void *target = vt ? vt[SRV_SLOT] : nullptr;
    void *detour = reinterpret_cast<void *>(&create_srv_hook);
    bool ok = install(hooks.create_srv, target, detour, "ID3D12Device::CreateShaderResourceView");
    if (ok)
        swofty::log("[gfx] srv hook armed: ID3D12Device::CreateShaderResourceView (vt+0x90, %s); clamp %s, %s",
                    module_of(target).c_str(), clamp_on.load() ? "on" : "off",
                    callbacks.any() ? "addon view callbacks registered" : "no addon view callback yet");
    else
        swofty::log("[gfx] srv hook: could not hook ID3D12Device::CreateShaderResourceView");
    release(dev);
    return settle(hooks, ok, "could not hook ID3D12Device::CreateShaderResourceView");
}

}

// The ready step: arms the hook when the clamp is on or an addon registered a view callback; otherwise the hook
// arms with the first callback.
bool arm_srv_clamp()
{
    char env[8] = {};
    GetEnvironmentVariableA("SWOFTY_SRV_CLAMP", env, sizeof env);
    clamp_on = srv_clamp::enabled(env);
    if (!clamp_on)
        swofty::log("[gfx] srv clamp is off (SWOFTY_SRV_CLAMP=1 turns it on)");
    bool want = srv_hook::wanted(clamp_on, callbacks.any());
    bool armed = want ? arm() : hooks.live();
    ready_passed = true;
    // A callback registered between the check and ready_passed found the ready step not yet passed: arm for it.
    if (!want && callbacks.any())
        armed = arm();
    if (!want && !armed)
        swofty::log("[gfx] srv hook not armed yet: no clamp and no addon view callback");
    return armed;
}

// 0 registered (live, or arming at the ready step), 1 registered but the hook is not live, -1 invalid, -2 full.
int add_srv_callback(swofty::d3d12::SrvFn fn, void *user, uint32_t *token)
{
    if (!fn)
        return -1;
    uint32_t t = callbacks.add(fn, user);
    if (!t)
        return -2;
    if (token)
        *token = t;
    if (!ready_passed.load())
        return 0;
    return arm() ? 0 : 1;
}

int remove_srv_callback(uint32_t token) { return callbacks.remove(token) ? 0 : -1; }

}

namespace runtime {

void srv_clamp_setup() { render::d3d12::arm_srv_clamp(); }

}

// swofty/d3d12.hpp looks these up by name: a core without them reads as unavailable.
extern "C" __declspec(dllexport) int SwoftyCoreD3d12OnSrv(swofty::d3d12::SrvFn fn, void *user, uint32_t *token)
{
    return render::d3d12::add_srv_callback(fn, user, token);
}

extern "C" __declspec(dllexport) int SwoftyCoreD3d12RemoveSrv(uint32_t token)
{
    return render::d3d12::remove_srv_callback(token);
}
