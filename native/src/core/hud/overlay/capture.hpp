#pragma once
#include <windows.h>

#include <d3d12.h>
#include <dxgi1_6.h>

#include <atomic>
#include <mutex>
#include <string>

namespace hud::overlay {

struct Renderer;

// SWOFTY_FRAME_CAPTURE: the composed frame copied off the back buffer at Present every few seconds, or once on
// request, and written as a PNG off the render thread.
class FrameCapture {
public:
    void setup();
    void step(Renderer &r, IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const DXGI_SWAP_CHAIN_DESC &desc, ULONGLONG now);
    bool request_frame(const std::wstring &path);
    int poll_request(std::string *why);

    bool on = false;

private:
    DWORD write();
    static DWORD WINAPI write_thread(void *self) { return static_cast<FrameCapture *>(self)->write(); }

    std::wstring dir;
    ULONGLONG every_ms = 10000, last = 0;
    unsigned count = 0;
    bool pending = false;
    std::atomic<bool> busy{false};
    UINT64 done_at = 0;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT width = 0, height = 0;
    DXGI_FORMAT fmt = DXGI_FORMAT_UNKNOWN;
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12Resource *buf = nullptr;
    UINT64 size = 0;
    bool failed = false;
    // An on-demand frame (the test agent's capture): the next presented frame goes to request.
    std::mutex request_lock;
    std::wstring request, writing;
    int request_state = 0;
    std::string request_why;
};

}
