#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/srv_clamp_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <type_traits>
#include <unordered_set>

namespace render::d3d12 {

namespace {

constexpr size_t RESOURCE_GETDESC_SLOT = 10, SRV_SLOT = 19;

using SrvFn = void(STDMETHODCALLTYPE *)(ID3D12Device *, ID3D12Resource *, const D3D12_SHADER_RESOURCE_VIEW_DESC *, SIZE_T);

// The srv clamp set: the device's CreateShaderResourceView, patched in its vtable.
struct SrvHooks : runtime::loader::HookSet<> {
    runtime::loader::Original<std::remove_pointer_t<SrvFn>> create_srv;
};

constinit SrvHooks hooks;

const GUID kNameW = {0x4cca5fd8, 0x921f, 0x42c8, {0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41}};

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

void STDMETHODCALLTYPE srv_hook(ID3D12Device *dev, ID3D12Resource *r, const D3D12_SHADER_RESOURCE_VIEW_DESC *d, SIZE_T h)
{
    using namespace srv_clamp;
    if (!r || !d) {
        hooks.create_srv(dev, r, d, h);
        return;
    }
    using GetDescFn = D3D12_RESOURCE_DESC *(STDMETHODCALLTYPE *)(ID3D12Resource *, D3D12_RESOURCE_DESC *);
    D3D12_RESOURCE_DESC rd{};
    reinterpret_cast<GetDescFn>((*reinterpret_cast<void ***>(r))[RESOURCE_GETDESC_SLOT])(r, &rd);
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
        hooks.create_srv(dev, r, d, h);
        return;
    }
    Resource res;
    res.texture = rd.Dimension != D3D12_RESOURCE_DIMENSION_BUFFER && rd.Dimension != D3D12_RESOURCE_DIMENSION_UNKNOWN;
    res.volume = rd.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    res.slices = rd.DepthOrArraySize;
    res.mips = rd.MipLevels;
    Plan p = plan(v, res);
    if (p.action == Action::Pass) {
        hooks.create_srv(dev, r, d, h);
        return;
    }
    reports.report(r, v, res, p);
    if (p.action == Action::Skip)
        return;
    D3D12_SHADER_RESOURCE_VIEW_DESC c = *d;
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
    hooks.create_srv(dev, r, &c, h);
}

}

bool arm_srv_clamp()
{
    if (!hooks.gate.offer())
        return hooks.live();
    char env[8] = {};
    GetEnvironmentVariableA("SWOFTY_SRV_CLAMP", env, sizeof env);
    if (!srv_clamp::enabled(env)) {
        swofty::log("[gfx] srv clamp is off (SWOFTY_SRV_CLAMP=1 turns it on)");
        return settle(hooks, false, "SWOFTY_SRV_CLAMP is not 1");
    }
    ID3D12Device *dev = probe_device();
    void **vt = dev ? *reinterpret_cast<void ***>(dev) : nullptr;
    void *srv_target = vt ? vt[SRV_SLOT] : nullptr;
    HMODULE srv_mod = nullptr;
    module_of(srv_target, &srv_mod);
    unsigned long long srv_offset = reinterpret_cast<uintptr_t>(srv_target) - reinterpret_cast<uintptr_t>(srv_mod);
    DWORD old = 0;
    bool ok = vt && VirtualProtect(&vt[SRV_SLOT], sizeof(void *), PAGE_READWRITE, &old);
    if (ok) {
        *hooks.create_srv.slot() = srv_target;
        vt[SRV_SLOT] = reinterpret_cast<void *>(&srv_hook);
        VirtualProtect(&vt[SRV_SLOT], sizeof(void *), old, &old);
        swofty::log("[gfx] srv clamp armed: ID3D12Device::CreateShaderResourceView (vt+0x98, vtable entry for %s+0x%llx) "
                    "patched; array and mip views past their texture are clamped",
                    module_of(srv_target).c_str(), srv_offset);
    } else
        swofty::log("[gfx] srv clamp: could not patch the ID3D12Device vtable");
    release(dev);
    return settle(hooks, ok, "could not patch the ID3D12Device vtable");
}

}

namespace runtime {

void srv_clamp_setup() { render::d3d12::arm_srv_clamp(); }

}
