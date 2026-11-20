#include "core/hud/overlay/capture.hpp"

#include "core/hud/hud_png.hpp"
#include "core/hud/overlay/renderer.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstdlib>
#include <vector>

namespace hud::overlay {

namespace {

std::string env(const char *name)
{
    char v[1024] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n && n < sizeof v ? std::string(v, n) : std::string();
}

}

void FrameCapture::setup()
{
    std::string dir = env("SWOFTY_FRAME_CAPTURE");
    if (dir.empty())
        return;
    std::string every = env("SWOFTY_FRAME_CAPTURE_EVERY");
    double s = every.empty() ? 10 : atof(every.c_str());
    every_ms = ULONGLONG((s > 0.5 ? s : 0.5) * 1000);
    std::replace(dir.begin(), dir.end(), '/', '\\');
    while (!dir.empty() && dir.back() == '\\')
        dir.pop_back();
    this->dir = runtime::wide(dir);
    for (size_t i = this->dir.find(L'\\', 3); i != std::wstring::npos; i = this->dir.find(L'\\', i + 1))
        CreateDirectoryW(this->dir.substr(0, i).c_str(), nullptr);
    CreateDirectoryW(this->dir.c_str(), nullptr);
    on = true;
    swofty::log("[capture] frame capture on: the composed frame every %.1f s to %s", double(every_ms) / 1000, dir.c_str());
}

DWORD FrameCapture::write()
{
    ULONGLONG t0 = GetTickCount64();
    uint8_t *mapped = nullptr;
    D3D12_RANGE all{0, SIZE_T(size)};
    std::string why;
    if (FAILED(buf->Map(0, &all, reinterpret_cast<void **>(&mapped))) || !mapped) {
        why = "mapping the readback buffer failed";
    } else {
        std::vector<uint8_t> rgb;
        bool ok = hud::png::to_rgb(hud::png::fmt_of(int(fmt)), mapped + fp.Offset, fp.Footprint.RowPitch, width, height, rgb);
        D3D12_RANGE none{0, 0};
        buf->Unmap(0, &none);
        if (!ok) {
            why = "the back buffer format is not one the capture converts";
        } else {
            std::string png = hud::png::encode_rgb(rgb.data(), width, height);
            wchar_t name[64];
            _snwprintf(name, 64, L"\\frame-%04u.png", count);
            std::wstring path = writing.empty() ? dir + name : writing;
            if (!runtime::write_file(path, png))
                why = "writing " + runtime::narrow(path) + " failed";
            else
                swofty::log("[capture] wrote %s (%ux%u, format %d, %zu KB, %llu ms off the render thread)",
                            runtime::narrow(path).c_str(), width, height, int(fmt), png.size() / 1024,
                            static_cast<unsigned long long>(GetTickCount64() - t0));
        }
    }
    if (!why.empty())
        swofty::log("[capture] frame %u: %s", count, why.c_str());
    if (!writing.empty()) {
        std::lock_guard<std::mutex> g(request_lock);
        writing.clear();
        request_state = why.empty() ? 1 : -1;
        request_why = why;
    }
    busy.store(false);
    return 0;
}

void FrameCapture::step(Renderer &r, IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const DXGI_SWAP_CHAIN_DESC &desc, ULONGLONG now)
{
    if (failed || busy.load())
        return;
    if (pending) {
        if (r.fence->GetCompletedValue() < done_at)
            return;
        pending = false;
        busy.store(true);
        HANDLE t = CreateThread(nullptr, 0, write_thread, this, 0, nullptr);
        if (t)
            CloseHandle(t);
        else
            busy.store(false);
        return;
    }
    bool requested;
    {
        std::lock_guard<std::mutex> g(request_lock);
        requested = !request.empty();
    }
    if (!requested && (dir.empty() || (last && now - last < every_ms)))
        return;
    if (requested) {
        std::lock_guard<std::mutex> g(request_lock);
        writing = request;
        request.clear();
    }
    if (hud::png::fmt_of(int(desc.BufferDesc.Format)) == hud::png::Fmt::unknown) {
        failed = true;
        swofty::log("[capture] back buffer format %d is not one the capture converts; capture off", int(desc.BufferDesc.Format));
        return;
    }
    ID3D12Device *dev = r.dev;
    if (!alloc && (FAILED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) ||
                   FAILED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) ||
                   FAILED(list->Close()))) {
        failed = true;
        swofty::log("[capture] making the capture command list failed; capture off");
        return;
    }
    ID3D12Resource *bb = nullptr;
    if (FAILED(sc->GetBuffer(sc->GetCurrentBackBufferIndex(), IID_PPV_ARGS(&bb))) || !bb)
        return;
    D3D12_RESOURCE_DESC rd = bb->GetDesc();
    UINT64 total = 0;
    dev->GetCopyableFootprints(&rd, 0, 1, 0, &fp, nullptr, nullptr, &total);
    if (!buf || size < total) {
        release(buf);
        D3D12_HEAP_PROPERTIES hp{};
        hp.Type = D3D12_HEAP_TYPE_READBACK;
        D3D12_RESOURCE_DESC bd{};
        bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
        bd.Width = total;
        bd.Height = 1;
        bd.DepthOrArraySize = 1;
        bd.MipLevels = 1;
        bd.Format = DXGI_FORMAT_UNKNOWN;
        bd.SampleDesc.Count = 1;
        bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        if (FAILED(dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
                                                IID_PPV_ARGS(&buf)))) {
            bb->Release();
            failed = true;
            swofty::log("[capture] making a %llu-byte readback buffer failed; capture off",
                        static_cast<unsigned long long>(total));
            return;
        }
        size = total;
    }
    if (FAILED(alloc->Reset()) || FAILED(list->Reset(alloc, nullptr))) {
        bb->Release();
        failed = true;
        return;
    }
    D3D12_RESOURCE_BARRIER b{};
    b.Transition.pResource = bb;
    b.Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
    b.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    list->ResourceBarrier(1, &b);
    D3D12_TEXTURE_COPY_LOCATION dst{}, src{};
    dst.pResource = buf;
    dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    dst.PlacedFootprint = fp;
    src.pResource = bb;
    src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    src.SubresourceIndex = 0;
    list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    std::swap(b.Transition.StateBefore, b.Transition.StateAfter);
    list->ResourceBarrier(1, &b);
    if (FAILED(list->Close())) {
        bb->Release();
        failed = true;
        return;
    }
    ID3D12CommandList *lists[] = {list};
    q->ExecuteCommandLists(1, lists);
    q->Signal(r.fence, ++r.fence_val);
    bb->Release();
    done_at = r.fence_val;
    pending = true;
    last = now;
    count++;
    width = desc.BufferDesc.Width;
    height = desc.BufferDesc.Height;
    fmt = desc.BufferDesc.Format;
}

bool FrameCapture::request_frame(const std::wstring &path)
{
    std::lock_guard<std::mutex> g(request_lock);
    if (!request.empty() || !writing.empty())
        return false;
    request = path;
    request_state = 0;
    request_why.clear();
    on = true;
    return true;
}

int FrameCapture::poll_request(std::string *why)
{
    std::lock_guard<std::mutex> g(request_lock);
    if (failed) {
        request.clear();
        if (why)
            *why = "frame capture is off (see the [capture] line in loader.log)";
        return -1;
    }
    if (request_state < 0 && why)
        *why = request_why;
    return request_state;
}

}
