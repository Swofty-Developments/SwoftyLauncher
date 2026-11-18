#pragma once
#include "core/render/postfx/pass.hpp"

#include <windows.h>

#include <d3d12.h>

#include <vector>

namespace render::postfx {

class Pipeline;

// A slot's working textures: the picture before the passes, the two the passes write in turn, and
// the addon textures that are neither persistent nor images. Each slot has its own, so passes of two
// frames that the GPU runs at once on different queues never write the same texture.
struct SlotTextures {
    ID3D12Resource *orig = nullptr, *pic[2] = {};
    std::vector<ID3D12Resource *> tex;
    bool clear = false;
};

// The core's textures for one picture size: each slot's working set, last frame's result, and the
// descriptor heaps.
struct PictureTextures {
    D3D12_RESOURCE_DESC desc{};
    DXGI_FORMAT view = DXGI_FORMAT_UNKNOWN;
    SlotTextures work[FRAMES];
    ID3D12Resource *prev = nullptr;
    ID3D12DescriptorHeap *heap = nullptr, *cpu = nullptr;
    UINT stride = 0;
    bool prev_valid = false;
    uint32_t prev_serial = 0;
    unsigned prev_frame = 0;
};

// The picture's textures with the addons' persistent textures and images, made for the game's picture size and
// remade when it changes; replaced objects are retired until the GPU is past them.
class FrameTextures : public PictureTextures {
public:
    explicit FrameTextures(Pipeline &owner) : pipeline(owner) {}

    // Counts the times the core made its heap or textures, so slots know their descriptor tables are old.
    unsigned textures_made = 0;
    UINT mip_steps_used = 0;

    // Makes the textures for a picture of desc's size; the ones being replaced go once the GPU is past them.
    bool make_frame(const D3D12_RESOURCE_DESC &d);
    // A slot's working set, made the first time the slot runs on this picture size.
    bool ensure_work(int k);
    // Makes what the frame needs for the textures a new plan added, and a descriptor heap sized for them.
    bool fit_frame();
    ID3D12Resource *tex_of(size_t i, int k);
    ID3D12Resource *input_res(int in, int k, DXGI_FORMAT &f);
    D3D12_CPU_DESCRIPTOR_HANDLE cpu_at(ID3D12DescriptorHeap *h, UINT i);
    D3D12_GPU_DESCRIPTOR_HANDLE gpu_at(ID3D12DescriptorHeap *h, UINT i);
    // Fills levels 1.. of r from level 0, which the caller has left in HOME: each level is written as a UAV, then
    // moved to HOME before the next one reads it.
    void build_mips(ID3D12GraphicsCommandList *l, ID3D12Resource *r, DXGI_FORMAT f, UINT w, UINT h, UINT levels, int k);
    void upload(ID3D12GraphicsCommandList *l, PassTexture &t);

private:
    UINT mip_base();
    bool make_heaps();

    Pipeline &pipeline;
};

}
