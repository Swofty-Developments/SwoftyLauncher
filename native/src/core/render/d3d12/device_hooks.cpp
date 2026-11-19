#include "core/render/d3d12/device_hooks.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12/subscribers.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>

namespace render::d3d12 {

constinit QueueWatch queue_watch;

namespace {

// The device set: the queue's ExecuteCommandLists, the list's ResourceBarrier, OMSetRenderTargets, CopyTextureRegion
// and CopyResource, and the device's SetName and CreateRenderTargetView.
struct DeviceHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook exec, barrier, setname, rtv, om, texture_copy, resource_copy;
};

DeviceHooks hooks;
constexpr size_t SETNAME_SLOT = 6, EXEC_SLOT = 10, BARRIER_SLOT = 26, RTV_SLOT = 20, OM_SLOT = 46;
constexpr size_t TEXTURE_COPY_SLOT = 16, RESOURCE_COPY_SLOT = 17;

void STDMETHODCALLTYPE exec_hook(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists)
{
    if (queue_watch.wanted())
        queue_watch.count(q);
    ExecPlan plan;
    for (int i = 0, k = subscribers.execs.n.load(); i < k; i++)
        subscribers.execs.fns[i](q, n, lists, plan);
    run_plan(plan, q, n, lists, [](ID3D12CommandQueue *queue, UINT count, ID3D12CommandList *const *first) {
        perf::Pause game;
        hooks.exec.call<void>(queue, count, first);
    });
}

void STDMETHODCALLTYPE barrier_hook(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *b)
{
    for (int i = 0, k = subscribers.barrier_observers.n.load(); i < k; i++)
        subscribers.barrier_observers.fns[i](list, n, b);
    for (int i = 0, k = subscribers.barriers.n.load(); i < k; i++)
        if (subscribers.barriers.fns[i](list, n, b))
            return;
    hooks.barrier.call<void>(list, n, b);
}

void STDMETHODCALLTYPE rtv_hook(ID3D12Device *d, ID3D12Resource *r, const D3D12_RENDER_TARGET_VIEW_DESC *desc,
                                D3D12_CPU_DESCRIPTOR_HANDLE at)
{
    hooks.rtv.call<void>(d, r, desc, at);
    for (int i = 0, k = subscribers.rtvs.n.load(); i < k; i++)
        subscribers.rtvs.fns[i](r, at);
}

void STDMETHODCALLTYPE om_hook(ID3D12GraphicsCommandList *l, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtv, BOOL single,
                               const D3D12_CPU_DESCRIPTOR_HANDLE *dsv)
{
    for (int i = 0, k = subscribers.render_targets.n.load(); i < k; i++)
        subscribers.render_targets.fns[i](l, n, rtv, single);
    hooks.om.call<void>(l, n, rtv, single, dsv);
}

HRESULT STDMETHODCALLTYPE setname_hook(ID3D12Object *o, LPCWSTR name)
{
    for (int i = 0, k = subscribers.setnames.n.load(); i < k; i++)
        subscribers.setnames.fns[i](o, name);
    return hooks.setname.call<HRESULT>(o, name);
}

void STDMETHODCALLTYPE texture_copy_hook(ID3D12GraphicsCommandList *list, const D3D12_TEXTURE_COPY_LOCATION *dst, UINT x, UINT y,
                                         UINT z, const D3D12_TEXTURE_COPY_LOCATION *src, const D3D12_BOX *box)
{
    hooks.texture_copy.call<void>(list, dst, x, y, z, src, box);
    for (int i = 0, n = subscribers.texture_copies.n.load(); i < n; ++i)
        subscribers.texture_copies.fns[i](list, dst, x, y, z, src, box);
}

void STDMETHODCALLTYPE resource_copy_hook(ID3D12GraphicsCommandList *list, ID3D12Resource *dst_resource,
                                          ID3D12Resource *src_resource)
{
    hooks.resource_copy.call<void>(list, dst_resource, src_resource);
    D3D12_TEXTURE_COPY_LOCATION dst{}, src{};
    dst.pResource = dst_resource;
    dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    src.pResource = src_resource;
    src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    for (int i = 0, n = subscribers.texture_copies.n.load(); i < n; ++i)
        subscribers.texture_copies.fns[i](list, &dst, 0, 0, 0, &src, nullptr);
}

}

void QueueWatch::count(ID3D12CommandQueue *q)
{
    DWORD tid = GetCurrentThreadId();
    ULONGLONG now = GetTickCount64();
    auto same = [&](const hud::arm_policy::ExecSeen &e) { return e.queue == q; };
    {
        std::lock_guard<std::mutex> g(lock);
        auto it = std::find_if(seen.begin(), seen.end(), same);
        if (it != seen.end()) {
            if (it->type == hud::arm_policy::LIST_TYPE_DIRECT) {
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

bool QueueWatch::recent_direct(DWORD tid, hud::arm_policy::ExecSeen &out)
{
    std::lock_guard<std::mutex> g(lock);
    const hud::arm_policy::ExecSeen *e = hud::arm_policy::present_queue_from_execs(seen, tid);
    if (e)
        out = *e;
    return e != nullptr;
}

bool exec_hooked() { return bool(hooks.exec); }

void exec_original(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists)
{
    if (hooks.exec && hud::arm_policy::vtable_slot_is(q, EXEC_SLOT, hooks.exec.target()))
        hooks.exec.call<void>(q, n, lists);
    else
        q->ExecuteCommandLists(n, lists);
}

void barrier_original(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *b)
{
    if (hooks.barrier && hud::arm_policy::vtable_slot_is(list, BARRIER_SLOT, hooks.barrier.target()))
        hooks.barrier.call<void>(list, n, b);
    else
        list->ResourceBarrier(n, b);
}

bool arm_device()
{
    if (!hooks.gate.offer())
        return hooks.live();
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
        if (SUCCEEDED(dev->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                                                   IID_PPV_ARGS(&buffer))) &&
            SUCCEEDED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) &&
            SUCCEEDED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) &&
            SUCCEEDED(dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&q))))
            ok = install(hooks.setname, (*reinterpret_cast<void ***>(buffer))[SETNAME_SLOT],
                         reinterpret_cast<void *>(&setname_hook), "ID3D12Object::SetName") &&
                 install(hooks.barrier, (*reinterpret_cast<void ***>(list))[BARRIER_SLOT],
                         reinterpret_cast<void *>(&barrier_hook), "ID3D12GraphicsCommandList::ResourceBarrier") &&
                 install(hooks.exec, (*reinterpret_cast<void ***>(q))[EXEC_SLOT], reinterpret_cast<void *>(&exec_hook),
                         "ID3D12CommandQueue::ExecuteCommandLists");
        if (ok) {
            // The copy observers are optional: a copy entry that cannot be hooked leaves the rest of the set armed.
            install(hooks.texture_copy, (*reinterpret_cast<void ***>(list))[TEXTURE_COPY_SLOT],
                    reinterpret_cast<void *>(&texture_copy_hook), "ID3D12GraphicsCommandList::CopyTextureRegion");
            install(hooks.resource_copy, (*reinterpret_cast<void ***>(list))[RESOURCE_COPY_SLOT],
                    reinterpret_cast<void *>(&resource_copy_hook), "ID3D12GraphicsCommandList::CopyResource");
            swofty::log("[d3d12] texture copy observers: CopyTextureRegion %s, CopyResource %s",
                        hooks.texture_copy ? "armed" : "unavailable", hooks.resource_copy ? "armed" : "unavailable");
        }
        if (ok &&
            install(hooks.rtv, (*reinterpret_cast<void ***>(dev))[RTV_SLOT], reinterpret_cast<void *>(&rtv_hook),
                    "ID3D12Device::CreateRenderTargetView") &&
            !install(hooks.om, (*reinterpret_cast<void ***>(list))[OM_SLOT], reinterpret_cast<void *>(&om_hook),
                     "ID3D12GraphicsCommandList::OMSetRenderTargets"))
            hooks.rtv = {};
        if (list)
            list->Close();
    }
    if (ok)
        swofty::log("[d3d12] device hooks armed: SetName (vt+0x30), ResourceBarrier (vt+0xd0), ExecuteCommandLists "
                    "(vt+0x50)%s",
                    hooks.om ? ", CreateRenderTargetView (vt+0xa0), OMSetRenderTargets (vt+0x170)" : "");
    release(q);
    release(list);
    release(alloc);
    release(buffer);
    release(dev);
    return settle(hooks, ok);
}

}
