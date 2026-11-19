#include "core/render/postfx/frame_textures.hpp"

#include "core/render/postfx/pipeline.hpp"

#include <swofty/mixin.hpp>

#include <cstring>

namespace render::postfx {

UINT FrameTextures::mip_base() { return FRAMES * MAX_PASSES * TABLE + UINT(pipeline.config.textures.size()) * (FRAMES + 1) + 1; }

bool FrameTextures::make_heaps()
{
    D3D12_DESCRIPTOR_HEAP_DESC hd{};
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    hd.NumDescriptors = mip_base() + FRAMES * MIP_STEPS * TABLE;
    hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    D3D12_DESCRIPTOR_HEAP_DESC cd = hd;
    cd.NumDescriptors = UINT(pipeline.config.textures.size()) * (FRAMES + 1) + 1;
    cd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;
    if (FAILED(pipeline.gpu.device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap))) ||
        FAILED(pipeline.gpu.device->CreateDescriptorHeap(&cd, IID_PPV_ARGS(&cpu))))
        return false;
    stride = pipeline.gpu.device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    return true;
}

bool FrameTextures::make_frame(const D3D12_RESOURCE_DESC &d)
{
    DXGI_FORMAT view = view_format(d.Format);
    if (view == DXGI_FORMAT_UNKNOWN) {
        swofty::log("[postfx] the game's picture has format %d, which the passes do not take; post-processing is off",
                    int(d.Format));
        return false;
    }
    std::vector<IUnknown *> old = {prev, heap, cpu};
    for (auto &wk : work) {
        old.insert(old.end(), {wk.orig, wk.pic[0], wk.pic[1]});
        old.insert(old.end(), wk.tex.begin(), wk.tex.end());
    }
    pipeline.placement.drop_depths(nullptr);
    static_cast<PictureTextures &>(*this) = PictureTextures{};
    textures_made++;
    desc = d;
    this->view = view;
    UINT w = UINT(d.Width), h = d.Height;
    if (pipeline.config.uses_previous)
        prev = pipeline.gpu.texture(w, h, view, false);
    for (auto &t : pipeline.config.textures) {
        if (!t.file.empty() && t.res)
            continue;
        postfx::Size s = postfx::texture_size(t.scale, t.width, t.height, w, h);
        t.w = s.w;
        t.h = s.h;
        if (!t.persistent && t.file.empty())
            continue;
        old.push_back(t.res);
        t.res = pipeline.gpu.texture(t.w, t.h, t.format, t.file.empty(), t.levels());
        t.clear = t.file.empty();
        if (!t.res) {
            swofty::log("[postfx] texture %s (%ux%u, format %d) could not be made; post-processing is off", t.name.c_str(), t.w,
                        t.h, int(t.format));
            return false;
        }
    }
    pipeline.retire(std::move(old));
    if ((pipeline.config.uses_previous && !prev) || !make_heaps())
        return false;
    pipeline.capture.remake(pipeline.gpu, d, pipeline.retired, pipeline.frame_count);
    swofty::log("[postfx] passes run on the game's %llux%u picture (format %d) with %zu texture(s) of their own",
                (unsigned long long)d.Width, d.Height, int(d.Format), pipeline.config.textures.size());
    return true;
}

bool FrameTextures::ensure_work(int k)
{
    SlotTextures &wk = work[k];
    if (wk.orig)
        return true;
    textures_made++;
    UINT w = UINT(desc.Width), h = desc.Height;
    wk.orig = pipeline.gpu.texture(w, h, view, false);
    wk.pic[0] = pipeline.gpu.texture(w, h, view, true);
    wk.pic[1] = pipeline.gpu.texture(w, h, view, true);
    wk.tex.assign(pipeline.config.textures.size(), nullptr);
    bool ok = wk.orig && wk.pic[0] && wk.pic[1];
    for (size_t i = 0; i < pipeline.config.textures.size() && ok; i++)
        if (!pipeline.config.textures[i].shared())
            ok = (wk.tex[i] = pipeline.gpu.texture(pipeline.config.textures[i].w, pipeline.config.textures[i].h,
                                                   pipeline.config.textures[i].format, true,
                                                   pipeline.config.textures[i].levels())) != nullptr;
    wk.clear = true;
    if (!ok)
        swofty::log("[postfx] the working textures of slot %d could not be made; post-processing is off", k);
    return ok;
}

ID3D12Resource *FrameTextures::tex_of(size_t i, int k)
{
    return pipeline.config.textures[i].shared() ? pipeline.config.textures[i].res : work[k].tex[i];
}

D3D12_CPU_DESCRIPTOR_HANDLE FrameTextures::cpu_at(ID3D12DescriptorHeap *h, UINT i)
{
    return {h->GetCPUDescriptorHandleForHeapStart().ptr + SIZE_T(i) * stride};
}

D3D12_GPU_DESCRIPTOR_HANDLE FrameTextures::gpu_at(ID3D12DescriptorHeap *h, UINT i)
{
    return {h->GetGPUDescriptorHandleForHeapStart().ptr + UINT64(i) * stride};
}

void FrameTextures::build_mips(ID3D12GraphicsCommandList *l, ID3D12Resource *r, DXGI_FORMAT f, UINT w, UINT h, UINT levels, int k)
{
    if (!pipeline.gpu.mip_pso || levels < 2)
        return;
    l->SetPipelineState(pipeline.gpu.mip_pso);
    for (UINT level = 1; level < levels; level++) {
        if (mip_steps_used >= MIP_STEPS)
            return;
        UINT at = mip_base() + (UINT(k) * MIP_STEPS + mip_steps_used++) * TABLE;
        for (UINT i = 0; i < 2 + INPUTS; i++)
            if (i == 2)
                pipeline.gpu.srv(r, f, cpu_at(heap, at + i), level - 1, 1);
            else
                pipeline.gpu.srv(nullptr, f, cpu_at(heap, at + i));
        pipeline.gpu.data_srv(nullptr, 0, 0, cpu_at(heap, at + DATA_AT));
        pipeline.gpu.uav(r, f, cpu_at(heap, at + TABLE - 1), level);
        postfx::Size to = postfx::mip_size(w, h, level);
        transition_level(l, r, level, HOME, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        l->SetComputeRootDescriptorTable(1, gpu_at(heap, at));
        l->Dispatch(postfx::groups(to.w, 8), postfx::groups(to.h, 8), 1);
        transition_level(l, r, level, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
    }
}

ID3D12Resource *FrameTextures::input_res(int in, int k, DXGI_FORMAT &f)
{
    f = view;
    if (in == IN_ORIGINAL)
        return work[k].orig;
    if (in == IN_PREVIOUS)
        return prev;
    if (in >= 0 && size_t(in) < pipeline.config.textures.size()) {
        f = pipeline.config.textures[size_t(in)].format;
        return tex_of(size_t(in), k);
    }
    f = DXGI_FORMAT_R8G8B8A8_UNORM;
    return nullptr;
}

void FrameTextures::upload(ID3D12GraphicsCommandList *l, PassTexture &t)
{
    D3D12_RESOURCE_DESC d = t.res->GetDesc();
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT rows = 0;
    UINT64 row_bytes = 0, total = 0;
    pipeline.gpu.device->GetCopyableFootprints(&d, 0, 1, 0, &fp, &rows, &row_bytes, &total);
    if (!t.upload && !(t.upload = pipeline.gpu.buffer(D3D12_HEAP_TYPE_UPLOAD, total, D3D12_RESOURCE_STATE_GENERIC_READ)))
        return;
    uint8_t *mapped = nullptr;
    if (FAILED(t.upload->Map(0, nullptr, reinterpret_cast<void **>(&mapped))))
        return;
    for (UINT y = 0; y < t.height; y++)
        memcpy(mapped + fp.Offset + size_t(y) * fp.Footprint.RowPitch, t.pixels.data() + size_t(y) * t.width * 4,
               size_t(t.width) * 4);
    t.upload->Unmap(0, nullptr);
    D3D12_TEXTURE_COPY_LOCATION to{}, from{};
    to.pResource = t.res;
    to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    from.pResource = t.upload;
    from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    from.PlacedFootprint = fp;
    transition(l, t.res, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    transition(l, t.res, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
    t.uploaded = true;
    t.pixels.clear();
    t.pixels.shrink_to_fit();
    pipeline.retire({t.upload});
    t.upload = nullptr;
}

bool FrameTextures::fit_frame()
{
    UINT w = UINT(desc.Width), h = desc.Height;
    for (size_t i = pipeline.config.legacy_textures; i < pipeline.config.textures.size(); i++) {
        PassTexture &t = pipeline.config.textures[i];
        postfx::Size sz = postfx::texture_size(t.scale, t.width, t.height, w, h);
        t.w = sz.w;
        t.h = sz.h;
        if (t.shared() && !t.res) {
            t.res = pipeline.gpu.texture(t.w, t.h, t.format, t.file.empty(), t.levels());
            t.clear = t.file.empty();
            if (!t.res)
                return false;
        }
    }
    for (auto &wk : work) {
        if (!wk.orig)
            continue;
        for (size_t i = 0; i < pipeline.config.textures.size(); i++)
            if (!pipeline.config.textures[i].shared() && !wk.tex[i] &&
                !(wk.tex[i] =
                      pipeline.gpu.texture(pipeline.config.textures[i].w, pipeline.config.textures[i].h,
                                           pipeline.config.textures[i].format, true, pipeline.config.textures[i].levels())))
                return false;
        wk.clear = true;
    }
    if (pipeline.config.uses_previous && !prev && !(prev = pipeline.gpu.texture(w, h, view, false)))
        return false;
    pipeline.retire({heap, cpu});
    heap = cpu = nullptr;
    textures_made++;
    return make_heaps();
}

}
