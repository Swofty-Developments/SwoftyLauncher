#include "core/render/postfx/capture.hpp"

#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace render::postfx {

namespace {

void write_ppm(const std::string &path, const uint8_t *src, UINT pitch, UINT w, UINT h, DXGI_FORMAT view)
{
    std::string out = "P6\n" + std::to_string(w) + " " + std::to_string(h) + "\n255\n";
    size_t at = out.size();
    out.resize(at + size_t(w) * h * 3);
    for (UINT y = 0; y < h; y++)
        for (UINT x = 0; x < w; x++) {
            uint32_t v;
            memcpy(&v, src + size_t(y) * pitch + size_t(x) * 4, 4);
            uint8_t *o = reinterpret_cast<uint8_t *>(&out[at + (size_t(y) * w + x) * 3]);
            if (view == DXGI_FORMAT_R10G10B10A2_UNORM) {
                o[0] = uint8_t((v & 1023) >> 2), o[1] = uint8_t(((v >> 10) & 1023) >> 2), o[2] = uint8_t(((v >> 20) & 1023) >> 2);
            } else {
                o[0] = uint8_t(v), o[1] = uint8_t(v >> 8), o[2] = uint8_t(v >> 16);
            }
        }
    runtime::write_file(runtime::wide(path), out);
}

// The mean colour and saturation of a picture read back to the CPU, over a grid of its middle.
void grid_mean(const uint8_t *src, UINT pitch, UINT w, UINT h, DXGI_FORMAT f, double out[4])
{
    double r = 0, g = 0, bl = 0, sat = 0;
    unsigned n = 0;
    for (UINT y = h / 8; y < h * 7 / 10; y += std::max(1u, h / 40))
        for (UINT x = w / 10; x < w * 9 / 10; x += std::max(1u, w / 64)) {
            uint32_t v;
            memcpy(&v, src + size_t(y) * pitch + size_t(x) * 4, 4);
            double c[3];
            if (f == DXGI_FORMAT_R10G10B10A2_UNORM)
                c[0] = (v & 1023) / 1023.0, c[1] = ((v >> 10) & 1023) / 1023.0, c[2] = ((v >> 20) & 1023) / 1023.0;
            else
                c[0] = (v & 255) / 255.0, c[1] = ((v >> 8) & 255) / 255.0, c[2] = ((v >> 16) & 255) / 255.0;
            double mx = std::max(c[0], std::max(c[1], c[2])), mn = std::min(c[0], std::min(c[1], c[2]));
            r += c[0], g += c[1], bl += c[2], sat += mx > 0 ? (mx - mn) / mx : 0;
            n++;
        }
    n = std::max(n, 1u);
    out[0] = r / n, out[1] = g / n, out[2] = bl / n, out[3] = sat / n;
}

void copy_to_readback(ID3D12GraphicsCommandList *l, ID3D12Resource *rb_res, const D3D12_PLACED_SUBRESOURCE_FOOTPRINT &footprint,
                      ID3D12Resource *back)
{
    D3D12_TEXTURE_COPY_LOCATION rb{}, from{};
    rb.pResource = rb_res;
    rb.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    rb.PlacedFootprint = footprint;
    from.pResource = back;
    from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    l->CopyTextureRegion(&rb, 0, 0, 0, &from, nullptr);
}

}

void PictureCapture::configure(const std::string &dir, const std::wstring &capture_file)
{
    dir_ = dir;
    file = capture_file;
}

void PictureCapture::remake(const GpuContext &gpu, const D3D12_RESOURCE_DESC &d, RetireQueue &retired, unsigned frame_count)
{
    if (dir_.empty() || pending)
        return;
    retired.retire({rb[0], rb[1]}, frame_count);
    UINT rows = 0;
    UINT64 row_bytes = 0, total = 0;
    gpu.device->GetCopyableFootprints(&d, 0, 1, 0, &fp, &rows, &row_bytes, &total);
    for (auto &r : rb)
        r = gpu.buffer(D3D12_HEAP_TYPE_READBACK, total, D3D12_RESOURCE_STATE_COPY_DEST);
}

void PictureCapture::land(const Slot *slots, const D3D12_RESOURCE_DESC &desc, DXGI_FORMAT view)
{
    if (!pending || slot_ < 0 || slots[slot_].pending || slots[slot_].fence->GetCompletedValue() < fence)
        return;
    pending = false;
    if (view != DXGI_FORMAT_R8G8B8A8_UNORM && view != DXGI_FORMAT_R10G10B10A2_UNORM)
        return;
    const char *which[2] = {"before", "after"};
    for (int i = 0; i < 2; i++) {
        uint8_t *mapped = nullptr;
        if (!rb[i] || FAILED(rb[i]->Map(0, nullptr, reinterpret_cast<void **>(&mapped))))
            continue;
        char name[64];
        snprintf(name, sizeof name, "\\postfx-%02u-%s.ppm", captures, which[i]);
        write_ppm(dir_ + name, mapped + fp.Offset, fp.Footprint.RowPitch, UINT(desc.Width), desc.Height, view);
        D3D12_RANGE none{0, 0};
        rb[i]->Unmap(0, &none);
    }
    swofty::log("[postfx] wrote the picture before and after the passes to %s\\postfx-%02u-{before,after}.ppm", dir_.c_str(),
                captures);
    captures++;
}

void PictureCapture::arm(ULONGLONG now, unsigned ran_total)
{
    if (!dir_.empty() && now - checked >= 1000) {
        checked = now;
        if (GetFileAttributesW(file.c_str()) != INVALID_FILE_ATTRIBUTES) {
            DeleteFileW(file.c_str());
            asked = true;
        }
    }
    now_ = !dir_.empty() && !pending && rb[0] && rb[1] && (asked || (captures < 6 && ran_total && now >= next));
}

void PictureCapture::copy(ID3D12GraphicsCommandList *l, ID3D12Resource *back, int which) const
{
    copy_to_readback(l, rb[which], fp, back);
}

void PictureCapture::ran(int slot, ULONGLONG now)
{
    if (!now_)
        return;
    pending = true;
    asked = false;
    slot_ = slot;
    next = now + 10000;
}

void PictureCapture::signalled(int slot, UINT64 fence_value)
{
    if (slot_ == slot)
        fence = fence_value;
}

void ClipRecorder::configure(const std::string &clip_dir, const std::wstring &clip_file)
{
    dir = clip_dir;
    file = clip_file;
}

bool ClipRecorder::busy() const
{
    return left || std::any_of(std::begin(ring), std::end(ring), [](const Entry &e) { return e.pending; });
}

ClipRecorder::Entry *ClipRecorder::begin(const GpuContext &gpu, const D3D12_RESOURCE_DESC &d, int k, DXGI_FORMAT view,
                                         RetireQueue &retired, unsigned frame_count)
{
    if (!left)
        return nullptr;
    if (view != DXGI_FORMAT_R8G8B8A8_UNORM && view != DXGI_FORMAT_R10G10B10A2_UNORM) {
        swofty::log("[shaders] clip: the picture has format %d, which the capture does not take; clip stopped", int(view));
        left = 0;
        return nullptr;
    }
    if (!ring[0].rb || w != d.Width || h != d.Height) {
        std::vector<IUnknown *> old;
        for (auto &e : ring) {
            old.push_back(e.rb);
            e = {};
        }
        retired.retire(std::move(old), frame_count);
        UINT rows = 0;
        UINT64 row_bytes = 0, total = 0;
        gpu.device->GetCopyableFootprints(&d, 0, 1, 0, &fp, &rows, &row_bytes, &total);
        for (auto &e : ring)
            if (!(e.rb = gpu.buffer(D3D12_HEAP_TYPE_READBACK, total, D3D12_RESOURCE_STATE_COPY_DEST))) {
                swofty::log("[shaders] clip: the readback buffers could not be made; clip stopped");
                left = 0;
                return nullptr;
            }
        w = UINT(d.Width);
        h = d.Height;
    }
    for (auto &e : ring)
        if (!e.pending) {
            e.pending = true;
            e.slot = k;
            e.land = frame_count + CLIP_DELAY;
            e.index = issued++;
            left--;
            view_ = view;
            return &e;
        }
    return nullptr;
}

void ClipRecorder::copy(ID3D12GraphicsCommandList *l, ID3D12Resource *back, const Entry &e) const
{
    copy_to_readback(l, e.rb, fp, back);
}

void ClipRecorder::write(const uint8_t *src, UINT pitch, unsigned index) const
{
    std::vector<uint8_t> rgb(size_t(w) * h * 3), out(size_t(CLIP_W) * CLIP_H * 3);
    for (UINT y = 0; y < h; y++)
        for (UINT x = 0; x < w; x++) {
            uint32_t v;
            memcpy(&v, src + size_t(y) * pitch + size_t(x) * 4, 4);
            uint8_t *o = &rgb[(size_t(y) * w + x) * 3];
            if (view_ == DXGI_FORMAT_R10G10B10A2_UNORM) {
                o[0] = uint8_t((v & 1023) >> 2), o[1] = uint8_t(((v >> 10) & 1023) >> 2), o[2] = uint8_t(((v >> 20) & 1023) >> 2);
            } else {
                o[0] = uint8_t(v), o[1] = uint8_t(v >> 8), o[2] = uint8_t(v >> 16);
            }
        }
    box_scale_rgb(rgb.data(), w, h, out.data(), CLIP_W, CLIP_H);
    std::string ppm = "P6\n" + std::to_string(CLIP_W) + " " + std::to_string(CLIP_H) + "\n255\n";
    ppm.append(reinterpret_cast<const char *>(out.data()), out.size());
    char name[48];
    snprintf(name, sizeof name, "\\clip-%08u.ppm", index);
    runtime::write_file(runtime::wide(dir + name), ppm);
}

void ClipRecorder::land(const Slot *slots, unsigned frame_count)
{
    for (auto &e : ring) {
        if (!e.pending || frame_count < e.land || !slots[e.slot].gpu_done())
            continue;
        e.pending = false;
        uint8_t *mapped = nullptr;
        if (!e.rb || FAILED(e.rb->Map(0, nullptr, reinterpret_cast<void **>(&mapped))))
            continue;
        write(mapped + fp.Offset, fp.Footprint.RowPitch, e.index);
        D3D12_RANGE none{0, 0};
        e.rb->Unmap(0, &none);
        written++;
    }
    if (running && !busy()) {
        running = false;
        swofty::log("[shaders] clip: wrote %u frame(s) to %s", written, dir.c_str());
    }
}

void ClipRecorder::start(unsigned frames)
{
    if (busy())
        return;
    CreateDirectoryW(runtime::wide(dir).c_str(), nullptr);
    left = frames ? std::min(frames, 6000u) : 240u;
    issued = written = 0;
    running = true;
    swofty::log("[shaders] clip: writing the next %u frame(s) that run passes to %s", left, dir.c_str());
}

void ClipRecorder::check_file()
{
    if (!dir.empty() && GetFileAttributesW(file.c_str()) != INVALID_FILE_ATTRIBUTES) {
        std::string text = runtime::read_file(file, 64);
        DeleteFileW(file.c_str());
        start(unsigned(std::strtoul(text.c_str(), nullptr, 10)));
    }
}

void BurstProbe::check_file(const std::wstring &swofty_root)
{
    std::wstring burst_file = swofty_root + L"postfx.burst";
    if (GetFileAttributesW(burst_file.c_str()) != INVALID_FILE_ATTRIBUTES && !left) {
        std::string text = runtime::read_file(burst_file, 64);
        DeleteFileW(burst_file.c_str());
        unsigned frames = unsigned(std::strtoul(text.c_str(), nullptr, 10));
        left = std::min(frames ? frames : 60u, 240u);
        swofty::log("[postfx] burst: reading back the next %u frame(s) as the game hands them to Present", left);
    }
}

void BurstProbe::record(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, const GpuContext &gpu,
                        const ResourceWatch &watched, unsigned frame_count)
{
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION || !(watched.bits(b.Transition.pResource) & W_BACK) ||
            b.Transition.StateBefore != D3D12_RESOURCE_STATE_RENDER_TARGET || b.Transition.StateAfter != 0)
            continue;
        ID3D12Resource *shown = b.Transition.pResource;
        Probe &p = probes[at % 256];
        if (p.pending)
            break;
        D3D12_RESOURCE_DESC d = shown->GetDesc();
        if (!p.rb) {
            D3D12_RESOURCE_DESC box = d;
            box.Width = w = UINT(d.Width) * 8 / 10;
            box.Height = h = d.Height * 6 / 10;
            UINT rows = 0;
            UINT64 row_bytes = 0, total = 0;
            gpu.device->GetCopyableFootprints(&box, 0, 1, 0, &fp, &rows, &row_bytes, &total);
            p.rb = gpu.buffer(D3D12_HEAP_TYPE_READBACK, total, D3D12_RESOURCE_STATE_COPY_DEST);
        }
        if (!p.rb)
            break;
        transition(list, shown, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_COPY_SOURCE);
        D3D12_TEXTURE_COPY_LOCATION to{}, from{};
        to.pResource = p.rb;
        to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        to.PlacedFootprint = fp;
        from.pResource = shown;
        from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        D3D12_BOX src{UINT(d.Width) / 10, d.Height / 8, 0, UINT(d.Width) / 10 + w, d.Height / 8 + h, 1};
        list->CopyTextureRegion(&to, 0, 0, 0, &from, &src);
        transition(list, shown, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET);
        p.pending = true;
        p.frame = frame_count;
        p.land = frame_count + 60;
        at++;
        left--;
        break;
    }
}

void BurstProbe::land(unsigned frame_count, DXGI_FORMAT view)
{
    for (auto &p : probes) {
        if (!p.pending || frame_count < p.land)
            continue;
        p.pending = false;
        uint8_t *mapped = nullptr;
        if (FAILED(p.rb->Map(0, nullptr, reinterpret_cast<void **>(&mapped))))
            continue;
        double m[4];
        grid_mean(mapped + fp.Offset, fp.Footprint.RowPitch, w, h, view, m);
        if (!dir.empty()) {
            char name[64];
            snprintf(name, sizeof name, "\\final-%08u.ppm", p.frame);
            write_ppm(dir + name, mapped + fp.Offset, fp.Footprint.RowPitch, w, h, view);
        }
        D3D12_RANGE none{0, 0};
        p.rb->Unmap(0, &none);
        swofty::log("[postfx] burst f=%u mean=%.4f,%.4f,%.4f sat=%.4f", p.frame, m[0], m[1], m[2], m[3]);
    }
}

}
