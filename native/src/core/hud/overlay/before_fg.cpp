#include "core/hud/overlay/before_fg.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/hud/arm_policy.hpp"
#include "core/hud/overlay/overlay.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cwchar>

namespace hud::overlay {

namespace {

void hook_setname(ID3D12Object *o, LPCWSTR name) { overlay.before_fg.on_setname(o, name); }

bool hook_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    return overlay.before_fg.on_barrier(list, n, barriers);
}

void hook_exec(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists, render::d3d12::ExecPlan &plan)
{
    overlay.before_fg.on_exec(q, n, lists, plan);
}

}

void BeforeFrameGen::subscribe()
{
    render::d3d12::on_setname(hook_setname);
    render::d3d12::on_barrier(hook_barrier);
    render::d3d12::on_exec(hook_exec);
}

void BeforeFrameGen::drop()
{
    if (!pending_ready.load(std::memory_order_relaxed))
        return;
    std::lock_guard<std::mutex> g(lock);
    pending_ready.store(false);
}

void BeforeFrameGen::keep(IDXGISwapChain3 *sc, const hud::drawlist::Frame &frame, const DXGI_SWAP_CHAIN_DESC &desc,
                          UINT color_space, ULONGLONG now)
{
    std::lock_guard<std::mutex> g(lock);
    if (frame.verts.empty()) {
        pending_ready.store(false);
        return;
    }
    pending.verts = frame.verts;
    pending.batches = frame.batches;
    pending.sc = sc;
    pending.color_space = color_space;
    pending.width = desc.BufferDesc.Width;
    pending.height = desc.BufferDesc.Height;
    pending.built = now;
    pending_ready.store(true);
}

void BeforeFrameGen::draw(ID3D12CommandQueue *q, ID3D12Resource *res, D3D12_RESOURCE_STATES state)
{
    static const uint32_t probe = perf::probe("core", "overlay exec draw");
    perf::Scope timed(probe);
    Pending frame;
    {
        std::lock_guard<std::mutex> g(lock);
        if (!pending_ready.exchange(false))
            return;
        frame = std::move(pending);
    }
    ULONGLONG now = GetTickCount64();
    if (overlay.off.load() || overlay.renderer.failed || !hud::arm_policy::pending_fresh(now, frame.built))
        return;
    D3D12_COMMAND_QUEUE_DESC qd = q->GetDesc();
    if (qd.Type != D3D12_COMMAND_LIST_TYPE_DIRECT) {
        if (!told_queue) {
            told_queue = true;
            swofty::log("[overlay] the game's back buffer is presented on queue %p of type %d, not a direct queue; the "
                        "overlay does not draw into it",
                        static_cast<void *>(q), int(qd.Type));
        }
        return;
    }
    D3D12_RESOURCE_DESC d = res->GetDesc();
    if (d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || !(d.Flags & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET) ||
        !hud::arm_policy::target_fits(frame.width, frame.height, d.Width, d.Height)) {
        if (!told_shape) {
            told_shape = true;
            swofty::log("[overlay] the game's back buffer %p (%llux%u, format %d, flags 0x%x) is not a render target "
                        "of the overlay's %ux%u; the overlay does not draw into it",
                        static_cast<void *>(res), static_cast<unsigned long long>(d.Width), d.Height, int(d.Format),
                        unsigned(d.Flags), frame.width, frame.height);
        }
        return;
    }
    ID3D12Device *dev = nullptr;
    if (FAILED(res->GetDevice(IID_PPV_ARGS(&dev))) || !dev)
        return;
    bool ready;
    {
        std::lock_guard<std::mutex> g(overlay.render_lock);
        if (!overlay.renderer.ready) {
            if (!overlay.renderer.init(q)) {
                overlay.renderer.failed = true;
                overlay.renderer.shutdown();
                swofty::log("[overlay] renderer init failed; the overlay stays armed but draws nothing");
            } else {
                overlay.live_dev.store(overlay.renderer.dev);
            }
        }
        ready = overlay.renderer.ready && overlay.renderer.dev == dev && overlay.renderer.same_device(q) &&
                overlay.renderer.dev->GetDeviceRemovedReason() == S_OK;
    }
    dev->Release();
    if (!ready) {
        if (!overlay.renderer.failed && !told_device) {
            told_device = true;
            swofty::log("[overlay] the game's back buffer %p or its queue %p belongs to another device than the "
                        "overlay renderer's; the overlay does not draw into it",
                        static_cast<void *>(res), static_cast<void *>(q));
        }
        return;
    }
    Renderer::Target t{res, DXGI_FORMAT(hud::arm_policy::renderable_format(int(d.Format))), frame.width, frame.height, state};
    overlay.draw_target(frame.sc, q, t, frame.verts, frame.batches, frame.color_space, now);
}

void BeforeFrameGen::on_setname(ID3D12Object *o, LPCWSTR name)
{
    if (!name || !overlay.armed.load(std::memory_order_relaxed))
        return;
    bool back = wcscmp(name, L"BackBuffer") == 0;
    if (!back && !back_count.load(std::memory_order_relaxed))
        return;
    auto *res = reinterpret_cast<ID3D12Resource *>(o);
    std::lock_guard<std::mutex> g(lock);
    back_buffers.erase(std::remove(back_buffers.begin(), back_buffers.end(), res), back_buffers.end());
    if (back) {
        if (back_buffers.size() >= BACK_BUFFERS_MAX)
            back_buffers.erase(back_buffers.begin());
        back_buffers.push_back(res);
    }
    back_count.store(back_buffers.size());
}

bool BeforeFrameGen::on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    if (!barriers || !active.load(std::memory_order_relaxed) || !back_count.load(std::memory_order_relaxed))
        return false;
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION ||
            !hud::arm_policy::presents_back_buffer(unsigned(b.Transition.StateBefore), unsigned(b.Transition.StateAfter),
                                                   unsigned(b.Flags)))
            continue;
        std::lock_guard<std::mutex> g(lock);
        if (std::find(back_buffers.begin(), back_buffers.end(), b.Transition.pResource) == back_buffers.end())
            continue;
        Presenting p{b.Transition.pResource,
                     D3D12_RESOURCE_STATES(hud::arm_policy::state_after_draw(unsigned(b.Transition.StateAfter)))};
        auto it = std::find_if(presenting.begin(), presenting.end(), [&](const auto &e) { return e.first == list; });
        if (it != presenting.end())
            it->second = p;
        else if (presenting.size() < PRESENTING_MAX)
            presenting.push_back({list, p});
        presenting_count.store(presenting.size());
    }
    return false;
}

void BeforeFrameGen::on_exec(ID3D12CommandQueue *, UINT n, ID3D12CommandList *const *lists, render::d3d12::ExecPlan &plan)
{
    if (!lists || !n || !presenting_count.load(std::memory_order_relaxed))
        return;
    ID3D12Resource *res = nullptr;
    D3D12_RESOURCE_STATES state{};
    UINT at = n;
    {
        std::lock_guard<std::mutex> g(lock);
        for (UINT i = 0; i < n; i++) {
            auto it = std::find_if(presenting.begin(), presenting.end(),
                                   [&](const auto &e) { return static_cast<ID3D12CommandList *>(e.first) == lists[i]; });
            if (it == presenting.end())
                continue;
            at = i;
            res = it->second.res;
            state = it->second.state;
            presenting.erase(it);
        }
        presenting_count.store(presenting.size());
    }
    if (at == n || !active.load() || !pending_ready.load())
        return;
    plan.after(at, [this, res, state](ID3D12CommandQueue *q) { draw(q, res, state); });
}

}
