#include "core/hud/overlay/textures.hpp"

#include "core/diagnostics/steps.hpp"
#include "core/hud/hud_font_atlas.hpp"
#include "core/hud/overlay/overlay.hpp"
#include "core/render/d3d12/filled_texture.hpp"

#include <swofty/mixin.hpp>

#include <cstring>

namespace hud::overlay {

namespace {

void decode_font(hud::img::Pixels &px)
{
    perf::Stopwatch font;
    if (!hud::img::grey(hud::font::ATLAS_PNG, hud::font::ATLAS_PNG_SIZE, px))
        swofty::log("[overlay] decoding the built-in font atlas failed; text draws nothing");
    swofty::log(perf::STEP_LINE, font.total_ms(), "overlay font atlas decode");
}

}

bool Textures::upload(Renderer &r, ID3D12GraphicsCommandList *l, UINT index, DXGI_FORMAT fmt, UINT w, UINT h, UINT bpp,
                      const uint8_t *pixels)
{
    D3D12_RESOURCE_DESC rd{};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    rd.Width = w;
    rd.Height = h;
    rd.DepthOrArraySize = 1;
    rd.MipLevels = 1;
    rd.Format = fmt;
    rd.SampleDesc.Count = 1;
    ID3D12Resource *tex = nullptr;
    if (FAILED(render::d3d12::create_filled_texture(r.dev, rd, D3D12_RESOURCE_STATE_COPY_DEST, &tex)))
        return false;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT64 total = 0;
    r.dev->GetCopyableFootprints(&rd, 0, 1, 0, &fp, nullptr, nullptr, &total);
    ID3D12Resource *staging = r.buffer(D3D12_HEAP_TYPE_UPLOAD, total, D3D12_RESOURCE_STATE_GENERIC_READ);
    uint8_t *map = nullptr;
    D3D12_RANGE none{0, 0};
    if (!staging || FAILED(staging->Map(0, &none, reinterpret_cast<void **>(&map))) || !map) {
        release(staging);
        tex->Release();
        return false;
    }
    for (UINT y = 0; y < h; y++)
        memcpy(map + fp.Offset + SIZE_T(y) * fp.Footprint.RowPitch, pixels + SIZE_T(y) * w * bpp, SIZE_T(w) * bpp);
    staging->Unmap(0, nullptr);
    D3D12_TEXTURE_COPY_LOCATION dst{}, src{};
    dst.pResource = tex;
    dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    src.pResource = staging;
    src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    src.PlacedFootprint = fp;
    l->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    Renderer::transition(l, tex, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
    D3D12_SHADER_RESOURCE_VIEW_DESC sv{};
    sv.Format = fmt;
    sv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    sv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    sv.Texture2D.MipLevels = 1;
    r.dev->CreateShaderResourceView(tex, &sv, r.tex_cpu(index));
    r.graveyard.push_back({r.fence_val + 1, staging});
    if (entries[index].res)
        r.graveyard.push_back({r.fence_val + 1, entries[index].res});
    entries[index].res = tex;
    uploads++;
    return true;
}

void Textures::sync(Renderer &r, ID3D12GraphicsCommandList *l)
{
    if (const hud::img::Pixels *px = font.pending(decode_font)) {
        if (upload(r, l, hud::drawlist::TEX_FONT, DXGI_FORMAT_R8_UNORM, px->width, px->height, 1, px->rgba.data()))
            swofty::log("[overlay] font atlas uploaded: %s, %ux%u SDF", hud::font::NAME, px->width, px->height);
        else
            swofty::log("[overlay] uploading the font atlas failed; text draws nothing");
        font.done();
    }
    std::lock_guard<std::mutex> g(overlay.reg_lock);
    for (size_t slot = 0; slot < overlay.reg.image_count(); slot++) {
        hud::drawlist::Image *im = overlay.reg.image(int(slot));
        UINT index = UINT(slot + 1);
        if (!im || im->rgba.empty() || index >= TEX_MAX || entries[index].generation == im->generation)
            continue;
        bool grey = im->channels == 1;
        bool ok = upload(r, l, index, grey ? DXGI_FORMAT_R8_UNORM : DXGI_FORMAT_R8G8B8A8_UNORM, im->width, im->height,
                         grey ? 1 : 4, im->rgba.data());
        if (!im->keep || !ok)
            swofty::log("[overlay] %s %s (%ux%u) %s", im->keep ? "font page" : "image",
                        im->id.substr(im->id.find('\n') + 1).c_str(), im->width, im->height,
                        ok ? "uploaded" : "upload failed; it draws nothing");
        entries[index].generation = im->generation;
        if (!im->keep) {
            im->rgba.clear();
            im->rgba.shrink_to_fit();
        }
    }
}

void Textures::release_all()
{
    for (auto &t : entries)
        release(t.res);
    entries.clear();
}

}
