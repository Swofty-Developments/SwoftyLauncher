#include "core/render/postfx/gpu.hpp"

#include "core/render/d3d12_hooks.hpp"
#include "core/render/postfx/pass.hpp"

#include <swofty/mixin.hpp>

#include <d3dcompiler.h>

#include <algorithm>

namespace render::postfx {

namespace {

using SerializeRootFn = HRESULT(WINAPI *)(const D3D12_ROOT_SIGNATURE_DESC *, D3D_ROOT_SIGNATURE_VERSION, ID3DBlob **,
                                          ID3DBlob **);

}

DXGI_FORMAT view_format(DXGI_FORMAT f)
{
    switch (f) {
    case DXGI_FORMAT_R8G8B8A8_UNORM:
    case DXGI_FORMAT_R8G8B8A8_UNORM_SRGB:
    case DXGI_FORMAT_R8G8B8A8_TYPELESS:
        return DXGI_FORMAT_R8G8B8A8_UNORM;
    case DXGI_FORMAT_R10G10B10A2_UNORM:
    case DXGI_FORMAT_R10G10B10A2_TYPELESS:
        return DXGI_FORMAT_R10G10B10A2_UNORM;
    case DXGI_FORMAT_R16G16B16A16_FLOAT:
    case DXGI_FORMAT_R16G16B16A16_TYPELESS:
        return DXGI_FORMAT_R16G16B16A16_FLOAT;
    case DXGI_FORMAT_R11G11B10_FLOAT:
        return DXGI_FORMAT_R11G11B10_FLOAT;
    default:
        return DXGI_FORMAT_UNKNOWN;
    }
}

UINT format_kind(DXGI_FORMAT view)
{
    return view == DXGI_FORMAT_R10G10B10A2_UNORM ? 1 : view == DXGI_FORMAT_R16G16B16A16_FLOAT ? 2 : 0;
}

DXGI_FORMAT depth_view(DXGI_FORMAT f)
{
    switch (f) {
    case DXGI_FORMAT_R24G8_TYPELESS:
        return DXGI_FORMAT_R24_UNORM_X8_TYPELESS;
    case DXGI_FORMAT_R32G8X24_TYPELESS:
        return DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS;
    case DXGI_FORMAT_R32_TYPELESS:
        return DXGI_FORMAT_R32_FLOAT;
    case DXGI_FORMAT_R16_TYPELESS:
        return DXGI_FORMAT_R16_UNORM;
    default:
        return DXGI_FORMAT_UNKNOWN;
    }
}

void transition(ID3D12GraphicsCommandList *list, ID3D12Resource *r, D3D12_RESOURCE_STATES from, D3D12_RESOURCE_STATES to)
{
    if (from == to || !r)
        return;
    D3D12_RESOURCE_BARRIER b{};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition.pResource = r;
    b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    b.Transition.StateBefore = from;
    b.Transition.StateAfter = to;
    d3d12::barrier_original(list, 1u, &b);
}

void transition_level(ID3D12GraphicsCommandList *list, ID3D12Resource *r, UINT level, D3D12_RESOURCE_STATES from,
                      D3D12_RESOURCE_STATES to)
{
    D3D12_RESOURCE_BARRIER b{};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition.pResource = r;
    b.Transition.Subresource = level;
    b.Transition.StateBefore = from;
    b.Transition.StateAfter = to;
    d3d12::barrier_original(list, 1u, &b);
}

ID3D12Resource *GpuContext::buffer(D3D12_HEAP_TYPE type, UINT64 size, D3D12_RESOURCE_STATES state) const
{
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = type;
    D3D12_RESOURCE_DESC bd{};
    bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    bd.Width = size;
    bd.Height = 1;
    bd.DepthOrArraySize = 1;
    bd.MipLevels = 1;
    bd.SampleDesc.Count = 1;
    bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    ID3D12Resource *r = nullptr;
    return SUCCEEDED(device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd, state, nullptr, IID_PPV_ARGS(&r)))
               ? r
               : nullptr;
}

ID3D12Resource *GpuContext::texture(UINT w, UINT h, DXGI_FORMAT f, bool uav, UINT mips) const
{
    D3D12_RESOURCE_DESC td{};
    td.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    td.Width = w;
    td.Height = h;
    td.DepthOrArraySize = 1;
    td.MipLevels = UINT16(mips);
    td.Format = f;
    td.SampleDesc = {1, 0};
    td.Flags = uav ? D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS : D3D12_RESOURCE_FLAG_NONE;
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    ID3D12Resource *r = nullptr;
    return SUCCEEDED(device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &td, HOME, nullptr, IID_PPV_ARGS(&r)))
               ? r
               : nullptr;
}

bool GpuContext::make_root()
{
    auto serialize =
        reinterpret_cast<SerializeRootFn>(GetProcAddress(GetModuleHandleA("d3d12.dll"), "D3D12SerializeRootSignature"));
    if (!serialize)
        return false;
    D3D12_DESCRIPTOR_RANGE ranges[3]{};
    ranges[0].RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    ranges[0].NumDescriptors = DATA_AT + 1;
    ranges[0].OffsetInDescriptorsFromTableStart = 0;
    ranges[1].RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_UAV;
    ranges[1].NumDescriptors = 1;
    ranges[1].OffsetInDescriptorsFromTableStart = TABLE - 1;
    D3D12_ROOT_PARAMETER params[2]{};
    params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    params[0].Constants.Num32BitValues = CONSTANTS;
    params[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    params[1].DescriptorTable.NumDescriptorRanges = 2;
    params[1].DescriptorTable.pDescriptorRanges = ranges;
    params[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    D3D12_STATIC_SAMPLER_DESC samplers[4]{};
    for (UINT i = 0; i < 4; i++) {
        samplers[i].Filter = i % 2 == 0 ? D3D12_FILTER_MIN_MAG_MIP_LINEAR : D3D12_FILTER_MIN_MAG_MIP_POINT;
        samplers[i].AddressU = samplers[i].AddressV = samplers[i].AddressW =
            i < 2 ? D3D12_TEXTURE_ADDRESS_MODE_CLAMP : D3D12_TEXTURE_ADDRESS_MODE_WRAP;
        samplers[i].MaxLOD = D3D12_FLOAT32_MAX;
        samplers[i].ShaderRegister = i;
        samplers[i].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    }
    D3D12_ROOT_SIGNATURE_DESC rd{};
    rd.NumParameters = 2;
    rd.pParameters = params;
    rd.NumStaticSamplers = 4;
    rd.pStaticSamplers = samplers;
    ID3DBlob *blob = nullptr, *err = nullptr;
    HRESULT hr = serialize(&rd, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &err);
    if (SUCCEEDED(hr))
        hr = device->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(), IID_PPV_ARGS(&root));
    release(blob);
    release(err);
    return SUCCEEDED(hr);
}

bool GpuContext::make_mip_pso()
{
    HMODULE compiler = LoadLibraryA("d3dcompiler_47.dll");
    auto compile = compiler ? reinterpret_cast<pD3DCompile>(GetProcAddress(compiler, "D3DCompile")) : nullptr;
    if (!compile) {
        swofty::log("[postfx] d3dcompiler_47.dll!D3DCompile is unavailable; mipmapped textures keep only their first level");
        return false;
    }
    static constexpr char hlsl[] =
        "Texture2D<float4> src:register(t2);RWTexture2D<float4> dst:register(u0);"
        "[numthreads(8,8,1)]void CS(uint3 id:SV_DispatchThreadID){uint w,h;dst.GetDimensions(w,h);"
        "if(id.x>=w||id.y>=h)return;uint sw,sh;src.GetDimensions(sw,sh);int2 m=int2(sw,sh)-1;int2 b=int2(id.xy)*2;"
        "dst[id.xy]=0.25*(src.Load(int3(min(b,m),0))+src.Load(int3(min(b+int2(1,0),m),0))"
        "+src.Load(int3(min(b+int2(0,1),m),0))+src.Load(int3(min(b+int2(1,1),m),0)));}";
    ID3DBlob *code = nullptr, *err = nullptr;
    HRESULT hr = compile(hlsl, sizeof hlsl - 1, nullptr, nullptr, nullptr, "CS", "cs_5_0", 0, 0, &code, &err);
    if (err) {
        if (FAILED(hr))
            swofty::log("[postfx] mip downsample: %.*s", int(err->GetBufferSize()),
                        static_cast<const char *>(err->GetBufferPointer()));
        err->Release();
    }
    if (SUCCEEDED(hr) && code) {
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = root;
        pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
        hr = device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&mip_pso));
    }
    release(code);
    if (FAILED(hr))
        swofty::log(
            "[postfx] the mip downsample pipeline could not be made (0x%08lx); mipmapped textures keep only their first level",
            hr);
    return SUCCEEDED(hr);
}

void GpuContext::srv(ID3D12Resource *r, DXGI_FORMAT f, D3D12_CPU_DESCRIPTOR_HANDLE at, UINT first, UINT count) const
{
    D3D12_SHADER_RESOURCE_VIEW_DESC sv{};
    sv.Format = f;
    sv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    sv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    sv.Texture2D.MostDetailedMip = first;
    sv.Texture2D.MipLevels = count ? count : r ? UINT(r->GetDesc().MipLevels) : 1;
    device->CreateShaderResourceView(r, &sv, at);
}

void GpuContext::data_srv(ID3D12Resource *r, UINT offset, UINT size, D3D12_CPU_DESCRIPTOR_HANDLE at) const
{
    D3D12_SHADER_RESOURCE_VIEW_DESC sv{};
    sv.Format = DXGI_FORMAT_R32_TYPELESS;
    sv.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
    sv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    sv.Buffer.FirstElement = offset / 4;
    sv.Buffer.NumElements = r ? std::max(1u, (size + 3) / 4) : 1;
    sv.Buffer.Flags = D3D12_BUFFER_SRV_FLAG_RAW;
    device->CreateShaderResourceView(r, &sv, at);
}

void GpuContext::uav(ID3D12Resource *r, DXGI_FORMAT f, D3D12_CPU_DESCRIPTOR_HANDLE at, UINT mip) const
{
    D3D12_UNORDERED_ACCESS_VIEW_DESC uv{};
    uv.Format = f;
    uv.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
    uv.Texture2D.MipSlice = mip;
    device->CreateUnorderedAccessView(r, nullptr, &uv, at);
}

void RetireQueue::retire(std::vector<IUnknown *> objects, unsigned frame_count)
{
    std::erase(objects, nullptr);
    if (!objects.empty())
        retired.push_back({std::move(objects), UINT64(frame_count) + 2 * FRAMES});
}

void RetireQueue::free(unsigned frame_count)
{
    std::erase_if(retired, [&](Retired &r) {
        if (frame_count < r.after)
            return false;
        for (auto *o : r.objects)
            o->Release();
        return true;
    });
}

}
