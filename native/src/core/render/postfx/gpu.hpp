#pragma once
#include "core/render/com.hpp"

#include <windows.h>

#include <d3d12.h>

#include <vector>

namespace render::postfx {

DXGI_FORMAT view_format(DXGI_FORMAT f);
DXGI_FORMAT depth_view(DXGI_FORMAT f);
UINT format_kind(DXGI_FORMAT view);
using com::release;

// Barriers the core records into a list, through the game's unhooked ResourceBarrier.
void transition(ID3D12GraphicsCommandList *list, ID3D12Resource *r, D3D12_RESOURCE_STATES from, D3D12_RESOURCE_STATES to);
void transition_level(ID3D12GraphicsCommandList *list, ID3D12Resource *r, UINT level, D3D12_RESOURCE_STATES from,
                      D3D12_RESOURCE_STATES to);

// The device the passes run on, taken from the game's resource, with the root signature every pass shares
// and the objects and views the passes make on it.
class GpuContext {
public:
    ID3D12Device *device = nullptr;
    ID3D12RootSignature *root = nullptr;
    ID3D12PipelineState *mip_pso = nullptr;

    bool make_root();
    bool make_mip_pso();

    ID3D12Resource *buffer(D3D12_HEAP_TYPE type, UINT64 size, D3D12_RESOURCE_STATES state) const;
    ID3D12Resource *texture(UINT w, UINT h, DXGI_FORMAT f, bool uav, UINT mips = 1) const;
    void srv(ID3D12Resource *r, DXGI_FORMAT f, D3D12_CPU_DESCRIPTOR_HANDLE at, UINT first = 0, UINT count = 0) const;
    // A raw view (ByteAddressBuffer) of bytes [offset, offset + size) of the buffer r; without r, a null view
    // that reads 0.
    void data_srv(ID3D12Resource *r, UINT offset, UINT size, D3D12_CPU_DESCRIPTOR_HANDLE at) const;
    void uav(ID3D12Resource *r, DXGI_FORMAT f, D3D12_CPU_DESCRIPTOR_HANDLE at, UINT mip = 0) const;
};

// Objects the core replaced or dropped, released once the GPU is past every frame that could still use them.
class RetireQueue {
    struct Retired {
        std::vector<IUnknown *> objects;
        UINT64 after = 0;
    };
    std::vector<Retired> retired;

public:
    void retire(std::vector<IUnknown *> objects, unsigned frame_count);
    void free(unsigned frame_count);
};

}
