#include "core/render/postfx/pass_recorder.hpp"

#include "core/frontend/view_state.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/postfx/pipeline.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstring>

namespace render::postfx {

bool PassRecorder::init(ID3D12Resource *res)
{
    if (FAILED(res->GetDevice(IID_PPV_ARGS(&pipeline.gpu.device))))
        return false;
    if (!pipeline.gpu.make_root()) {
        swofty::log("[postfx] could not make the passes' root signature; post-processing is off");
        return false;
    }
    pipeline.gpu.make_mip_pso();
    for (auto &s : slots) {
        if (FAILED(pipeline.gpu.device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&s.fence))) ||
            FAILED(pipeline.gpu.device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&s.alloc))) ||
            FAILED(pipeline.gpu.device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, s.alloc, nullptr,
                                                          IID_PPV_ARGS(&s.list))))
            return false;
        s.list->Close();
    }
    D3D12_QUERY_HEAP_DESC qd{};
    qd.Type = D3D12_QUERY_HEAP_TYPE_TIMESTAMP;
    qd.Count = FRAMES * MAX_PASSES * 2;
    if (SUCCEEDED(pipeline.gpu.device->CreateQueryHeap(&qd, IID_PPV_ARGS(&queries))))
        query_readback = pipeline.gpu.buffer(D3D12_HEAP_TYPE_READBACK, qd.Count * 8, D3D12_RESOURCE_STATE_COPY_DEST);
    for (auto &p : pipeline.config.passes) {
        if (p.inst >= 0)
            continue;
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = pipeline.gpu.root;
        pd.CS.pShaderBytecode = p.dxil.data();
        pd.CS.BytecodeLength = p.dxil.size();
        HRESULT hr = pipeline.gpu.device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&p.pso));
        if (FAILED(hr)) {
            p.off = true;
            swofty::log("[postfx] %s's pass %s is off: its pipeline could not be made (0x%08lx)", p.addon.c_str(), p.name.c_str(),
                        hr);
        } else
            swofty::log("[postfx] %s's pass %s is ready (%s, %zu bytes of DXIL)", p.addon.c_str(), p.name.c_str(),
                        p.shader.c_str(), p.dxil.size());
    }
    pipeline.started = GetTickCount64();
    return true;
}

void PassRecorder::read_frequency(ID3D12CommandQueue *q)
{
    UINT64 freq = 0;
    if (!ticks_per_ms && query_readback && q && SUCCEEDED(q->GetTimestampFrequency(&freq)) && freq)
        ticks_per_ms = double(freq) / 1000.0;
}

// Packs the data of the live passes into slot s's upload buffer (offsets[i] and sizes[i] for live[i]), growing
// the buffer when they don't fit. Sizes are 0 where a pass has none. Under lock, for a slot the GPU is done with.
void PassRecorder::pack_data(Slot &s, const std::vector<int> &live, std::vector<UINT> &offsets, std::vector<UINT> &sizes)
{
    offsets.assign(live.size(), 0);
    sizes.assign(live.size(), 0);
    if (!any_data.load(std::memory_order_relaxed))
        return;
    std::lock_guard<std::mutex> g(data_lock);
    std::vector<const std::string *> found(live.size(), nullptr);
    UINT total = 0;
    for (size_t i = 0; i < live.size(); i++) {
        const RenderPass &p = pipeline.config.passes[size_t(live[i])];
        const std::string *b = pass_data.find(postfx::DataStore::key(p.addon_id, p.name));
        if (!b || b->empty())
            continue;
        found[i] = b;
        offsets[i] = total;
        sizes[i] = UINT(b->size());
        total += (sizes[i] + DATA_ALIGN - 1) / DATA_ALIGN * DATA_ALIGN;
    }
    if (!total)
        return;
    if (total > s.data_cap) {
        UINT cap = (total + DATA_MAX - 1) / DATA_MAX * DATA_MAX;
        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_UPLOAD;
        D3D12_RESOURCE_DESC bd{};
        bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
        bd.Width = cap;
        bd.Height = bd.DepthOrArraySize = bd.MipLevels = 1;
        bd.SampleDesc.Count = 1;
        bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        ID3D12Resource *b = nullptr;
        void *at = nullptr;
        if (FAILED(pipeline.gpu.device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd,
                                                                D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, IID_PPV_ARGS(&b))) ||
            FAILED(b->Map(0, nullptr, &at))) {
            release(b);
            sizes.assign(live.size(), 0);
            return;
        }
        pipeline.retire({s.data});
        s.data = b;
        s.data_at = static_cast<uint8_t *>(at);
        s.data_cap = cap;
    }
    UINT bytes = 0, with = 0;
    for (size_t i = 0; i < live.size(); i++)
        if (found[i]) {
            memcpy(s.data_at + offsets[i], found[i]->data(), sizes[i]);
            // a view is whole words: zero the rest of the last one, which could hold another pass's earlier bytes
            memset(s.data_at + offsets[i] + sizes[i], 0, (4 - sizes[i] % 4) % 4);
            bytes += sizes[i];
            with++;
        }
    if (!data_logged) {
        data_logged = true;
        swofty::log("[postfx] passes read data from their addons' code (t10): %u pass(es), %u byte(s)", with, bytes);
    }
}

void PassRecorder::read_times(Slot &s)
{
    if (!ticks_per_ms || !s.passes)
        return;
    UINT base = UINT(&s - slots) * MAX_PASSES * 2;
    D3D12_RANGE r{SIZE_T(base) * 8, SIZE_T(base + s.passes * 2) * 8};
    uint64_t *t = nullptr;
    if (FAILED(query_readback->Map(0, &r, reinterpret_cast<void **>(&t))))
        return;
    for (UINT i = 0; i < s.passes; i++) {
        if (size_t(s.pass_of[i]) >= pipeline.config.passes.size())
            continue;
        RenderPass &p = pipeline.config.passes[size_t(s.pass_of[i])];
        uint64_t a = t[base + i * 2], b = t[base + i * 2 + 1];
        if (b < a)
            continue;
        double ms = double(b - a) / ticks_per_ms;
        p.ms_sum += ms;
        p.ms_peak = std::max(p.ms_peak, ms);
        if (++p.ms_frames < BUDGET_FRAMES)
            continue;
        double avg = p.ms_sum / p.ms_frames;
        if (avg > p.budget_ms) {
            if (++p.over >= 3 && !p.off) {
                p.off = true;
                swofty::log("[postfx] %s is off: it took %.3f ms of GPU time a frame, over its %.2f ms budget",
                            pipeline.config.describe(p).c_str(), avg, p.budget_ms);
            }
        } else
            p.over = 0;
        if (pipeline.ran_total <= 4 * BUDGET_FRAMES)
            swofty::log("[postfx] %s: %.3f ms of GPU time a frame on average, %.3f ms at most",
                        pipeline.config.describe(p).c_str(), avg, p.ms_peak);
        p.ms_sum = 0;
        p.ms_peak = 0;
        p.ms_frames = 0;
    }
    D3D12_RANGE none{0, 0};
    query_readback->Unmap(0, &none);
}

// A slot whose GPU work is done. The game records frames ahead, so the slots of lists it has not
// submitted yet are skipped rather than waited for. A slot recorded into a game list that was never
// submitted is free again after two seconds: the game reset that list, so the core's work in it never
// runs.
int PassRecorder::pick_slot()
{
    ULONGLONG now = GetTickCount64();
    for (UINT n = 0; n < FRAMES; n++) {
        UINT k = (slot_at + n) % FRAMES;
        Slot &s = slots[k];
        if (s.pending && now - s.armed_at > 2000) {
            swofty::log("[postfx] a game list holding the passes was never submitted; its slot is reused");
            s.pending = false;
        }
        if (s.pending || s.fence->GetCompletedValue() < s.fence_value)
            continue;
        slot_at = (k + 1) % FRAMES;
        return int(k);
    }
    if (!warned_busy)
        swofty::log("[postfx] every one of the %u slots is in flight; passes skip frames until one is free", FRAMES);
    warned_busy = true;
    pipeline.report.stats.busy++;
    pipeline.report.trace.tried = postfx::IDLE_BUSY;
    pipeline.report.trace.why = postfx::idle_text(pipeline.report.trace.tried);
    return -1;
}

bool PassRecorder::record(const Where &w, int k)
{
    if (!pipeline.gpu.device && !init(w.back)) {
        pipeline.broken = true;
        return false;
    }
    if (pipeline.gpu.device->GetDeviceRemovedReason() != S_OK) {
        pipeline.broken = true;
        swofty::log("[postfx] the GPU device was removed; post-processing is off");
        return false;
    }
    pipeline.capture.land(slots, pipeline.frame.desc, pipeline.frame.view);
    pipeline.retired.free(pipeline.frame_count);
    Slot &s = slots[k];
    postfx::hold(s.depth, w.depth);
    read_times(s);
    s.passes = 0;
    D3D12_RESOURCE_DESC d = w.back->GetDesc();
    if (!pipeline.frame.heap || d.Width != pipeline.frame.desc.Width || d.Height != pipeline.frame.desc.Height ||
        d.Format != pipeline.frame.desc.Format)
        if (!pipeline.frame.make_frame(d)) {
            pipeline.broken = true;
            return false;
        }
    if (!pipeline.frame.ensure_work(k)) {
        pipeline.broken = true;
        return false;
    }
    if (s.views_made != pipeline.frame.textures_made) {
        for (auto &view : s.views)
            view.set = false;
        s.views_made = pipeline.frame.textures_made;
    }
    SlotTextures &wk = pipeline.frame.work[k];
    frontend::view::State v = frontend::view::current();
    pipeline.report.trace.view = v;
    unsigned idle = 0;
    std::vector<int> live = pipeline.config.live_passes(v, &idle);
    pipeline.report.trace.live = unsigned(live.size());
    if (live.empty()) {
        pipeline.report.trace.tried = postfx::idle_of_passes(idle);
        pipeline.report.trace.why = postfx::idle_text(pipeline.report.trace.tried);
        return false;
    }
    d3d12::ListState saved;
    if (w.game && !d3d12::list_state(w.list, saved)) {
        pipeline.report.stats.unknown_state++;
        pipeline.report.trace.tried = postfx::IDLE_LIST_STATE;
        pipeline.report.trace.why = postfx::idle_text(pipeline.report.trace.tried);
        return false;
    }
    ID3D12GraphicsCommandList *l = w.list;
    ULONGLONG now = GetTickCount64();
    pipeline.capture.arm(now, pipeline.ran_total);
    if (pipeline.frame.prev_serial != v.serial) {
        pipeline.frame.prev_valid = false;
        for (auto &t : pipeline.config.textures)
            t.clear |= t.persistent;
    }
    bool had_previous = pipeline.frame.prev && pipeline.frame.prev_valid && pipeline.frame.prev_serial == v.serial;
    bool has_previous = had_previous && pipeline.frame_count - pipeline.frame.prev_frame <= 1;
    pipeline.report.stats.no_previous += !has_previous;
    pipeline.report.stats.prev_gap += had_previous && !has_previous;
    UINT pw = UINT(d.Width), ph = d.Height;

    ClipRecorder::Entry *clip_entry =
        pipeline.clip.begin(pipeline.gpu, d, k, pipeline.frame.view, pipeline.retired, pipeline.frame_count);
    transition(l, w.back, w.back_state, D3D12_RESOURCE_STATE_COPY_SOURCE);
    transition(l, wk.orig, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyResource(wk.orig, w.back);
    if (clip_entry)
        pipeline.clip.copy(l, w.back, *clip_entry);
    if (pipeline.capture.armed())
        pipeline.capture.copy(l, w.back, 0);
    transition(l, wk.orig, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
    for (auto &t : pipeline.config.textures)
        if (!t.file.empty() && !t.uploaded && t.res)
            pipeline.frame.upload(l, t);
    l->SetComputeRootSignature(pipeline.gpu.root);
    l->SetDescriptorHeaps(1, &pipeline.frame.heap);
    pipeline.frame.mip_steps_used = 0;
    UINT clear_at = FRAMES * MAX_PASSES * TABLE, ntex = UINT(pipeline.config.textures.size());
    for (size_t i = 0; i < pipeline.config.textures.size(); i++) {
        PassTexture &t = pipeline.config.textures[i];
        bool own = !t.shared();
        ID3D12Resource *r = pipeline.frame.tex_of(i, k);
        if (!r || !(own ? wk.clear : t.clear))
            continue;
        UINT at = own ? ntex * (1 + UINT(k)) + UINT(i) : UINT(i);
        pipeline.gpu.uav(r, t.format, pipeline.frame.cpu_at(pipeline.frame.heap, clear_at + at));
        pipeline.gpu.uav(r, t.format, pipeline.frame.cpu_at(pipeline.frame.cpu, at));
        transition(l, r, HOME, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        const float zero[4] = {};
        l->ClearUnorderedAccessViewFloat(pipeline.frame.gpu_at(pipeline.frame.heap, clear_at + at),
                                         pipeline.frame.cpu_at(pipeline.frame.cpu, at), r, zero, 0, nullptr);
        transition(l, r, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
        if (t.levels() > 1)
            pipeline.frame.build_mips(l, r, t.format, t.w, t.h, t.levels(), k);
        if (!own)
            t.clear = false;
    }
    wk.clear = false;

    uint32_t c[CONSTANTS] = {};
    float t_now = float(double(now - pipeline.started) / 1000.0);
    memcpy(&c[4], &t_now, 4);
    c[5] = pipeline.frame_count;
    c[6] = format_kind(pipeline.frame.view);
    memcpy(&c[27], &pipeline.frametime, 4);
    c[28] = pw;
    c[29] = ph;
    c[30] = pipeline.frame_count * 2654435761u + 0x9e3779b9u;
    c[31] = v.kind;
    c[32] = v.flags;
    c[33] = v.valid;
    c[34] = has_previous;
    ID3D12Resource *depth = nullptr;
    DXGI_FORMAT depth_fmt = DXGI_FORMAT_R32_FLOAT;
    if (w.depth && (w.depth_state & (D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_DEPTH_WRITE))) {
        D3D12_RESOURCE_DESC dd = w.depth->GetDesc();
        DXGI_FORMAT f = depth_view(dd.Format);
        if (f != DXGI_FORMAT_UNKNOWN && dd.SampleDesc.Count == 1 && !(dd.Flags & D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE)) {
            depth = w.depth;
            depth_fmt = f;
            c[24] = UINT(dd.Width);
            c[25] = dd.Height;
            c[26] = 1;
        }
    }
    if (depth && !depth_logged) {
        depth_logged = true;
        swofty::log("[postfx] passes read the game's depth buffer (%ux%u, format %d) as t1", c[24], c[25],
                    int(depth->GetDesc().Format));
    }
    // Where this place leaves no readable depth (the HUD's list on track), the frame's copy, when the game
    // wrote its depth buffer this frame or the one before. It rests readable: no transition. Copies are made
    // only while passes keep running in such places (depth_copy_until).
    bool depth_copied = false;
    if (!depth) {
        if (ID3D12Resource *dc = pipeline.depth_copy.fresh(pipeline.frame_count)) {
            D3D12_RESOURCE_DESC dd = dc->GetDesc();
            depth = dc;
            depth_fmt = depth_view(dd.Format);
            c[24] = UINT(dd.Width);
            c[25] = dd.Height;
            c[26] = 1;
            depth_copied = true;
        }
    }
    D3D12_RESOURCE_STATES depth_read = D3D12_RESOURCE_STATE_DEPTH_READ | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    bool depth_moved = depth && !depth_copied && !(w.depth_state & D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    if (depth_moved)
        transition(l, depth, w.depth_state, depth_read);
    bool timed = queries && query_readback;
    UINT qbase = UINT(k) * MAX_PASSES * 2;
    ID3D12Resource *cur = wk.orig;
    int next = 0;
    std::vector<UINT> data_offsets, data_sizes;
    pack_data(s, live, data_offsets, data_sizes);
    size_t live_i = 0;
    for (int index : live) {
        RenderPass &p = pipeline.config.passes[size_t(index)];
        UINT data_size = data_sizes[live_i], data_offset = data_offsets[live_i];
        live_i++;
        ID3D12Resource *out = p.output < 0 ? wk.pic[next] : pipeline.frame.tex_of(size_t(p.output), k);
        DXGI_FORMAT of = p.output < 0 ? pipeline.frame.view : pipeline.config.textures[size_t(p.output)].format;
        UINT ow = p.output < 0 ? pw : pipeline.config.textures[size_t(p.output)].w,
             oh = p.output < 0 ? ph : pipeline.config.textures[size_t(p.output)].h;
        UINT at = (UINT(k) * MAX_PASSES + s.passes) * TABLE;
        auto *view = &s.views[s.passes * TABLE];
        auto put_srv = [&](UINT i, ID3D12Resource *r, DXGI_FORMAT f) {
            if (postfx::rewrite(view[i], r, f))
                pipeline.gpu.srv(r, f, pipeline.frame.cpu_at(pipeline.frame.heap, at + i));
        };
        put_srv(0, cur, pipeline.frame.view);
        put_srv(1, depth, depth_fmt);
        for (UINT i = 0; i < INPUTS; i++) {
            DXGI_FORMAT f;
            ID3D12Resource *r = pipeline.frame.input_res(p.inputs[i], k, f);
            put_srv(2 + i, r, f);
        }
        if (data_size) { // written every frame: the bytes move within the buffer
            pipeline.gpu.data_srv(s.data, data_offset, data_size, pipeline.frame.cpu_at(pipeline.frame.heap, at + DATA_AT));
            view[DATA_AT] = {};
        } else if (postfx::rewrite(view[DATA_AT], static_cast<ID3D12Resource *>(nullptr), DXGI_FORMAT_UNKNOWN)) {
            pipeline.gpu.data_srv(nullptr, 0, 0, pipeline.frame.cpu_at(pipeline.frame.heap, at + DATA_AT));
        }
        c[postfx::DATA_SIZE_AT] = data_size;
        if (postfx::rewrite(view[TABLE - 1], out, of))
            pipeline.gpu.uav(out, of, pipeline.frame.cpu_at(pipeline.frame.heap, at + TABLE - 1));
        transition(l, out, HOME, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        c[0] = ow;
        c[1] = oh;
        float texel[2] = {1.0f / float(ow), 1.0f / float(oh)};
        memcpy(&c[2], texel, 8);
        c[7] = s.passes;
        memcpy(&c[postfx::OPTIONS_AT], p.options, 16 * sizeof(float));
        memcpy(&c[postfx::PARAMS_AT], p.options + 16, 16 * sizeof(float));
        if (timed)
            l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, qbase + s.passes * 2);
        l->SetPipelineState(p.pso);
        l->SetComputeRoot32BitConstants(0, CONSTANTS, c, 0);
        l->SetComputeRootDescriptorTable(1, pipeline.frame.gpu_at(pipeline.frame.heap, at));
        l->Dispatch(postfx::groups(ow, p.threads[0]), postfx::groups(oh, p.threads[1]), 1);
        if (timed)
            l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, qbase + s.passes * 2 + 1);
        transition(l, out, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
        if (p.output >= 0 && pipeline.config.textures[size_t(p.output)].levels() > 1)
            pipeline.frame.build_mips(l, out, of, ow, oh, pipeline.config.textures[size_t(p.output)].levels(), k);
        s.pass_of[s.passes++] = index;
        if (p.output < 0) {
            cur = out;
            next = 1 - next;
        }
    }
    if (depth_moved)
        transition(l, depth, depth_read, w.depth_state);
    if (timed)
        l->ResolveQueryData(queries, D3D12_QUERY_TYPE_TIMESTAMP, qbase, s.passes * 2, query_readback, UINT64(qbase) * 8);

    transition(l, cur, HOME, D3D12_RESOURCE_STATE_COPY_SOURCE);
    if (cur != wk.orig) {
        transition(l, w.back, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COPY_DEST);
        l->CopyResource(w.back, cur);
        transition(l, w.back, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE);
    }
    if (pipeline.capture.armed())
        pipeline.capture.copy(l, w.back, 1);
    transition(l, w.back, D3D12_RESOURCE_STATE_COPY_SOURCE, w.back_state);
    if (w.hudless && cur != wk.orig && pipeline.fg_on.load()) {
        D3D12_RESOURCE_DESC hd = w.hudless->GetDesc();
        if (hd.Width == d.Width && hd.Height == d.Height && view_format(hd.Format) == pipeline.frame.view) {
            transition(l, w.hudless, w.hudless_state, D3D12_RESOURCE_STATE_COPY_DEST);
            l->CopyResource(w.hudless, cur);
            transition(l, w.hudless, D3D12_RESOURCE_STATE_COPY_DEST, w.hudless_state);
            if (!hudless_logged)
                swofty::log("[postfx] the game's HUD-less copy of the picture gets the passes too, for Frame Generation");
            hudless_logged = true;
        }
    }
    if (pipeline.frame.prev) {
        transition(l, pipeline.frame.prev, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
        l->CopyResource(pipeline.frame.prev, cur);
        transition(l, pipeline.frame.prev, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
        pipeline.frame.prev_valid = true;
        pipeline.frame.prev_serial = v.serial;
        pipeline.frame.prev_frame = pipeline.frame_count;
    }
    transition(l, cur, D3D12_RESOURCE_STATE_COPY_SOURCE, HOME);
    if (w.game)
        d3d12::restore_list_state(l, saved);

    s.pending = true;
    s.armed_at = now;
    pipeline.capture.ran(k, now);
    pipeline.report.trace.runs++;
    pipeline.report.stats.runs++;
    pipeline.report.trace.ran = s.passes;
    pipeline.report.trace.path = w.path;
    pipeline.report.trace.back = w.back;
    pipeline.report.trace.why = nullptr;
    if (++pipeline.ran_total == 1) {
        pipeline.capture.first_run(now);
        swofty::log("[postfx] the picture now goes through %u pass(es) before the HUD, %s", s.passes,
                    w.game ? "inside the game's list at its HUD-less step" : "in a list between the game's lists");
    }
    return true;
}

bool PassRecorder::run_between(ID3D12CommandQueue *q, ID3D12Resource *back, D3D12_RESOURCE_STATES state, ID3D12Resource *hudless,
                               D3D12_RESOURCE_STATES hudless_state, const char *path)
{
    if (!pipeline.gpu.device && !init(back)) {
        pipeline.broken = true;
        return false;
    }
    int k = pick_slot();
    if (k < 0)
        return false;
    Slot &s = slots[k];
    if (FAILED(s.alloc->Reset()) || FAILED(s.list->Reset(s.alloc, nullptr))) {
        pipeline.broken = true;
        swofty::log("[postfx] could not record the passes' command list; post-processing is off");
        return false;
    }
    Where w{s.list, false, back, state, hudless, hudless_state, pipeline.placement.depth_res, pipeline.placement.depth_state,
            path};
    bool made = record(w, k);
    if (FAILED(s.list->Close())) {
        pipeline.broken = true;
        s.pending = false;
        swofty::log("[postfx] the passes' command list did not close; post-processing is off");
        return false;
    }
    if (!made)
        return false;
    read_frequency(q);
    ID3D12CommandList *one[] = {s.list};
    d3d12::exec_original(q, 1u, one);
    q->Signal(s.fence, ++s.fence_value);
    s.pending = false;
    pipeline.capture.signalled(k, s.fence_value);
    return true;
}

}
