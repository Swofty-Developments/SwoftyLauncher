#include "core/render/postfx/list_placement.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/render/postfx/pipeline.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <array>
#include <cstdio>
#include <cstring>

namespace render::postfx {

ListPlacement::BackUse &ListPlacement::back_use(ID3D12Resource *b)
{
    BackUse *oldest = &back_uses[0];
    for (auto &u : back_uses) {
        if (u.back == b)
            return u;
        if (u.seen < oldest->seen)
            oldest = &u;
    }
    postfx::hold(oldest->depth, nullptr);
    *oldest = BackUse{};
    oldest->back = b;
    oldest->seen = ++back_seen;
    return *oldest;
}

std::string ListPlacement::name_of(ID3D12Resource *r)
{
    auto it = names.find(r);
    return it == names.end() ? std::string("?") : runtime::narrow(it->second);
}

void ListPlacement::log_diag(ID3D12GraphicsCommandList *list, const Mark &m)
{
    swofty::log("[postfx] the first tonemap list %p has %zu transitions; back buffer %p ends in state 0x%x; %s",
                static_cast<void *>(list), m.seen.size(), static_cast<void *>(m.target), unsigned(m.state),
                m.ui ? "it draws UI too" : "the UI comes in a later list");
    for (auto &b : m.seen) {
        D3D12_RESOURCE_DESC d = b.Transition.pResource->GetDesc();
        swofty::log("[postfx]   %s %llux%u fmt %d: 0x%x -> 0x%x", name_of(b.Transition.pResource).c_str(),
                    (unsigned long long)d.Width, d.Height, int(d.Format), unsigned(b.Transition.StateBefore),
                    unsigned(b.Transition.StateAfter));
    }
}

// While the game reads the finished picture for its HUD (paused, panels over the race), that read is the one
// place the passes run; without such reads they run at the HUD-less step or between the game's lists, and in
// the frame after a read with none in it, between the game's lists or at Present (postfx::HudRead).
bool ListPlacement::hud_reads() { return hud_read.holds(pipeline.frame_count); }

// A list's Mark, made when it has none; its side entry notes that it has one.
ListPlacement::Mark &ListPlacement::mark_of(ID3D12GraphicsCommandList *list)
{
    if (Side *s = side_marks.find(list, true))
        s->marked.store(true, std::memory_order_relaxed);
    return marks[list];
}

void ListPlacement::unmark(ID3D12GraphicsCommandList *list)
{
    marks.erase(list);
    if (Side *s = side_marks.find(list, false)) {
        s->depth.store(nullptr, std::memory_order_relaxed);
        s->bound.store(nullptr, std::memory_order_relaxed);
        s->ui.store(false, std::memory_order_relaxed);
        s->marked.store(false, std::memory_order_relaxed);
    }
}

void ListPlacement::on_create_rtv(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at)
{
    back_views.created(res, at, pipeline.watched);
}

// The picture and the HUD draw into the back buffer as the first render target.
void ListPlacement::on_render_targets(ID3D12GraphicsCommandList *list, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtvs, BOOL)
{
    if (!n || !rtvs)
        return;
    if (ID3D12Resource *res = back_views.bound(rtvs[0].ptr))
        if (Side *s = side_marks.find(list, true))
            s->bound.store(res, std::memory_order_relaxed);
}

// Takes a list's side marks into its Mark m. A depth barrier makes the Mark when the list has none, as
// it does on the locked path.
ListPlacement::Mark *ListPlacement::absorb(ID3D12GraphicsCommandList *list, Mark *m)
{
    Side *s = side_marks.find(list, false);
    if (!s)
        return m;
    ID3D12Resource *d = s->depth.exchange(nullptr, std::memory_order_relaxed);
    bool ui = s->ui.exchange(false, std::memory_order_relaxed);
    if (!m) {
        if (!d)
            return nullptr;
        m = &mark_of(list);
    }
    if (d) {
        m->depth = d;
        m->depth_state = s->depth_state.load(std::memory_order_relaxed);
    }
    m->ui |= ui;
    return m;
}

// Marks a batch that moves only depth and UI images without the lock, the way the locked path marks
// it: a UI barrier counts once the list has a Mark or a depth barrier. False when the batch needs the
// lock: while the diagnostics record each barrier, and once the side table is full.
bool ListPlacement::mark_aside(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, bool depth)
{
    if (!diag_done || order_left || side_marks.full())
        return false;
    Side *s = side_marks.find(list, depth);
    if (!s)
        return !depth;
    bool marked = s->marked.load(std::memory_order_relaxed) || s->depth.load(std::memory_order_relaxed);
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            continue;
        uint8_t c = pipeline.watched.bits(b.Transition.pResource);
        postfx::note(*s, marked, (c & W_DEPTH) != 0, (c & W_UI) != 0, b.Transition.pResource, b.Transition.StateAfter);
    }
    return true;
}

// Marks what each game list does with the watched resources. At the HUD-less step (the HUD-less copy
// leaves UAV while this list has made the back buffer a render target) the passes are recorded into the
// list, before the game's barrier, with every state as it was before this batch.
bool ListPlacement::on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    static const uint32_t probe = perf::probe("core", "postfx ResourceBarrier");
    if (!barriers || !n || pipeline.broken)
        return false;
    perf::Scope p(probe);
    uint8_t seen = 0;
    for (UINT i = 0; i < n; i++)
        if (barriers[i].Type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            seen |= pipeline.watched.bits(barriers[i].Transition.pResource);
    if (!seen)
        return false;
    static const uint32_t marking = perf::probe("core", "postfx ResourceBarrier (watched)");
    perf::Scope m_scope(marking);
    if ((seen & W_DEPTH) && pipeline.gpu.device && pipeline.depth_copy.wanted(pipeline.frame_count))
        pipeline.depth_copy.record(list, n, barriers, pipeline.gpu, pipeline.watched, pipeline.lock, pipeline.retired,
                                   pipeline.frame_count);
    if (!(seen & (W_TONEMAP | W_BACK | W_HUDLESS)) && mark_aside(list, n, barriers, seen & W_DEPTH))
        return false;
    std::lock_guard<std::recursive_mutex> g(pipeline.lock);
    auto it = marks.find(list);
    Mark *m = absorb(list, it == marks.end() ? nullptr : &it->second);
    ID3D12Resource *hudless = nullptr, *leaving = nullptr;
    D3D12_RESOURCE_STATES hudless_now{};
    bool moves_back = false;
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            continue;
        uint8_t c = pipeline.watched.bits(b.Transition.pResource);
        moves_back |= (c & W_BACK) != 0;
        if ((c & W_HUDLESS) && (b.Transition.StateBefore & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) &&
            b.Flags == D3D12_RESOURCE_BARRIER_FLAG_NONE) {
            hudless = b.Transition.pResource;
            hudless_now = b.Transition.StateBefore;
        }
        if ((c & W_BACK) && b.Transition.StateBefore == D3D12_RESOURCE_STATE_RENDER_TARGET &&
            (b.Transition.StateAfter & READ_STATES) && b.Flags == D3D12_RESOURCE_BARRIER_FLAG_NONE && !leaving)
            leaving = b.Transition.pResource;
    }
    if (pipeline.burst.active() && pipeline.gpu.device)
        pipeline.burst.record(list, n, barriers, pipeline.gpu, pipeline.watched, pipeline.frame_count);
    // The game records a frame's lists in parallel, so the core cannot tell at one place whether another
    // place of the same frame runs the passes too. On track the game reads the finished picture once
    // per frame for its HUD, as the first back buffer barrier of the HUD's list (or the first one that
    // leaves the render target in the list that drew it): the passes run there and nowhere else. In
    // menus there is no such read; the passes run at the HUD-less step of the list that made the back
    // buffer a render target, once per back buffer and frame. In the frame after a read the step is held
    // off, and the picture's submission leaves the passes waiting for this frame's read or the fallbacks.
    if ((hudless || leaving) && state_armed && list->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT && !pipeline.killed_now() &&
        !pipeline.config.passes.empty()) {
        PassRecorder::Where w{list,     true,        nullptr, D3D12_RESOURCE_STATE_RENDER_TARGET,
                              hudless,  hudless_now, nullptr, D3D12_RESOURCE_STATE_COMMON,
                              "in-list"};
        bool split_step = false, at_read = false;
        if (leaving && (!m || !m->left_rt) && !postfx::placed_lately(pipeline.frame_count, back_use(leaving).place.placed_at)) {
            BackUse &u = back_use(leaving);
            hud_read.read(pipeline.frame_count);
            w.back = leaving;
            w.hudless = m ? m->hudless : nullptr;
            w.hudless_state = m ? m->hudless_state : D3D12_RESOURCE_STATE_COMMON;
            w.depth = m && m->depth ? m->depth : u.depth;
            w.depth_state = m && m->depth ? m->depth_state : u.depth_state;
            w.path = "in-list (HUD read)";
            at_read = true;
        } else if (hudless && !hud_reads() && !moves_back && m && m->target && m->state == D3D12_RESOURCE_STATE_RENDER_TARGET &&
                   m->slot < 0 && back_use(m->target).place.fresh) {
            w.back = m->target;
            w.depth = m->depth;
            w.depth_state = m->depth_state;
        } else if (hudless && !hud_reads() && !moves_back && (!m || !m->target)) {
            Side *sm = side_marks.find(list, false);
            ID3D12Resource *bound = sm ? sm->bound.load(std::memory_order_relaxed) : nullptr;
            if (bound && back_use(bound).place.split_step(pipeline.frame_count)) {
                BackUse &u = back_use(bound);
                w.back = bound;
                w.depth = m && m->depth ? m->depth : u.depth;
                w.depth_state = m && m->depth ? m->depth_state : u.depth_state;
                w.path = "in-list (split step)";
                split_step = true;
            }
        }
        if (w.back) {
            back_use(w.back).place.place(pipeline.frame_count);
            if (!pipeline.gpu.device && !pipeline.recorder.init(w.back)) {
                pipeline.broken = true;
                return false;
            }
            int k = pipeline.recorder.pick_slot();
            if (k >= 0 && pipeline.recorder.record(w, k)) {
                if (!m)
                    m = &mark_of(list);
                m->slot = k;
                m->ran_back = w.back;
                pipeline.report.stats.in_list++;
                pipeline.report.stats.split_step += split_step;
                pipeline.report.stats.hud_read += at_read;
                if (order_left) {
                    char at[24];
                    snprintf(at, sizeof at, " [PASSES@%04x]", unsigned(uintptr_t(w.back) & 0xffff));
                    m->order += at;
                }
            }
        }
    }
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            continue;
        ID3D12Resource *r = b.Transition.pResource;
        uint8_t c = pipeline.watched.bits(r);
        bool tonemap = c & W_TONEMAP, back = c & W_BACK, depth = c & W_DEPTH;
        if (!m) {
            if (!tonemap && !back && !depth && !(c & W_HUDLESS))
                continue;
            m = &mark_of(list);
        }
        if (depth) {
            m->depth = r;
            m->depth_state = b.Transition.StateAfter;
        }
        if (c & W_UI)
            m->ui = true;
        if (c & W_HUDLESS) {
            m->hudless = r;
            m->hudless_state = b.Transition.StateAfter;
            if ((b.Transition.StateBefore & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) &&
                !(b.Transition.StateAfter & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) && b.Flags == D3D12_RESOURCE_BARRIER_FLAG_NONE)
                m->closes = true;
            if ((b.Transition.StateAfter & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) && m->target &&
                m->state == D3D12_RESOURCE_STATE_RENDER_TARGET) {
                BackUse &u = back_use(m->target);
                postfx::hold(u.depth, m->depth);
                u.depth_state = m->depth_state;
            }
        }
        if (!diag_done && m->seen.size() < 96)
            m->seen.push_back(b);
        if (order_left && m->order.size() < 3000) {
            char step[64];
            snprintf(step, sizeof step, " %s%s%s%s%s:%x>%x%s", tonemap ? "T" : "", back ? "B" : "", depth ? "D" : "",
                     (c & W_HUDLESS) ? "H" : "", (c & W_UI) ? "U" : "", unsigned(b.Transition.StateBefore),
                     unsigned(b.Transition.StateAfter), b.Flags ? "s" : "");
            if (back)
                snprintf(step + strlen(step), sizeof step - strlen(step), "@%04x", unsigned(uintptr_t(r) & 0xffff));
            m->order += step;
        }
        if (tonemap &&
            (b.Transition.StateBefore & (D3D12_RESOURCE_STATE_RENDER_TARGET | D3D12_RESOURCE_STATE_UNORDERED_ACCESS)) &&
            (b.Transition.StateAfter & READ_STATES))
            m->tonemap = true;
        if (back) {
            if (b.Transition.StateBefore == D3D12_RESOURCE_STATE_RENDER_TARGET && (b.Transition.StateAfter & READ_STATES))
                m->left_rt = true;
            m->target = r;
            m->state = b.Transition.StateAfter;
            if (b.Transition.StateAfter == D3D12_RESOURCE_STATE_RENDER_TARGET &&
                !(b.Transition.StateBefore & (READ_STATES | D3D12_RESOURCE_STATE_RENDER_TARGET))) {
                BackUse &u = back_use(r);
                u.place.drawn(pipeline.frame_count);
                m->drew = r;
            }
        }
    }
    return false;
}

// A list reset before it was submitted throws its commands away, the core's passes with them: their
// slot is free again and the frame's back buffer still waits for them.
void ListPlacement::on_reset(ID3D12GraphicsCommandList *list)
{
    std::lock_guard<std::recursive_mutex> g(pipeline.lock);
    auto it = marks.find(list);
    if (it != marks.end() && it->second.slot >= 0) {
        pipeline.recorder.slots[it->second.slot].pending = false;
        if (it->second.ran_back) {
            BackUse &u = back_use(it->second.ran_back);
            u.place.unplace();
        }
        pipeline.report.stats.dropped++;
    }
    unmark(list);
}

// The back buffer whose passes have waited longest for the list that closes its HUD-less step.
ListPlacement::BackUse *ListPlacement::waiting_back()
{
    BackUse *at = nullptr;
    for (auto &u : back_uses)
        if (u.back && u.place.waits(pipeline.frame_count) &&
            (!at || pipeline.frame_count - u.place.opened_at > pipeline.frame_count - at->place.opened_at))
            at = &u;
    return at;
}

// Submits the game's lists, the core's own list after a picture list that ended before the HUD or after the list
// that closed a HUD-less step the picture's list left open, and a fence signal after any list the passes were
// recorded into, so their slot is known done. A list that draws into the game's next back buffer closes the
// passes' frame while the game's own frames do (postfx::FrameClock).
void ListPlacement::on_exec(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists, d3d12::ExecPlan &plan)
{
    static const uint32_t probe = perf::probe("core", "postfx ExecuteCommandLists");
    if (!lists || !n || pipeline.broken)
        return;
    perf::Scope p(probe);
    UINT split = n;
    bool after_close = false, before = false;
    ID3D12Resource *res = nullptr, *hudless = nullptr, *later_depth = nullptr;
    D3D12_RESOURCE_STATES state{}, hudless_state{}, later_depth_state{};
    int signal[FRAMES];
    UINT signals = 0;
    {
        std::lock_guard<std::recursive_mutex> g(pipeline.lock);
        for (UINT i = 0; i < n; i++) {
            auto *gl = reinterpret_cast<ID3D12GraphicsCommandList *>(lists[i]);
            auto it = marks.find(gl);
            Mark *mark = absorb(gl, it == marks.end() ? nullptr : &it->second);
            if (!mark)
                continue;
            Mark &m = *mark;
            if (m.drew) {
                const void *was = pipeline.frame_clock.drawn;
                ULONGLONG now = GetTickCount64();
                if (pipeline.frame_clock.submit(m.drew, now, pipeline.started))
                    pipeline.game_frame(was, now);
            }
            if (m.slot >= 0 && signals < FRAMES)
                signal[signals++] = m.slot;
            pipeline.report.trace.picture |= m.tonemap;
            if (m.tonemap && !diag_done) {
                log_diag(reinterpret_cast<ID3D12GraphicsCommandList *>(lists[i]), m);
                diag_done = true;
            }
            // A list that ends with the back buffer out of render target while the passes still wait for the
            // step's closing list draws the HUD and hands the frame to Present: they run right before it.
            if (m.target && m.state != D3D12_RESOURCE_STATE_RENDER_TARGET && split == n && !pipeline.killed_now() &&
                !pipeline.config.passes.empty() &&
                back_use(m.target).place.target_left(pipeline.frame_count, gl->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT)) {
                split = i;
                before = true;
                res = m.target;
                state = D3D12_RESOURCE_STATE_RENDER_TARGET;
            }
            if (m.target && !m.tonemap)
                back_use(m.target).place.moved(m.state == D3D12_RESOURCE_STATE_RENDER_TARGET);
            if (m.tonemap && m.slot < 0 && split == n && !hud_read.read_in(pipeline.frame_count) && !pipeline.killed_now() &&
                !pipeline.config.passes.empty()) {
                if (m.ui) {
                    pipeline.report.stats.ui_together++;
                    pipeline.report.trace.tried = postfx::IDLE_UI_TOGETHER;
                    pipeline.report.trace.why = postfx::idle_text(pipeline.report.trace.tried);
                } else if (m.target && m.state == D3D12_RESOURCE_STATE_RENDER_TARGET) {
                    // The picture is in the back buffer. A list that ends with the HUD-less step still open
                    // leaves the passes to the list that closes the step, which may draw the rest of the
                    // picture first; while the game's own frames close the passes' frames there is no Present
                    // to fall back on, and the passes run right after this list. In the frame after a HUD read
                    // with none in this one yet, they wait too: the HUD's list may still read the picture and
                    // run them there, and without that read they run at the fallbacks (postfx::HudRead).
                    bool open = m.hudless && (m.hudless_state & D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
                    postfx::PictureWait wait =
                        postfx::picture_wait(open, hud_read.latch(pipeline.frame_count),
                                             pipeline.frame_clock.game_frames(GetTickCount64(), pipeline.started));
                    postfx::PictureEnd end =
                        back_use(m.target).place.picture_submitted(pipeline.frame_count, wait.open, wait.may_defer);
                    pipeline.report.stats.deferred += end == postfx::END_DEFER;
                    if (end == postfx::END_BETWEEN) {
                        split = i;
                        res = m.target;
                        state = m.state;
                        hudless = m.hudless;
                        hudless_state = m.hudless_state;
                    }
                }
            } else if (m.closes && !(m.hudless_state & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) && split == n &&
                       !pipeline.killed_now() && !pipeline.config.passes.empty()) {
                // The list that closed a HUD-less step the picture's list left open, without running the passes
                // in it: they run right after it while the back buffer is still a render target.
                BackUse *u = waiting_back();
                if (u && u->place.step_closed(pipeline.frame_count, gl->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT)) {
                    split = i;
                    after_close = true;
                    res = u->back;
                    state = D3D12_RESOURCE_STATE_RENDER_TARGET;
                    hudless = m.hudless;
                    hudless_state = m.hudless_state;
                }
            }
            if (m.depth) {
                if (split == n || i < split || (i == split && !before)) {
                    postfx::hold(depth_res, m.depth);
                    depth_state = m.depth_state;
                } else {
                    later_depth = m.depth;
                    later_depth_state = m.depth_state;
                }
            }
            if (order_left && m.target && !m.order.empty()) {
                order_left--;
                swofty::log("[postfx] order: list %p of %u in this submission%s:%s%s", static_cast<void *>(lists[i]), n,
                            m.tonemap ? " (picture)" : "", m.order.c_str(),
                            split != i ? ""
                            : before   ? " [BETWEEN before this list]"
                                       : " [BETWEEN after this list]");
            }
            unmark(gl);
        }
    }
    if (split == n && !signals) {
        if (later_depth) {
            std::lock_guard<std::recursive_mutex> g(pipeline.lock);
            postfx::hold(depth_res, later_depth);
            depth_state = later_depth_state;
        }
        return;
    }
    auto take_later_depth = [this, later_depth, later_depth_state](ID3D12CommandQueue *) {
        if (!later_depth)
            return;
        std::lock_guard<std::recursive_mutex> g(pipeline.lock);
        postfx::hold(depth_res, later_depth);
        depth_state = later_depth_state;
    };
    if (split < n) {
        auto between = [this, res, state, hudless, hudless_state, after_close, before](ID3D12CommandQueue *queue) {
            std::lock_guard<std::recursive_mutex> g(pipeline.lock);
            if (pipeline.recorder.run_between(queue, res, state, hudless, hudless_state,
                                              before        ? "between (before the list that takes the back buffer to the HUD)"
                                              : after_close ? "between (after the list that closed the step)"
                                                            : "between")) {
                pipeline.report.stats.between++;
                pipeline.report.stats.after_close += after_close;
                pipeline.report.stats.before_hud += before;
            }
        };
        if (!before)
            plan.after(split, between);
        else if (split)
            plan.after(split - 1, between);
        else
            between(q);
        plan.after(split, take_later_depth);
    } else {
        plan.at_end(take_later_depth);
    }
    if (signals) {
        std::array<int, FRAMES> done{};
        std::copy(signal, signal + signals, done.begin());
        plan.at_end([this, done, signals](ID3D12CommandQueue *queue) {
            std::lock_guard<std::recursive_mutex> g(pipeline.lock);
            pipeline.recorder.read_frequency(queue);
            for (UINT i = 0; i < signals; i++) {
                Slot &s = pipeline.recorder.slots[done[i]];
                queue->Signal(s.fence, ++s.fence_value);
                s.pending = false;
                pipeline.capture.signalled(done[i], s.fence_value);
            }
        });
    }
}

}
