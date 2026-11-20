#include "core/render/postfx/depth_copy.hpp"

#include "core/render/d3d12_hooks.hpp"

#include <swofty/mixin.hpp>

namespace render::postfx {

ID3D12Resource *DepthCopy::for_source(ID3D12Resource *src, const D3D12_RESOURCE_DESC &d, const GpuContext &gpu,
                                      RetireQueue &retired, unsigned frame_count)
{
    ID3D12Resource *have = copy.load(std::memory_order_acquire);
    const D3D12_RESOURCE_DESC &h = copy_desc;
    if (have && copy_of == src && h.Width == d.Width && h.Height == d.Height && h.Format == d.Format &&
        h.MipLevels == d.MipLevels && h.DepthOrArraySize == d.DepthOrArraySize)
        return have;
    if (!gpu.device || failed)
        return nullptr;
    D3D12_RESOURCE_DESC cd = d;
    cd.Flags &= ~D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE;
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    ID3D12Resource *r = nullptr;
    if (FAILED(gpu.device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &cd,
                                                   D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, nullptr, IID_PPV_ARGS(&r)))) {
        failed = true;
        swofty::log("[postfx] could not make a copy of the game's depth buffer (%llux%u, format %d); passes read "
                    "depth only where the game leaves it readable",
                    static_cast<unsigned long long>(d.Width), d.Height, int(d.Format));
        return nullptr;
    }
    retired.retire({copy.exchange(r, std::memory_order_acq_rel)}, frame_count);
    copy_of = src;
    copy_desc = d;
    if (!logged) {
        logged = true;
        swofty::log("[postfx] the core copies the game's depth buffer (%llux%u, format %d) where the game stops "
                    "writing it, for passes that run where it isn't readable",
                    static_cast<unsigned long long>(d.Width), d.Height, int(d.Format));
    }
    return r;
}

void DepthCopy::record(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, const GpuContext &gpu,
                       const ResourceWatch &watched, std::recursive_mutex &lock, RetireQueue &retired,
                       const unsigned &frame_count)
{
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION || !(watched.bits(b.Transition.pResource) & W_DEPTH) ||
            b.Flags != D3D12_RESOURCE_BARRIER_FLAG_NONE || b.Transition.Subresource != D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES ||
            !(b.Transition.StateBefore & D3D12_RESOURCE_STATE_DEPTH_WRITE) ||
            (b.Transition.StateAfter & D3D12_RESOURCE_STATE_DEPTH_WRITE))
            continue;
        if (list->GetType() != D3D12_COMMAND_LIST_TYPE_DIRECT)
            return;
        ID3D12Resource *src = b.Transition.pResource;
        D3D12_RESOURCE_DESC d = src->GetDesc();
        if (d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || d.SampleDesc.Count != 1 ||
            depth_view(d.Format) == DXGI_FORMAT_UNKNOWN)
            return;
        ID3D12Resource *dst;
        {
            std::lock_guard<std::recursive_mutex> g(lock);
            dst = for_source(src, d, gpu, retired, frame_count);
        }
        if (!dst)
            return;
        D3D12_RESOURCE_BARRIER to[2]{}, back[2]{};
        for (auto *t : {to, back})
            for (int j = 0; j < 2; j++) {
                t[j].Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
                t[j].Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
            }
        to[0].Transition.pResource = back[0].Transition.pResource = src;
        to[0].Transition.StateBefore = back[0].Transition.StateAfter = b.Transition.StateBefore;
        to[0].Transition.StateAfter = back[0].Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_SOURCE;
        to[1].Transition.pResource = back[1].Transition.pResource = dst;
        to[1].Transition.StateBefore = back[1].Transition.StateAfter = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        to[1].Transition.StateAfter = back[1].Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
        d3d12::barrier_original(list, 2u, to);
        list->CopyResource(dst, src);
        d3d12::barrier_original(list, 2u, back);
        copied_at.store(frame_count, std::memory_order_relaxed);
        return;
    }
}

ID3D12Resource *DepthCopy::fresh(unsigned frame_count)
{
    until.store(frame_count + FRAMES, std::memory_order_relaxed);
    ID3D12Resource *dc = copy.load(std::memory_order_acquire);
    if (dc && frame_count - copied_at.load(std::memory_order_relaxed) <= 2)
        return dc;
    return nullptr;
}

}
