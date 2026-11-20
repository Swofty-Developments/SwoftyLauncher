#pragma once
#include <windows.h>

#include "core/hud/hud_drawlist.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <atomic>
#include <mutex>
#include <utility>
#include <vector>

namespace hud::overlay {

struct Pending {
    std::vector<hud::drawlist::Vertex> verts;
    std::vector<hud::drawlist::Batch> batches;
    IDXGISwapChain3 *sc = nullptr;
    UINT color_space = 0, width = 0, height = 0;
    ULONGLONG built = 0;
};

struct Presenting {
    ID3D12Resource *res;
    D3D12_RESOURCE_STATES state;
};

constexpr size_t BACK_BUFFERS_MAX = 16, PRESENTING_MAX = 64;

// While frame generation owns the swapchain, the overlay draws into the game's back buffer before Present instead:
// the frame built at Present waits until the game submits the list that moves its next back buffer to the present
// state, and is recorded after that list.
class BeforeFrameGen {
public:
    void subscribe();
    void keep(IDXGISwapChain3 *sc, const hud::drawlist::Frame &frame, const DXGI_SWAP_CHAIN_DESC &desc, UINT color_space,
              ULONGLONG now);
    void drop();
    void set_active(bool on) { active.store(on); }
    void on_setname(ID3D12Object *o, LPCWSTR name);
    bool on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers);
    void on_exec(ID3D12CommandQueue *, UINT n, ID3D12CommandList *const *lists, render::d3d12::ExecPlan &plan);

private:
    void draw(ID3D12CommandQueue *q, ID3D12Resource *res, D3D12_RESOURCE_STATES state);

    std::mutex lock;
    Pending pending;
    std::vector<ID3D12Resource *> back_buffers;
    std::vector<std::pair<ID3D12GraphicsCommandList *, Presenting>> presenting;
    std::atomic<bool> active{false}, pending_ready{false};
    std::atomic<size_t> back_count{0}, presenting_count{0};
    bool told_queue = false, told_shape = false, told_device = false;
};

}
