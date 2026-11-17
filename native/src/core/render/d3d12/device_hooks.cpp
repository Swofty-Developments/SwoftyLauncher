#include "core/render/d3d12/device_hooks.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12/subscribers.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>

namespace d3d12hooks {

constinit QueueWatch queue_watch;

namespace {

safetyhook::InlineHook h_exec, h_barrier, h_setname, h_rtv, h_om;
constexpr size_t SETNAME_SLOT = 6, EXEC_SLOT = 10, BARRIER_SLOT = 26, RTV_SLOT = 20, OM_SLOT = 46;

void STDMETHODCALLTYPE exec_hook(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists)
{
    if (queue_watch.wanted())
        queue_watch.count(q);
    ExecPlan plan;
    for (int i = 0, k = subscribers.execs.n.load(); i < k; i++)
        subscribers.execs.fns[i](q, n, lists, plan);
    run_plan(plan, q, n, lists, [](ID3D12CommandQueue *queue, UINT count, ID3D12CommandList *const *first) {
        perf::Pause game;
        h_exec.call<void>(queue, count, first);
    });
}

void STDMETHODCALLTYPE barrier_hook(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *b)
{
    for (int i = 0, k = subscribers.barriers.n.load(); i < k; i++)
        if (subscribers.barriers.fns[i](list, n, b))
            return;
    h_barrier.call<void>(list, n, b);
}

void STDMETHODCALLTYPE rtv_hook(ID3D12Device *d, ID3D12Resource *r, const D3D12_RENDER_TARGET_VIEW_DESC *desc,
                                D3D12_CPU_DESCRIPTOR_HANDLE at)
{
    h_rtv.call<void>(d, r, desc, at);
    for (int i = 0, k = subscribers.rtvs.n.load(); i < k; i++)
        subscribers.rtvs.fns[i](r, at);
}

void STDMETHODCALLTYPE om_hook(ID3D12GraphicsCommandList *l, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtv,
                               BOOL single, const D3D12_CPU_DESCRIPTOR_HANDLE *dsv)
{
    for (int i = 0, k = subscribers.render_targets.n.load(); i < k; i++)
        subscribers.render_targets.fns[i](l, n, rtv, single);
    h_om.call<void>(l, n, rtv, single, dsv);
}

HRESULT STDMETHODCALLTYPE setname_hook(ID3D12Object *o, LPCWSTR name)
{
    for (int i = 0, k = subscribers.setnames.n.load(); i < k; i++)
        subscribers.setnames.fns[i](o, name);
    return h_setname.call<HRESULT>(o, name);
}

}

void QueueWatch::count(ID3D12CommandQueue *q)
{
    DWORD tid = GetCurrentThreadId();
    ULONGLONG now = GetTickCount64();
    auto same = [&](const hud::guard::ExecSeen &e) { return e.queue == q; };
    {
        std::lock_guard<std::mutex> g(lock);
        auto it = std::find_if(seen.begin(), seen.end(), same);
        if (it != seen.end()) {
            if (it->type == hud::guard::LIST_TYPE_DIRECT) {
                it->tid = tid;
                it->tick = now;
            }
            return;
        }
        if (seen.size() >= 8)
            return;
    }
    int type = int(q->GetDesc().Type);
    std::lock_guard<std::mutex> g(lock);
    if (seen.size() < 8 && std::none_of(seen.begin(), seen.end(), same))
        seen.push_back({q, type, tid, now});
}

bool QueueWatch::recent_direct(DWORD tid, hud::guard::ExecSeen &out)
{
    std::lock_guard<std::mutex> g(lock);
    const hud::guard::ExecSeen *e = hud::guard::present_queue_from_execs(seen, tid);
    if (e)
        out = *e;
    return e != nullptr;
}

bool exec_hooked()
{
    return bool(h_exec);
}

void exec_original(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists)
{
    if (h_exec && hud::guard::vtable_slot_is(q, EXEC_SLOT, h_exec.target()))
        h_exec.call<void>(q, n, lists);
    else
        q->ExecuteCommandLists(n, lists);
}

void barrier_original(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *b)
{
    if (h_barrier && hud::guard::vtable_slot_is(list, BARRIER_SLOT, h_barrier.target()))
        h_barrier.call<void>(list, n, b);
    else
        list->ResourceBarrier(n, b);
}

bool arm_device()
{
    static int state;
    if (state)
        return state > 0;
    state = -1;
    ID3D12Device *dev = probe_device();
    ID3D12Resource *buffer = nullptr;
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12CommandQueue *q = nullptr;
    bool ok = false;
    if (dev) {
        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_UPLOAD;
        D3D12_RESOURCE_DESC bd{};
        bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
        bd.Width = 256;
        bd.Height = 1;
        bd.DepthOrArraySize = 1;
        bd.MipLevels = 1;
        bd.SampleDesc.Count = 1;
        bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        D3D12_COMMAND_QUEUE_DESC qd{};
        qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        if (SUCCEEDED(dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ,
                                                   nullptr, IID_PPV_ARGS(&buffer))) &&
            SUCCEEDED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) &&
            SUCCEEDED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) &&
            SUCCEEDED(dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&q))))
            ok = install(h_setname, (*reinterpret_cast<void ***>(buffer))[SETNAME_SLOT],
                         reinterpret_cast<void *>(&setname_hook), "ID3D12Object::SetName") &&
                 install(h_barrier, (*reinterpret_cast<void ***>(list))[BARRIER_SLOT],
                         reinterpret_cast<void *>(&barrier_hook), "ID3D12GraphicsCommandList::ResourceBarrier") &&
                 install(h_exec, (*reinterpret_cast<void ***>(q))[EXEC_SLOT], reinterpret_cast<void *>(&exec_hook),
                         "ID3D12CommandQueue::ExecuteCommandLists");
        if (ok && install(h_rtv, (*reinterpret_cast<void ***>(dev))[RTV_SLOT], reinterpret_cast<void *>(&rtv_hook),
                          "ID3D12Device::CreateRenderTargetView") &&
            !install(h_om, (*reinterpret_cast<void ***>(list))[OM_SLOT], reinterpret_cast<void *>(&om_hook),
                     "ID3D12GraphicsCommandList::OMSetRenderTargets"))
            h_rtv = {};
        if (list)
            list->Close();
    }
    if (ok)
        swofty::log("[d3d12] device hooks armed: SetName (vt+0x30), ResourceBarrier (vt+0xd0), ExecuteCommandLists "
                    "(vt+0x50)%s",
                    h_om ? ", CreateRenderTargetView (vt+0xa0), OMSetRenderTargets (vt+0x170)" : "");
    release(q);
    release(list);
    release(alloc);
    release(buffer);
    release(dev);
    state = ok ? 1 : -1;
    return ok;
}

}
