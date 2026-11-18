#pragma once
#include "core/render/d3d12_hooks.hpp"
#include "core/render/postfx/back_buffer_views.hpp"
#include "core/render/postfx/descriptor_logic.hpp"
#include "core/render/postfx/frame_logic.hpp"

#include <windows.h>

#include <d3d12.h>

#include <string>
#include <unordered_map>
#include <vector>

namespace render::postfx {

class Pipeline;

// Where in the game's command lists a frame's passes run. The barrier hook marks what each list does with the
// watched resources (the tonemap output, the back buffers, the HUD-less copy, the depth buffer, UI images) and
// runs the passes in the list at the place it finds; the submission hook runs them between the game's lists
// where no list had such a place, and signals the slots the game's lists carried.
class ListPlacement {
public:
    struct Mark {
        bool tonemap = false, ui = false, left_rt = false, closes = false;
        int slot = -1;
        ID3D12Resource *target = nullptr, *hudless = nullptr, *depth = nullptr, *ran_back = nullptr, *drew = nullptr;
        D3D12_RESOURCE_STATES state = D3D12_RESOURCE_STATE_COMMON, hudless_state = D3D12_RESOURCE_STATE_COMMON,
                              depth_state = D3D12_RESOURCE_STATE_COMMON;
        std::vector<D3D12_RESOURCE_BARRIER> seen;
        std::string order;
    };

    // Each back buffer's use for one frame: fresh from the barrier that makes it a render target until the
    // passes run on it, the frame they last ran on it, and the depth buffer as the picture's list left it.
    // The lists of a frame are recorded in parallel, so the list that draws the rest of a split picture can
    // run the passes before the picture's list makes the back buffer a render target; that barrier then
    // leaves it not fresh. The depth buffer here and in
    // depth_res is held (postfx::hold): the HUD's list and the lists between read it a frame later, after
    // the game may have freed it.
    struct BackUse {
        ID3D12Resource *back = nullptr;
        BackPlace place;
        ID3D12Resource *depth = nullptr;
        D3D12_RESOURCE_STATES depth_state = D3D12_RESOURCE_STATE_COMMON;
        unsigned seen = 0;
    };

    // Batches that move only depth and UI images, most watched batches, are marked without the lock by
    // the thread recording the list; the locked paths take the side marks into the list's Mark before they
    // read it.
    using Side = SideMark<ID3D12GraphicsCommandList, ID3D12Resource, D3D12_RESOURCE_STATES>;

    explicit ListPlacement(Pipeline &owner) : pipeline(owner) {}

    ID3D12Resource *depth_res = nullptr;
    D3D12_RESOURCE_STATES depth_state = D3D12_RESOURCE_STATE_COMMON;
    std::unordered_map<ID3D12Resource *, std::wstring> names;
    std::unordered_map<ID3D12GraphicsCommandList *, Mark> marks;
    unsigned order_left = 0;
    BackUse back_uses[6];
    BackBufferViews back_views;
    unsigned back_seen = 0;
    HudRead hud_read;
    bool diag_done = false, saw_back = false, tonemap_named = false, state_armed = false;
    std::vector<std::wstring> named_seen;
    SideMarks<ID3D12GraphicsCommandList, ID3D12Resource, D3D12_RESOURCE_STATES> side_marks;

    void on_setname(ID3D12Object *o, LPCWSTR name);
    void on_create_rtv(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at);
    void on_render_targets(ID3D12GraphicsCommandList *list, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtvs, BOOL);
    bool on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers);
    void on_reset(ID3D12GraphicsCommandList *list);
    void on_exec(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists, d3d12::ExecPlan &plan);

    void drop_depths(ID3D12Resource *keep, ID3D12Resource *renamed = nullptr);
    BackUse &back_use(ID3D12Resource *b);
    BackUse *waiting_back();
    bool hud_reads();

private:
    std::string name_of(ID3D12Resource *r);
    void log_diag(ID3D12GraphicsCommandList *list, const Mark &m);
    void log_named(ID3D12Object *o, LPCWSTR name);
    Mark &mark_of(ID3D12GraphicsCommandList *list);
    void unmark(ID3D12GraphicsCommandList *list);
    Mark *absorb(ID3D12GraphicsCommandList *list, Mark *m);
    bool mark_aside(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, bool depth);

    Pipeline &pipeline;
};

}
