#include "core/hud/hardware_settings.hpp"
#include "core/render/com.hpp"
#include "core/render/d3d12/filled_texture.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/grade_math.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <d3d12.h>

#include <cstdlib>
#include <cwchar>
#include <mutex>
#include <string>
#include <vector>

// Shader looks. The game maps scene colour to display colour through a 64x64x64 RGBA16F LUT,
// eaid://ego_package/aces_rrt_odt_lut.image, which a compute pass rewrites every frame before the
// passes that sample it; the HUD draws after them. This module finds that LUT by its debug name,
// reads the game's own copy back, runs every entry through the active look (grade_math.hpp) and,
// whenever the game transitions the LUT from written to read, copies the graded copy over it. It
// only adds copies and barriers to the game's command lists, so no pipeline state changes. Every
// two seconds it reads the game's LUT again and regrades when it changed, as after a brightness
// change; while HDR is on in the game's settings it leaves the LUT alone, as looks are SDR.

namespace render::grade {

namespace {

constexpr wchar_t TONEMAP_NAME[] = L"eaid://ego_package/aces_rrt_odt_lut.image";
constexpr ULONGLONG REFRESH_MS = 2000, LAND_MS = 1000;

struct Tonemap {
    ID3D12Resource *res;
    D3D12_RESOURCE_DESC desc;
};

using com::release;

class Grader {
public:
    void setup();
    void on_setname(ID3D12Object *o, LPCWSTR name);
    bool on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers);

private:
    bool hdr_on();
    bool make_readback(ID3D12GraphicsCommandList *list, const D3D12_RESOURCE_DESC &desc);
    void take_readback(ID3D12GraphicsCommandList *list, const Tonemap &t);
    Tonemap *tonemap_of(ID3D12Resource *res);
    void forget(ID3D12Resource *res);

    std::recursive_mutex lock;
    std::vector<Tonemap> tonemaps;
    grade::Lut look;
    float strength = 1;
    std::string addon;
    std::wstring settings_path;

    // The graded copy, a copy source for good. Copies it replaced stay alive, as lists the GPU has not
    // run yet may still read them.
    ID3D12Resource *ours = nullptr;
    std::vector<ID3D12Resource *> retired;
    std::vector<uint8_t> source;
    ID3D12Resource *readback = nullptr;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
    UINT rows = 0;
    ULONGLONG read_at = 0, next_read = 0;
    bool hdr = false, failed = false;
    unsigned copies = 0;
};

// Not constinit: std::recursive_mutex's constructor is not constexpr.
Grader grader;

// The game's HDR switch, `<hdr mode="0">` in its hardware settings; off when the file is missing.
bool Grader::hdr_on()
{
    std::string xml = runtime::read_file(settings_path, size_t(1) << 20);
    size_t at = xml.find("<hdr mode=\"");
    return at != std::string::npos && atoi(xml.c_str() + at + 11) != 0;
}

// A texture shaped like the game's LUT, without its UAV flag, filled from tight rows through an
// upload buffer on a queue of our own.
ID3D12Resource *upload(ID3D12Device *dev, D3D12_RESOURCE_DESC desc, const std::vector<uint8_t> &texels)
{
    desc.Flags = D3D12_RESOURCE_FLAG_NONE;
    ID3D12Resource *tex = nullptr, *up = nullptr;
    ID3D12CommandQueue *q = nullptr;
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12Fence *fence = nullptr;
    bool ok = false;
    do {
        if (FAILED(render::d3d12::create_filled_texture(dev, desc, D3D12_RESOURCE_STATE_COPY_DEST, &tex)))
            break;
        D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
        UINT n = 0;
        UINT64 row_bytes = 0, total = 0;
        dev->GetCopyableFootprints(&desc, 0, 1, 0, &fp, &n, &row_bytes, &total);
        D3D12_HEAP_PROPERTIES uh{};
        uh.Type = D3D12_HEAP_TYPE_UPLOAD;
        D3D12_RESOURCE_DESC bd{};
        bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
        bd.Width = total;
        bd.Height = 1;
        bd.DepthOrArraySize = 1;
        bd.MipLevels = 1;
        bd.SampleDesc.Count = 1;
        bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        if (FAILED(dev->CreateCommittedResource(&uh, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                                                IID_PPV_ARGS(&up))))
            break;
        uint8_t *mapped = nullptr;
        D3D12_RANGE none{0, 0};
        if (FAILED(up->Map(0, &none, reinterpret_cast<void **>(&mapped))))
            break;
        const size_t tight = size_t(row_bytes);
        for (UINT z = 0; z < desc.DepthOrArraySize; z++)
            for (UINT y = 0; y < desc.Height; y++)
                memcpy(mapped + fp.Offset + (size_t(z) * n + y) * fp.Footprint.RowPitch,
                       texels.data() + (size_t(z) * desc.Height + y) * tight, tight);
        up->Unmap(0, nullptr);
        D3D12_COMMAND_QUEUE_DESC qd{};
        qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        if (FAILED(dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&q))) ||
            FAILED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) ||
            FAILED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) ||
            FAILED(dev->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence))))
            break;
        D3D12_TEXTURE_COPY_LOCATION dst{}, src{};
        dst.pResource = tex;
        dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        src.pResource = up;
        src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        src.PlacedFootprint = fp;
        list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
        D3D12_RESOURCE_BARRIER b{};
        b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        b.Transition.pResource = tex;
        b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        b.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
        b.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
        list->ResourceBarrier(1, &b);
        if (FAILED(list->Close()))
            break;
        ID3D12CommandList *lists[] = {list};
        q->ExecuteCommandLists(1, lists);
        HANDLE done = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!done || FAILED(q->Signal(fence, 1)) || FAILED(fence->SetEventOnCompletion(1, done))) {
            if (done)
                CloseHandle(done);
            break;
        }
        WaitForSingleObject(done, 5000);
        CloseHandle(done);
        ok = fence->GetCompletedValue() == 1;
    } while (false);
    release(fence);
    release(list);
    release(alloc);
    release(q);
    release(up);
    if (!ok)
        release(tex);
    return ok ? tex : nullptr;
}

bool Grader::make_readback(ID3D12GraphicsCommandList *list, const D3D12_RESOURCE_DESC &desc)
{
    ID3D12Device *dev = nullptr;
    if (FAILED(list->GetDevice(IID_PPV_ARGS(&dev))))
        return false;
    UINT64 row_bytes = 0, total = 0;
    dev->GetCopyableFootprints(&desc, 0, 1, 0, &footprint, &rows, &row_bytes, &total);
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_READBACK;
    D3D12_RESOURCE_DESC bd{};
    bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    bd.Width = total;
    bd.Height = 1;
    bd.DepthOrArraySize = 1;
    bd.MipLevels = 1;
    bd.SampleDesc.Count = 1;
    bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    HRESULT hr = dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
                                              IID_PPV_ARGS(&readback));
    dev->Release();
    return SUCCEEDED(hr);
}

// Once the last readback has had time to land, grades the game's LUT when it differs from the one
// the graded copy came from.
void Grader::take_readback(ID3D12GraphicsCommandList *list, const Tonemap &t)
{
    const size_t tight = size_t(t.desc.Width) * 8;
    std::vector<uint8_t> fresh(tight * t.desc.Height * t.desc.DepthOrArraySize);
    uint8_t *mapped = nullptr;
    D3D12_RANGE all{0, SIZE_T(footprint.Offset + SIZE_T(rows) * footprint.Footprint.RowPitch * t.desc.DepthOrArraySize)};
    if (FAILED(readback->Map(0, &all, reinterpret_cast<void **>(&mapped))))
        return;
    for (UINT z = 0; z < t.desc.DepthOrArraySize; z++)
        for (UINT y = 0; y < t.desc.Height; y++)
            memcpy(fresh.data() + (size_t(z) * t.desc.Height + y) * tight,
                   mapped + footprint.Offset + (size_t(z) * rows + y) * footprint.Footprint.RowPitch, tight);
    D3D12_RANGE none{0, 0};
    readback->Unmap(0, &none);
    if (fresh == source)
        return;
    std::vector<uint8_t> graded(fresh.size());
    float peak = 0;
    for (size_t i = 0; i < fresh.size(); i += 8) {
        uint16_t h[4];
        memcpy(h, fresh.data() + i, 8);
        float rgb[3] = {grade::unpack_half(h[0]), grade::unpack_half(h[1]), grade::unpack_half(h[2])};
        peak = std::max({peak, rgb[0], rgb[1], rgb[2]});
        grade::apply(look, strength, rgb);
        for (int c = 0; c < 3; c++)
            h[c] = grade::pack_half(rgb[c]);
        memcpy(graded.data() + i, h, 8);
    }
    if (!(peak > 0))
        return;
    ID3D12Device *dev = nullptr;
    ID3D12Resource *made = nullptr;
    if (SUCCEEDED(list->GetDevice(IID_PPV_ARGS(&dev)))) {
        made = upload(dev, t.desc, graded);
        dev->Release();
    }
    if (!made) {
        failed = true;
        swofty::log("[shaders] could not upload the graded tonemap LUT; the game's colours stay");
        return;
    }
    bool first = !ours;
    if (ours)
        retired.push_back(ours);
    ours = made;
    source = std::move(fresh);
    swofty::log("[shaders] %s the game's tonemap LUT (brightest %.3f) with %s's look", first ? "graded" : "regraded",
                double(peak), addon.c_str());
}

Tonemap *Grader::tonemap_of(ID3D12Resource *res)
{
    for (auto &t : tonemaps)
        if (t.res == res)
            return &t;
    return nullptr;
}

void Grader::forget(ID3D12Resource *res)
{
    for (size_t i = 0; i < tonemaps.size(); i++)
        if (tonemaps[i].res == res) {
            tonemaps.erase(tonemaps.begin() + ptrdiff_t(i));
            return;
        }
}

bool same_shape(const D3D12_RESOURCE_DESC &a, const D3D12_RESOURCE_DESC &b)
{
    return a.Dimension == b.Dimension && a.Width == b.Width && a.Height == b.Height && a.DepthOrArraySize == b.DepthOrArraySize &&
           a.Format == b.Format && a.MipLevels == b.MipLevels;
}

// The game frees resources and reuses their addresses, so any SetName drops the old entry first.
void Grader::on_setname(ID3D12Object *o, LPCWSTR name)
{
    if (name) {
        auto *res = reinterpret_cast<ID3D12Resource *>(o);
        std::lock_guard<std::recursive_mutex> g(lock);
        forget(res);
        if (wcscmp(name, TONEMAP_NAME) == 0) {
            D3D12_RESOURCE_DESC d = res->GetDesc();
            if (d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D && d.MipLevels == 1 &&
                d.Format == DXGI_FORMAT_R16G16B16A16_FLOAT)
                tonemaps.push_back({res, d});
            else
                swofty::log("[shaders] the game's tonemap LUT has format %d, not RGBA16F; left alone", int(d.Format));
        }
    }
}

bool written_to_read(const D3D12_RESOURCE_BARRIER &b)
{
    return b.Type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION && b.Flags == D3D12_RESOURCE_BARRIER_FLAG_NONE &&
           (b.Transition.StateBefore & (D3D12_RESOURCE_STATE_UNORDERED_ACCESS | D3D12_RESOURCE_STATE_COPY_DEST)) &&
           (b.Transition.StateAfter &
            (D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE));
}

bool Grader::on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    std::vector<D3D12_RESOURCE_BARRIER> changed;
    {
        std::lock_guard<std::recursive_mutex> g(lock);
        for (UINT i = 0; barriers && i < n && !failed; i++) {
            const D3D12_RESOURCE_BARRIER &b = barriers[i];
            if (!written_to_read(b))
                continue;
            Tonemap *t = tonemap_of(b.Transition.pResource);
            if (!t)
                continue;
            if (!same_shape(b.Transition.pResource->GetDesc(), t->desc)) {
                forget(t->res);
                continue;
            }
            ULONGLONG now = GetTickCount64();
            if (read_at && now - read_at >= LAND_MS) {
                take_readback(list, *t);
                read_at = 0;
            }
            bool reading = false;
            if (!read_at && now >= next_read) {
                bool was = hdr;
                hdr = hdr_on();
                if (hdr != was || !next_read)
                    swofty::log(hdr ? "[shaders] HDR is on in the game's settings; %s's look waits for SDR"
                                    : "[shaders] SDR: %s's look applies",
                                addon.c_str());
                next_read = now + REFRESH_MS;
                reading = !hdr && (readback || make_readback(list, t->desc));
            }
            bool replacing = ours && !hdr;
            if (!reading && !replacing)
                continue;
            if (changed.empty())
                changed.assign(barriers, barriers + n);
            D3D12_RESOURCE_STATES state = b.Transition.StateBefore;
            auto move = [&](D3D12_RESOURCE_STATES to) {
                if (state == to)
                    return;
                D3D12_RESOURCE_BARRIER step = b;
                step.Transition.StateBefore = state;
                step.Transition.StateAfter = to;
                render::d3d12::barrier_original(list, 1u, &step);
                state = to;
            };
            if (reading) {
                move(D3D12_RESOURCE_STATE_COPY_SOURCE);
                D3D12_TEXTURE_COPY_LOCATION dst{}, src{};
                dst.pResource = readback;
                dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
                dst.PlacedFootprint = footprint;
                src.pResource = t->res;
                src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
                list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
                read_at = now;
            }
            if (replacing) {
                move(D3D12_RESOURCE_STATE_COPY_DEST);
                list->CopyResource(t->res, ours);
                if (++copies == 1)
                    swofty::log("[shaders] the picture now goes through %s's look", addon.c_str());
            }
            changed[i].Transition.StateBefore = state;
        }
    }
    if (changed.empty())
        return false;
    render::d3d12::barrier_original(list, n, changed.data());
    return true;
}

void Grader::setup()
{
    if (!runtime::enabled("shaders", nullptr))
        return;
    const json::Value &s = runtime::config()["shader"];
    std::string rel = s["lut"].str();
    if (rel.empty()) {
        swofty::log("[shaders] the shaders feature is on, but assets.json names no look; the game's colours stay");
        return;
    }
    std::string why;
    if (!grade::unpack(runtime::read_file(runtime::under_swofty(rel)), look, why)) {
        swofty::log("[shaders] %s is unusable: %s; the game's colours stay", rel.c_str(), why.c_str());
        return;
    }
    const json::Value &st = s["strength"];
    strength = st.type == json::Value::Num ? std::clamp(float(st.n), 0.0f, 1.0f) : 1.0f;
    addon = s["addon"].str("an addon");
    settings_path = hud::hardware_settings::path();
    render::d3d12::on_setname([](ID3D12Object *o, LPCWSTR name) { grader.on_setname(o, name); });
    render::d3d12::on_barrier([](ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers) {
        return grader.on_barrier(list, n, barriers);
    });
    bool hooked = render::d3d12::arm_device();
    if (!hooked)
        swofty::log("[shaders] the core's D3D12 device hooks did not arm; the game's colours stay");
    if (hooked)
        swofty::log("[shaders] grading the picture with %s's look (%ux%ux%u) at %.0f%%; HDR setting read from %ls", addon.c_str(),
                    look.size, look.size, look.size, double(strength) * 100,
                    settings_path.empty() ? L"(no Documents folder)" : settings_path.c_str());
}

}

}

namespace runtime {

void grade_setup() { render::grade::grader.setup(); }

}
