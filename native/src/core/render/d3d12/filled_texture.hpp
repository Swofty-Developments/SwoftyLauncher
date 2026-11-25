#pragma once
#include <d3d12.h>

namespace render::d3d12 {

// A committed default-heap texture of one subresource that a copy fills completely before anything reads it.
// It is created without zeroing. vkd3d-proton creates a zeroed committed texture in
// VK_IMAGE_LAYOUT_ZERO_INITIALIZED_EXT; a copy of the whole subresource skips the queue's first transition, and in
// some sessions the driver's deferred zero then lands after the copy, so the texture reads as zero for the whole
// session (the overlay's text and images went blank). A texture of more than one subresource, or a runtime that
// rejects D3D12_HEAP_FLAG_CREATE_NOT_ZEROED, gets a zeroed texture.
inline HRESULT create_filled_texture(ID3D12Device *dev, const D3D12_RESOURCE_DESC &desc, D3D12_RESOURCE_STATES state,
                                     ID3D12Resource **out)
{
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    bool one = desc.MipLevels == 1 && (desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D || desc.DepthOrArraySize == 1);
    if (one && SUCCEEDED(dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_CREATE_NOT_ZEROED, &desc, state, nullptr,
                                                      IID_PPV_ARGS(out))))
        return S_OK;
    return dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, state, nullptr, IID_PPV_ARGS(out));
}

}
