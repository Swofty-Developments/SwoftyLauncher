#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12/subscribers.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstring>
#include <mutex>
#include <vector>

namespace render::d3d12 {

namespace {

enum : uint8_t { ARG_TABLE = 1, ARG_CONSTANTS, ARG_CBV, ARG_SRV, ARG_UAV };
enum : size_t {
    RESET_SLOT = 10,
    CLEAR_STATE_SLOT = 11,
    PSO_SLOT = 25,
    HEAPS_SLOT = 28,
    COMPUTE_ROOT_SLOT = 29,
    GRAPHICS_ROOT_SLOT = 30,
    COMPUTE_TABLE_SLOT = 31,
    GRAPHICS_TABLE_SLOT = 32,
    COMPUTE_CONSTANT_SLOT = 33,
    COMPUTE_CONSTANTS_SLOT = 35,
    COMPUTE_CBV_SLOT = 37,
    COMPUTE_SRV_SLOT = 39,
    COMPUTE_UAV_SLOT = 41,
    PSO1_SLOT = 75,
};

// The list state set: every command list call whose bindings list_state reads back.
struct ListHooks : runtime::loader::HookSet<> {
    safetyhook::InlineHook reset, clear_state, pso, heaps, compute_root, compute_table, compute_constant, compute_constants,
        compute_cbv, compute_srv, compute_uav, pso1, graphics_root, graphics_table;
};

ListHooks hooks;
thread_local ID3D12GraphicsCommandList *state_list;
thread_local ListState *state_of;

// Every list's state, made on first sight. A list first seen anywhere but Reset is not known until its
// next Reset, as what it bound before the hooks saw it is lost.
class ListStates {
public:
    ListState *tracked(ID3D12GraphicsCommandList *l);

private:
    std::mutex lock;
    std::vector<std::pair<ID3D12GraphicsCommandList *, ListState *>> states;
};

constinit ListStates lists;

ListState *tracked(ID3D12GraphicsCommandList *l) { return lists.tracked(l); }

ListState *ListStates::tracked(ID3D12GraphicsCommandList *l)
{
    if (l == state_list)
        return state_of;
    std::lock_guard<std::mutex> g(lock);
    auto it = std::lower_bound(
        states.begin(), states.end(), l,
        [](const std::pair<ID3D12GraphicsCommandList *, ListState *> &e, ID3D12GraphicsCommandList *k) { return e.first < k; });
    if (it == states.end() || it->first != l)
        it = states.insert(it, {l, new ListState()});
    state_list = l;
    state_of = it->second;
    return state_of;
}

void forget(ListState *s, ID3D12PipelineState *pso)
{
    s->known = true;
    s->pso = pso;
    s->state_object = nullptr;
    s->heaps = 0;
    s->compute_root = nullptr;
    s->args = 0;
    s->graphics_root = nullptr;
    s->tables = 0;
}

RootArg *arg_at(ListState *s, UINT index)
{
    if (index >= 64)
        return nullptr;
    if (index >= s->args) {
        for (UINT i = s->args; i <= index; i++)
            s->arg[i].kind = 0;
        s->args = index + 1;
    }
    return &s->arg[index];
}

HRESULT STDMETHODCALLTYPE reset_hook(ID3D12GraphicsCommandList *l, ID3D12CommandAllocator *a, ID3D12PipelineState *p)
{
    HRESULT hr = hooks.reset.call<HRESULT>(l, a, p);
    if (SUCCEEDED(hr)) {
        forget(tracked(l), p);
        for (int i = 0, k = subscribers.resets.n.load(); i < k; i++)
            subscribers.resets.fns[i](l);
    }
    return hr;
}

void STDMETHODCALLTYPE clear_state_hook(ID3D12GraphicsCommandList *l, ID3D12PipelineState *p)
{
    hooks.clear_state.call<void>(l, p);
    ListState *s = tracked(l);
    bool known = s->known;
    forget(s, p);
    s->known = known;
}

void STDMETHODCALLTYPE pso_hook(ID3D12GraphicsCommandList *l, ID3D12PipelineState *p)
{
    hooks.pso.call<void>(l, p);
    ListState *s = tracked(l);
    s->pso = p;
    s->state_object = nullptr;
}

void STDMETHODCALLTYPE pso1_hook(ID3D12GraphicsCommandList *l, ID3D12StateObject *o)
{
    hooks.pso1.call<void>(l, o);
    ListState *s = tracked(l);
    s->state_object = o;
    s->pso = nullptr;
}

void STDMETHODCALLTYPE heaps_hook(ID3D12GraphicsCommandList *l, UINT n, ID3D12DescriptorHeap *const *h)
{
    hooks.heaps.call<void>(l, n, h);
    ListState *s = tracked(l);
    s->heaps = std::min<UINT>(n, 2);
    for (UINT i = 0; i < s->heaps; i++)
        s->heap[i] = h[i];
}

void STDMETHODCALLTYPE compute_root_hook(ID3D12GraphicsCommandList *l, ID3D12RootSignature *r)
{
    hooks.compute_root.call<void>(l, r);
    ListState *s = tracked(l);
    if (s->compute_root != r)
        s->args = 0;
    s->compute_root = r;
}

void STDMETHODCALLTYPE graphics_root_hook(ID3D12GraphicsCommandList *l, ID3D12RootSignature *r)
{
    hooks.graphics_root.call<void>(l, r);
    ListState *s = tracked(l);
    if (s->graphics_root != r)
        s->tables = 0;
    s->graphics_root = r;
}

void STDMETHODCALLTYPE graphics_table_hook(ID3D12GraphicsCommandList *l, UINT i, D3D12_GPU_DESCRIPTOR_HANDLE h)
{
    hooks.graphics_table.call<void>(l, i, h);
    if (i >= 64)
        return;
    ListState *s = tracked(l);
    for (UINT k = s->tables; k < i; k++)
        s->table[k] = 0;
    s->table[i] = h.ptr;
    s->tables = std::max(s->tables, i + 1);
}

void STDMETHODCALLTYPE compute_table_hook(ID3D12GraphicsCommandList *l, UINT i, D3D12_GPU_DESCRIPTOR_HANDLE h)
{
    hooks.compute_table.call<void>(l, i, h);
    if (RootArg *a = arg_at(tracked(l), i)) {
        a->kind = ARG_TABLE;
        a->value = h.ptr;
    }
}

void STDMETHODCALLTYPE compute_constant_hook(ID3D12GraphicsCommandList *l, UINT i, UINT v, UINT at)
{
    hooks.compute_constant.call<void>(l, i, v, at);
    RootArg *a = arg_at(tracked(l), i);
    if (a && at < 64) {
        if (a->kind != ARG_CONSTANTS)
            a->count = 0;
        a->kind = ARG_CONSTANTS;
        a->c[at] = v;
        a->count = std::max(a->count, at + 1);
    }
}

void STDMETHODCALLTYPE compute_constants_hook(ID3D12GraphicsCommandList *l, UINT i, UINT n, const void *v, UINT at)
{
    hooks.compute_constants.call<void>(l, i, n, v, at);
    RootArg *a = arg_at(tracked(l), i);
    if (a && v && at + n <= 64) {
        if (a->kind != ARG_CONSTANTS)
            a->count = 0;
        a->kind = ARG_CONSTANTS;
        memcpy(&a->c[at], v, size_t(n) * 4);
        a->count = std::max(a->count, at + n);
    }
}

template <uint8_t KIND, safetyhook::InlineHook ListHooks::*H>
void STDMETHODCALLTYPE compute_view_hook(ID3D12GraphicsCommandList *l, UINT i, D3D12_GPU_VIRTUAL_ADDRESS va)
{
    (hooks.*H).call<void>(l, i, va);
    if (RootArg *a = arg_at(tracked(l), i)) {
        a->kind = KIND;
        a->value = va;
    }
}

}

bool list_state(ID3D12GraphicsCommandList *list, ListState &out)
{
    if (!hooks.reset)
        return false;
    ListState *s = tracked(list);
    if (!s->known)
        return false;
    out.known = true;
    out.pso = s->pso;
    out.state_object = s->state_object;
    out.heaps = s->heaps;
    out.heap[0] = s->heap[0];
    out.heap[1] = s->heap[1];
    out.compute_root = s->compute_root;
    out.args = s->args;
    out.graphics_root = s->graphics_root;
    out.tables = s->tables;
    memcpy(out.table, s->table, size_t(s->tables) * 8);
    for (UINT i = 0; i < s->args; i++) {
        out.arg[i].kind = s->arg[i].kind;
        out.arg[i].count = s->arg[i].count;
        out.arg[i].value = s->arg[i].value;
        if (s->arg[i].kind == ARG_CONSTANTS)
            memcpy(out.arg[i].c, s->arg[i].c, size_t(s->arg[i].count) * 4);
    }
    return true;
}

// Puts a list's bindings back as list_state read them. The calls go through the hooks, so the list's
// tracked state ends as it was.
void restore_list_state(ID3D12GraphicsCommandList *l, const ListState &s)
{
    if (s.heaps)
        l->SetDescriptorHeaps(s.heaps, s.heap);
    if (s.graphics_root)
        for (UINT i = 0; i < s.tables; i++)
            if (s.table[i])
                l->SetGraphicsRootDescriptorTable(i, {s.table[i]});
    ID3D12GraphicsCommandList4 *l4 = nullptr;
    if (s.state_object && SUCCEEDED(l->QueryInterface(IID_PPV_ARGS(&l4)))) {
        l4->SetPipelineState1(s.state_object);
        l4->Release();
    } else if (s.pso)
        l->SetPipelineState(s.pso);
    if (!s.compute_root)
        return;
    l->SetComputeRootSignature(s.compute_root);
    for (UINT i = 0; i < s.args; i++) {
        const RootArg &a = s.arg[i];
        switch (a.kind) {
        case ARG_TABLE:
            l->SetComputeRootDescriptorTable(i, {a.value});
            break;
        case ARG_CONSTANTS:
            l->SetComputeRoot32BitConstants(i, a.count, a.c, 0);
            break;
        case ARG_CBV:
            l->SetComputeRootConstantBufferView(i, a.value);
            break;
        case ARG_SRV:
            l->SetComputeRootShaderResourceView(i, a.value);
            break;
        case ARG_UAV:
            l->SetComputeRootUnorderedAccessView(i, a.value);
            break;
        }
    }
}

bool arm_list_state()
{
    if (!hooks.gate.offer())
        return hooks.live();
    ID3D12Device *dev = probe_device();
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12GraphicsCommandList4 *list4 = nullptr;
    bool ok = false;
    if (dev && SUCCEEDED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) &&
        SUCCEEDED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) &&
        SUCCEEDED(list->QueryInterface(IID_PPV_ARGS(&list4)))) {
        void **vt = *reinterpret_cast<void ***>(list), **vt4 = *reinterpret_cast<void ***>(list4);
        struct Want {
            size_t slot;
            safetyhook::InlineHook *h;
            void *detour;
        } want[] = {
            {RESET_SLOT, &hooks.reset, reinterpret_cast<void *>(&reset_hook)},
            {CLEAR_STATE_SLOT, &hooks.clear_state, reinterpret_cast<void *>(&clear_state_hook)},
            {PSO_SLOT, &hooks.pso, reinterpret_cast<void *>(&pso_hook)},
            {HEAPS_SLOT, &hooks.heaps, reinterpret_cast<void *>(&heaps_hook)},
            {COMPUTE_ROOT_SLOT, &hooks.compute_root, reinterpret_cast<void *>(&compute_root_hook)},
            {COMPUTE_TABLE_SLOT, &hooks.compute_table, reinterpret_cast<void *>(&compute_table_hook)},
            {GRAPHICS_ROOT_SLOT, &hooks.graphics_root, reinterpret_cast<void *>(&graphics_root_hook)},
            {GRAPHICS_TABLE_SLOT, &hooks.graphics_table, reinterpret_cast<void *>(&graphics_table_hook)},
            {COMPUTE_CONSTANT_SLOT, &hooks.compute_constant, reinterpret_cast<void *>(&compute_constant_hook)},
            {COMPUTE_CONSTANTS_SLOT, &hooks.compute_constants, reinterpret_cast<void *>(&compute_constants_hook)},
            {COMPUTE_CBV_SLOT, &hooks.compute_cbv,
             reinterpret_cast<void *>(&compute_view_hook<ARG_CBV, &ListHooks::compute_cbv>)},
            {COMPUTE_SRV_SLOT, &hooks.compute_srv,
             reinterpret_cast<void *>(&compute_view_hook<ARG_SRV, &ListHooks::compute_srv>)},
            {COMPUTE_UAV_SLOT, &hooks.compute_uav,
             reinterpret_cast<void *>(&compute_view_hook<ARG_UAV, &ListHooks::compute_uav>)},
            {PSO1_SLOT, &hooks.pso1, reinterpret_cast<void *>(&pso1_hook)},
        };
        ok = true;
        for (auto &a : want)
            for (auto &b : want)
                if (&a != &b && (a.slot == PSO1_SLOT ? vt4 : vt)[a.slot] == (b.slot == PSO1_SLOT ? vt4 : vt)[b.slot]) {
                    swofty::log("[d3d12] command list slots vt+0x%zx and vt+0x%zx share their code; list state is not "
                                "followed",
                                a.slot * 8, b.slot * 8);
                    ok = false;
                }
        for (auto &w : want)
            if (ok)
                ok = install(*w.h, (w.slot == PSO1_SLOT ? vt4 : vt)[w.slot], w.detour, "ID3D12GraphicsCommandList state");
        if (!ok)
            for (auto &w : want)
                *w.h = {};
        list->Close();
    }
    if (ok)
        swofty::log("[d3d12] command list state hooks armed: Reset, ClearState, SetPipelineState(1), "
                    "SetDescriptorHeaps, the compute root bindings and the graphics descriptor tables");
    release(list4);
    release(list);
    release(alloc);
    release(dev);
    return settle(hooks, ok);
}

}
