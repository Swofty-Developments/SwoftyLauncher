#pragma once
#include <windows.h>

#include "core/hud/font_upload.hpp"
#include "core/hud/hud_image.hpp"

#include <d3d12.h>

#include <cstdint>
#include <vector>

namespace hud::overlay {

constexpr UINT TEX_MAX = 256;

struct Renderer;

// What the batches sample: the built-in SDF font atlas and the addons' images, one texture per slot. Each uploads on
// the overlay's own command list the first frame after it loads.
class Textures {
public:
    void reset() { entries.assign(TEX_MAX, {}); }
    void sync(Renderer &r, ID3D12GraphicsCommandList *l);
    void release_all();

private:
    bool upload(Renderer &r, ID3D12GraphicsCommandList *l, UINT index, DXGI_FORMAT fmt, UINT w, UINT h, UINT bpp,
                const uint8_t *pixels);

    struct Tex {
        ID3D12Resource *res = nullptr;
        uint32_t generation = 0;
    };
    std::vector<Tex> entries;
    hud::FontUpload font;
    unsigned uploads = 0;
};

}
