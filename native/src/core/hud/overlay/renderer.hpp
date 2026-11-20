#pragma once
#include <windows.h>

#include "core/hud/hud_drawlist.hpp"
#include "core/hud/overlay/textures.hpp"
#include "core/render/com.hpp"

#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_6.h>

#include <vector>

namespace hud::overlay {

using Vertex = hud::drawlist::Vertex;
using render::com::release;

constexpr UINT MAX_VERTS = hud::drawlist::MAX_VERTS;
constexpr UINT SLOTS = 3;

struct Pso {
    DXGI_FORMAT fmt;
    ID3D12PipelineState *pso;
};

struct Slot {
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    UINT64 done_at = 0;
};

// The draw list's renderer: solid, image and SDF-text triangles into a layer the size of the back buffer, one
// texture and one scissor rect per batch, then the layer composited onto the back buffer in its output's colour
// encoding. A ring of command lists behind one fence; a frame whose slot is still on the GPU is skipped.
struct Renderer {
    PFN_D3D12_SERIALIZE_ROOT_SIGNATURE serialize_root = nullptr;
    ID3D12Device *dev = nullptr;
    ID3D12RootSignature *root = nullptr, *comp_root = nullptr;
    ID3DBlob *vs = nullptr, *ps = nullptr, *cvs = nullptr, *cps = nullptr;
    ID3D12PipelineState *geo = nullptr;
    std::vector<Pso> psos;
    ID3D12DescriptorHeap *rtv_heap = nullptr, *srv_heap = nullptr;
    UINT rtv_inc = 0, srv_inc = 0;
    Slot slots[SLOTS];
    UINT next = 0;
    ID3D12Resource *vbuf = nullptr;
    uint8_t *vmap = nullptr;
    ID3D12Fence *fence = nullptr;
    UINT64 fence_val = 0;
    ID3D12Resource *layer = nullptr;
    UINT layer_w = 0, layer_h = 0;
    D3D12_RESOURCE_STATES layer_state = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    std::vector<std::pair<UINT64, ID3D12Resource *>> graveyard;
    ID3D12QueryHeap *ts_heap = nullptr;
    ID3D12Resource *ts_buf = nullptr;
    UINT64 ts_freq = 0;
    double gpu_ms = 0;
    unsigned gpu_n = 0;
    bool ready = false;
    bool failed = false;

    static constexpr DXGI_FORMAT LAYER_FMT = DXGI_FORMAT_R8G8B8A8_UNORM;

    bool compile(pD3DCompile fn, const char *entry, const char *model, ID3DBlob **out);
    ID3D12RootSignature *make_sig(const D3D12_ROOT_SIGNATURE_DESC &rs);
    bool make_root();
    ID3D12PipelineState *make_pso(DXGI_FORMAT fmt, bool composite);
    ID3D12PipelineState *pso_for(DXGI_FORMAT fmt);
    ID3D12Resource *buffer(D3D12_HEAP_TYPE type, UINT64 size, D3D12_RESOURCE_STATES state);
    bool make_resources(ID3D12CommandQueue *q);

    std::vector<ID3D12CommandQueue *> queues;
    Textures textures;

    D3D12_CPU_DESCRIPTOR_HANDLE tex_cpu(UINT i);
    D3D12_GPU_DESCRIPTOR_HANDLE tex_gpu(UINT i);
    bool same_device(ID3D12CommandQueue *q);
    bool init(ID3D12CommandQueue *q);
    void bury();
    bool ensure_layer(UINT w, UINT h);
    void read_timestamps(UINT slot);
    static void transition(ID3D12GraphicsCommandList *l, ID3D12Resource *r, D3D12_RESOURCE_STATES from, D3D12_RESOURCE_STATES to);

    enum Outcome { drawn, busy, broken };

    struct Target {
        ID3D12Resource *res;
        DXGI_FORMAT fmt;
        UINT width, height;
        D3D12_RESOURCE_STATES state;
    };

    Outcome render(const Target &t, ID3D12CommandQueue *q, const std::vector<Vertex> &verts,
                   const std::vector<hud::drawlist::Batch> &batches, UINT mode, float white);
    void shutdown();
};

}
