#include "core/render/d3d12_hooks.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/frontend/view_state.hpp"
#include "core/hud/hud_guard.hpp"
#include "core/render/postfx/capture.hpp"
#include "core/render/postfx/gpu.hpp"
#include "core/render/postfx/pass.hpp"
#include "core/render/postfx/report.hpp"
#include "core/render/postfx/resource_watch.hpp"
#include "core/render/postfx/rules.hpp"
#include "core/render/postfx_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <d3d12.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

bool settings_state(const std::string &addon, const std::string &option, size_t &current,
                    std::vector<std::string> *texts, std::vector<std::string> *commands,
                    std::vector<std::string> *values);

// Post-process passes. The game tonemaps the 3D picture into eaid://ego_package/tonemap_output.image,
// grades and encodes it into the back buffer, then draws the HUD into the back buffer. The passes run
// once a frame between the picture and the HUD, so the HUD draws over their result as the game drew it,
// translucent panels included. The game records a frame's lists in parallel, so where they run is
// decided per back buffer: each one is fresh from the barrier that makes it a render target until the
// passes run on it.
//
// - On track while the HUD reads the finished picture (paused, menus over the race), the passes run at
//   the first barrier of a list that moves the back buffer from render target to a read state.
// - Otherwise, in the game's list where it moves eaid://ego_package/hudless_backbuffer.image out of UAV
//   while the back buffer it made a render target is still one (its HUD-less step).
// - Where the list that made the picture ends with the back buffer a render target and the step done, in
//   a list of the core's own submitted right after it.
// - Where it ends inside that step, the list that closes the step can draw the rest of the picture into
//   the back buffer first: the passes run in that list while it has the back buffer bound (the split
//   step), else in a list of the core's own right after it, or right before the list that takes the back
//   buffer out of render target for the HUD and Present when none closed the step by then; at Present only
//   when the core saw no such list (postfx::BackPlace).
//
// In the game's lists the core records its passes before the game's barrier and then puts back the
// pipeline, descriptor heaps, compute bindings and graphics descriptor tables it changed
// (d3d12hooks::list_state). The passes copy the back buffer, run as DXIL compute shaders
// (swofty/postfx.hlsli) through a slot's own textures, and copy the result back; with frame generation
// on, the result goes into the game's HUD-less copy too. Each slot has its own fence and working
// textures, so frames the GPU runs at once on different queues never share them; every texture rests
// in NON_PIXEL_SHADER_RESOURCE between recordings. A pass whose pipeline cannot be made, or whose GPU
// time stays above its budget, is turned off and logged; SWOFTY_POSTFX=0 or a swofty/postfx.off file
// turns every pass off.

namespace {

using namespace postfx;

constexpr wchar_t TONEMAP_NAME[] = L"eaid://ego_package/tonemap_output.image";
constexpr wchar_t BACKBUFFER_NAME[] = L"BackBuffer";
constexpr wchar_t HUDLESS_NAME[] = L"eaid://ego_package/hudless_backbuffer.image";
constexpr wchar_t DEPTH_NAME[] = L"eaid://ego_package/gbuffer_depth_stencil.image";
constexpr wchar_t UI_PREFIX[] = L"eaid://ui_package/";
using postfx::IN_NONE;
using postfx::IN_ORIGINAL;
using postfx::IN_PREVIOUS;

struct Mark {
    bool tonemap = false, ui = false, left_rt = false, closes = false;
    int slot = -1;
    ID3D12Resource *target = nullptr, *hudless = nullptr, *depth = nullptr, *ran_back = nullptr, *drew = nullptr;
    D3D12_RESOURCE_STATES state = D3D12_RESOURCE_STATE_COMMON, hudless_state = D3D12_RESOURCE_STATE_COMMON,
                          depth_state = D3D12_RESOURCE_STATE_COMMON;
    std::vector<D3D12_RESOURCE_BARRIER> seen;
    std::string order;
};


// The core's textures for one picture size: each slot's working set, last frame's result, and the
// addons' persistent textures and images.
// A slot's working textures: the picture before the passes, the two the passes write in turn, and
// the addon textures that are neither persistent nor images. Each slot has its own, so passes of two
// frames that the GPU runs at once on different queues never write the same texture.
struct Work {
    ID3D12Resource *orig = nullptr, *pic[2] = {};
    std::vector<ID3D12Resource *> tex;
    bool clear = false;
};

struct Frame {
    D3D12_RESOURCE_DESC desc{};
    DXGI_FORMAT view = DXGI_FORMAT_UNKNOWN;
    Work work[FRAMES];
    ID3D12Resource *prev = nullptr;
    ID3D12DescriptorHeap *heap = nullptr, *cpu = nullptr;
    UINT stride = 0;
    bool prev_valid = false;
    uint32_t prev_serial = 0;
    unsigned prev_frame = 0;
};

ResourceWatch watched;

std::recursive_mutex lock;
std::vector<RenderPass> passes;
std::vector<PassTexture> textures;
std::vector<AddonToggle> toggles;
bool uses_previous, legacy_previous;
size_t legacy_passes, legacy_textures;
std::vector<postfx::EffectDecl> effects;
postfx::ActiveSpec active;
postfx::Plan plan;
std::unordered_map<std::string, ID3D12PipelineState *> effect_psos;
std::wstring active_file;
FILETIME active_at{};
ULONGLONG active_checked;
bool active_told, shaders_off, shader_key_down;
int shader_vk;
ID3D12Resource *depth_res;
D3D12_RESOURCE_STATES depth_state;
bool depth_logged;
std::unordered_map<ID3D12Resource *, std::wstring> names;
std::unordered_map<ID3D12GraphicsCommandList *, Mark> marks;
unsigned order_left;
BurstProbe burst;
// Each back buffer's use for one frame: fresh from the barrier that makes it a render target until the
// passes run on it, the frame they last ran on it, and the depth buffer as the picture's list left it.
// The lists of a frame are recorded in parallel, so the list that draws the rest of a split picture can
// run the passes before the picture's list makes the back buffer a render target; that barrier then
// leaves it not fresh. The depth buffer here and in
// depth_res is held (postfx::hold): the HUD's list and the lists between read it a frame later, after
// the game may have freed it.
struct BackUse {
    ID3D12Resource *back = nullptr;
    postfx::BackPlace place;
    ID3D12Resource *depth = nullptr;
    D3D12_RESOURCE_STATES depth_state = D3D12_RESOURCE_STATE_COMMON;
    unsigned seen = 0;
};
BackUse back_uses[6];
// The back buffers' render target views by descriptor address, and the latest views made, so a view
// made before its back buffer was named is still found. A list that binds one of them draws into that
// back buffer: the game sometimes ends the picture's list inside the HUD-less step and draws the rest
// of the picture in the next list, and that list closes the step with the back buffer bound.
struct ViewAt {
    std::atomic<SIZE_T> at{0};
    std::atomic<ID3D12Resource *> res{nullptr};
};
ViewAt back_views[32], recent_views[64];
std::atomic<unsigned> recent_next;
std::atomic<SIZE_T> views_lo{SIZE_T(-1)}, views_hi{0};
void pin_view(SIZE_T at, ID3D12Resource *res);
unsigned back_seen;
postfx::HudRead hud_read;


// Lets go of the held depth buffers other than keep: all of them when the picture changes, the old ones
// when the game names a new depth buffer, and only renamed when the game gives a held one another name.
void drop_depths(ID3D12Resource *keep, ID3D12Resource *renamed = nullptr)
{
    auto drop = [&](ID3D12Resource *&d) {
        if (d && (keep ? d != keep : !renamed || d == renamed))
            postfx::hold(d, nullptr);
    };
    for (auto &u : back_uses)
        drop(u.depth);
    drop(depth_res);
}

BackUse &back_use(ID3D12Resource *b)
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
bool diag_done, killed, broken, warned_busy, hudless_logged, saw_back, tonemap_named, state_armed;
std::wstring off_file, trace_file, swofty_root_dir;

GpuContext gpu;
UINT levels_of(const PassTexture &t);
Slot slots[FRAMES];
// Counts the times the core made its heap or textures, so slots know their descriptor tables are old.
unsigned textures_made;
UINT slot_at;
ID3D12QueryHeap *queries;
ID3D12Resource *query_readback;
double ticks_per_ms;
Frame frame;
RetireQueue retired;
std::atomic<bool> fg_on{true};
ULONGLONG started, last_check, live_at, frame_at;
std::atomic<ULONGLONG> present_at;
postfx::FrameClock frame_clock;
float frametime;
unsigned frame_count, ran_total;

constexpr unsigned NAMED_LINES = 80;
constexpr const wchar_t *INTERESTING[] = {L"tonemap", L"output", L"final", L"hudless", L"backbuffer", L"pathtrac",
                                          L"_pt", L"pt_", L"denois", L"upscal", L"composite", L"present",
                                          L"display", L"post", L"lut", L"bloom", L"exposure"};
std::vector<std::wstring> named_seen;

FrameReport report;
ULONGLONG trace_checked;

PictureCapture capture;
ClipRecorder clip;

std::string name_of(ID3D12Resource *r)
{
    auto it = names.find(r);
    return it == names.end() ? std::string("?") : runtime::narrow(it->second);
}

bool killed_now()
{
    ULONGLONG now = GetTickCount64();
    if (now - last_check < 1000)
        return killed;
    last_check = now;
    bool was = killed;
    killed = GetFileAttributesW(off_file.c_str()) != INVALID_FILE_ATTRIBUTES;
    if (killed != was)
        swofty::log(killed ? "[postfx] swofty/postfx.off exists; every pass is off" : "[postfx] swofty/postfx.off is gone; passes run again");
    return killed;
}



// The device comes from the game's resource, never from the queue: a queue can be Streamline's proxy,
// whose device makes proxy command lists that the hooked ExecuteCommandLists cannot take.
bool init(ID3D12Resource *res)
{
    if (FAILED(res->GetDevice(IID_PPV_ARGS(&gpu.device))))
        return false;
    if (!gpu.make_root()) {
        swofty::log("[postfx] could not make the passes' root signature; post-processing is off");
        return false;
    }
    gpu.make_mip_pso();
    for (auto &s : slots) {
        if (FAILED(gpu.device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&s.fence))) ||
            FAILED(gpu.device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&s.alloc))) ||
            FAILED(gpu.device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, s.alloc, nullptr, IID_PPV_ARGS(&s.list))))
            return false;
        s.list->Close();
    }
    D3D12_QUERY_HEAP_DESC qd{};
    qd.Type = D3D12_QUERY_HEAP_TYPE_TIMESTAMP;
    qd.Count = FRAMES * MAX_PASSES * 2;
    if (SUCCEEDED(gpu.device->CreateQueryHeap(&qd, IID_PPV_ARGS(&queries))))
        query_readback = gpu.buffer(D3D12_HEAP_TYPE_READBACK, qd.Count * 8, D3D12_RESOURCE_STATE_COPY_DEST);
    for (auto &p : passes) {
        if (p.inst >= 0)
            continue;
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = gpu.root;
        pd.CS.pShaderBytecode = p.dxil.data();
        pd.CS.BytecodeLength = p.dxil.size();
        HRESULT hr = gpu.device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&p.pso));
        if (FAILED(hr)) {
            p.off = true;
            swofty::log("[postfx] %s's pass %s is off: its pipeline could not be made (0x%08lx)", p.addon.c_str(),
                        p.name.c_str(), hr);
        } else
            swofty::log("[postfx] %s's pass %s is ready (%s, %zu bytes of DXIL)", p.addon.c_str(), p.name.c_str(),
                        p.shader.c_str(), p.dxil.size());
    }
    started = GetTickCount64();
    return true;
}

// The GPU clock rate comes from the first queue the passes run on.
void read_frequency(ID3D12CommandQueue *q)
{
    UINT64 freq = 0;
    if (!ticks_per_ms && query_readback && q && SUCCEEDED(q->GetTimestampFrequency(&freq)) && freq)
        ticks_per_ms = double(freq) / 1000.0;
}

void retire(std::vector<IUnknown *> objects)
{
    retired.retire(std::move(objects), frame_count);
}

bool read_tex_file(PassTexture &t)
{
    std::string bytes = runtime::read_file(runtime::under_swofty(t.file), size_t(64) << 20);
    uint32_t head[4] = {};
    if (bytes.size() < 16 || memcmp(bytes.data(), "SWTX", 4) != 0)
        return false;
    memcpy(head, bytes.data(), 16);
    t.width = head[1];
    t.height = head[2];
    if (!t.width || !t.height || t.width > 8192 || t.height > 8192 || bytes.size() < 16 + size_t(t.width) * t.height * 4)
        return false;
    t.pixels = bytes.substr(16, size_t(t.width) * t.height * 4);
    t.format = DXGI_FORMAT_R8G8B8A8_UNORM;
    return true;
}

UINT mip_base()
{
    return FRAMES * MAX_PASSES * TABLE + UINT(textures.size()) * (FRAMES + 1) + 1;
}

bool make_heaps()
{
    D3D12_DESCRIPTOR_HEAP_DESC hd{};
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    hd.NumDescriptors = mip_base() + FRAMES * MIP_STEPS * TABLE;
    hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    D3D12_DESCRIPTOR_HEAP_DESC cd = hd;
    cd.NumDescriptors = UINT(textures.size()) * (FRAMES + 1) + 1;
    cd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;
    if (FAILED(gpu.device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&frame.heap))) ||
        FAILED(gpu.device->CreateDescriptorHeap(&cd, IID_PPV_ARGS(&frame.cpu))))
        return false;
    frame.stride = gpu.device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    return true;
}

// Makes the textures for a picture of desc's size; the ones being replaced go once the GPU is past them.
bool make_frame(const D3D12_RESOURCE_DESC &d)
{
    DXGI_FORMAT view = view_format(d.Format);
    if (view == DXGI_FORMAT_UNKNOWN) {
        swofty::log("[postfx] the game's picture has format %d, which the passes do not take; post-processing is off",
                    int(d.Format));
        return false;
    }
    std::vector<IUnknown *> old = {frame.prev, frame.heap, frame.cpu};
    for (auto &wk : frame.work) {
        old.insert(old.end(), {wk.orig, wk.pic[0], wk.pic[1]});
        old.insert(old.end(), wk.tex.begin(), wk.tex.end());
    }
    drop_depths(nullptr);
    frame = Frame{};
    textures_made++;
    frame.desc = d;
    frame.view = view;
    UINT w = UINT(d.Width), h = d.Height;
    if (uses_previous)
        frame.prev = gpu.texture(w, h, view, false);
    for (auto &t : textures) {
        if (!t.file.empty() && t.res)
            continue;
        postfx::Size s = postfx::texture_size(t.scale, t.width, t.height, w, h);
        t.w = s.w;
        t.h = s.h;
        if (!t.persistent && t.file.empty())
            continue;
        old.push_back(t.res);
        t.res = gpu.texture(t.w, t.h, t.format, t.file.empty(), levels_of(t));
        t.clear = t.file.empty();
        if (!t.res) {
            swofty::log("[postfx] texture %s (%ux%u, format %d) could not be made; post-processing is off",
                        t.name.c_str(), t.w, t.h, int(t.format));
            return false;
        }
    }
    retire(std::move(old));
    if ((uses_previous && !frame.prev) || !make_heaps())
        return false;
    capture.remake(gpu, d, retired, frame_count);
    swofty::log("[postfx] passes run on the game's %llux%u picture (format %d) with %zu texture(s) of their own",
                (unsigned long long)d.Width, d.Height, int(d.Format), textures.size());
    return true;
}

UINT levels_of(const PassTexture &t)
{
    return t.file.empty() ? postfx::mip_levels(t.w, t.h, t.mips) : 1;
}

bool shared(const PassTexture &t)
{
    return t.persistent || !t.file.empty();
}

// A slot's working set, made the first time the slot runs on this picture size.
bool ensure_work(int k)
{
    Work &wk = frame.work[k];
    if (wk.orig)
        return true;
    textures_made++;
    UINT w = UINT(frame.desc.Width), h = frame.desc.Height;
    wk.orig = gpu.texture(w, h, frame.view, false);
    wk.pic[0] = gpu.texture(w, h, frame.view, true);
    wk.pic[1] = gpu.texture(w, h, frame.view, true);
    wk.tex.assign(textures.size(), nullptr);
    bool ok = wk.orig && wk.pic[0] && wk.pic[1];
    for (size_t i = 0; i < textures.size() && ok; i++)
        if (!shared(textures[i]))
            ok = (wk.tex[i] = gpu.texture(textures[i].w, textures[i].h, textures[i].format, true, levels_of(textures[i]))) != nullptr;
    wk.clear = true;
    if (!ok)
        swofty::log("[postfx] the working textures of slot %d could not be made; post-processing is off", k);
    return ok;
}

ID3D12Resource *tex_of(size_t i, int k)
{
    return shared(textures[i]) ? textures[i].res : frame.work[k].tex[i];
}

D3D12_CPU_DESCRIPTOR_HANDLE cpu_at(ID3D12DescriptorHeap *h, UINT i)
{
    return {h->GetCPUDescriptorHandleForHeapStart().ptr + SIZE_T(i) * frame.stride};
}

D3D12_GPU_DESCRIPTOR_HANDLE gpu_at(ID3D12DescriptorHeap *h, UINT i)
{
    return {h->GetGPUDescriptorHandleForHeapStart().ptr + UINT64(i) * frame.stride};
}


// The bytes each addon's code gave one of its passes (SwoftyCorePostfxSetData). Every frame a pass reads the latest
// as t10, from its slot's own upload buffer.
std::mutex data_lock;
postfx::DataStore pass_data;
std::atomic<bool> any_data;
bool data_logged;
std::atomic<bool> device_armed;  // the device hooks armed: passes can run at all

// Packs the data of the live passes into slot s's upload buffer (offsets[i] and sizes[i] for live[i]), growing
// the buffer when they don't fit. Sizes are 0 where a pass has none. Under lock, for a slot the GPU is done with.
void pack_data(Slot &s, const std::vector<int> &live, std::vector<UINT> &offsets, std::vector<UINT> &sizes)
{
    offsets.assign(live.size(), 0);
    sizes.assign(live.size(), 0);
    if (!any_data.load(std::memory_order_relaxed))
        return;
    std::lock_guard<std::mutex> g(data_lock);
    std::vector<const std::string *> found(live.size(), nullptr);
    UINT total = 0;
    for (size_t i = 0; i < live.size(); i++) {
        const RenderPass &p = passes[size_t(live[i])];
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
        if (FAILED(gpu.device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ,
                                                   nullptr, IID_PPV_ARGS(&b))) ||
            FAILED(b->Map(0, nullptr, &at))) {
            release(b);
            sizes.assign(live.size(), 0);
            return;
        }
        retire({s.data});
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


UINT mip_steps_used;

// Fills levels 1.. of r from level 0, which the caller has left in HOME: each level is written as a UAV, then
// moved to HOME before the next one reads it.
void build_mips(ID3D12GraphicsCommandList *l, ID3D12Resource *r, DXGI_FORMAT f, UINT w, UINT h, UINT levels, int k)
{
    if (!gpu.mip_pso || levels < 2)
        return;
    l->SetPipelineState(gpu.mip_pso);
    for (UINT level = 1; level < levels; level++) {
        if (mip_steps_used >= MIP_STEPS)
            return;
        UINT at = mip_base() + (UINT(k) * MIP_STEPS + mip_steps_used++) * TABLE;
        for (UINT i = 0; i < 2 + INPUTS; i++)
            if (i == 2)
                gpu.srv(r, f, cpu_at(frame.heap, at + i), level - 1, 1);
            else
                gpu.srv(nullptr, f, cpu_at(frame.heap, at + i));
        gpu.data_srv(nullptr, 0, 0, cpu_at(frame.heap, at + DATA_AT));
        gpu.uav(r, f, cpu_at(frame.heap, at + TABLE - 1), level);
        postfx::Size to = postfx::mip_size(w, h, level);
        transition_level(l, r, level, HOME, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        l->SetComputeRootDescriptorTable(1, gpu_at(frame.heap, at));
        l->Dispatch(postfx::groups(to.w, 8), postfx::groups(to.h, 8), 1);
        transition_level(l, r, level, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
    }
}


// The depth buffer as the game leaves each of its passes that write it. On track the passes run in the
// HUD's list, which never moves the depth buffer, so its state there is unknown; instead the core copies
// the buffer right before the game's own barrier that takes it out of DEPTH_WRITE, where its state is
// that barrier's StateBefore, exactly (copy_depth). The last copy of a frame holds the frame's finished
// depth. The copy rests in NON_PIXEL_SHADER_RESOURCE, and passes read it wherever the place they run
// leaves no readable depth of its own.
std::atomic<ID3D12Resource *> depth_copy;
std::atomic<unsigned> depth_copy_frame;  // frame_count when a copy was last recorded
std::atomic<unsigned> depth_copy_until;  // copies are made until this frame: passes ran without readable depth
constexpr unsigned DEPTH_COPY_FRAMES = 120;
ID3D12Resource *depth_copy_of;           // the game's buffer the copy matches, compared, not held; under lock
D3D12_RESOURCE_DESC depth_copy_desc;
bool depth_copy_logged, depth_copy_failed;

// The copy for the game's depth buffer src, made or remade when the game's buffer changed. Under lock.
ID3D12Resource *depth_copy_for(ID3D12Resource *src, const D3D12_RESOURCE_DESC &d)
{
    ID3D12Resource *have = depth_copy.load(std::memory_order_acquire);
    const D3D12_RESOURCE_DESC &h = depth_copy_desc;
    if (have && depth_copy_of == src && h.Width == d.Width && h.Height == d.Height && h.Format == d.Format &&
        h.MipLevels == d.MipLevels && h.DepthOrArraySize == d.DepthOrArraySize)
        return have;
    if (!gpu.device || depth_copy_failed)
        return nullptr;
    D3D12_RESOURCE_DESC cd = d;
    cd.Flags &= ~D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE;
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    ID3D12Resource *r = nullptr;
    if (FAILED(gpu.device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &cd,
                                               D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, nullptr,
                                               IID_PPV_ARGS(&r)))) {
        depth_copy_failed = true;
        swofty::log("[postfx] could not make a copy of the game's depth buffer (%llux%u, format %d); passes read "
                    "depth only where the game leaves it readable",
                    static_cast<unsigned long long>(d.Width), d.Height, int(d.Format));
        return nullptr;
    }
    retire({depth_copy.exchange(r, std::memory_order_acq_rel)});
    depth_copy_of = src;
    depth_copy_desc = d;
    if (!depth_copy_logged) {
        depth_copy_logged = true;
        swofty::log("[postfx] the core copies the game's depth buffer (%llux%u, format %d) where the game stops "
                    "writing it, for passes that run where it isn't readable",
                    static_cast<unsigned long long>(d.Width), d.Height, int(d.Format));
    }
    return r;
}


std::string describe(const RenderPass &p)
{
    return p.inst >= 0 ? postfx::pass_title(active.name, p.effect, p.label, p.name) : p.addon + "'s pass " + p.name;
}

void read_times(Slot &s)
{
    if (!ticks_per_ms || !s.passes)
        return;
    UINT base = UINT(&s - slots) * MAX_PASSES * 2;
    D3D12_RANGE r{SIZE_T(base) * 8, SIZE_T(base + s.passes * 2) * 8};
    uint64_t *t = nullptr;
    if (FAILED(query_readback->Map(0, &r, reinterpret_cast<void **>(&t))))
        return;
    for (UINT i = 0; i < s.passes; i++) {
        if (size_t(s.pass_of[i]) >= passes.size())
            continue;
        RenderPass &p = passes[size_t(s.pass_of[i])];
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
                            describe(p).c_str(), avg, p.budget_ms);
            }
        } else
            p.over = 0;
        if (ran_total <= 4 * BUDGET_FRAMES)
            swofty::log("[postfx] %s: %.3f ms of GPU time a frame on average, %.3f ms at most", describe(p).c_str(), avg,
                        p.ms_peak);
        p.ms_sum = 0;
        p.ms_peak = 0;
        p.ms_frames = 0;
    }
    D3D12_RANGE none{0, 0};
    query_readback->Unmap(0, &none);
}

// Where a frame's passes are recorded: into the game's list at its HUD-less step, or into a slot's own
// list between the game's lists. The states are the resources' states at that point of the frame.
struct Where {
    ID3D12GraphicsCommandList *list;
    bool game;
    ID3D12Resource *back;
    D3D12_RESOURCE_STATES back_state;
    ID3D12Resource *hudless;
    D3D12_RESOURCE_STATES hudless_state;
    ID3D12Resource *depth;
    D3D12_RESOURCE_STATES depth_state;
    const char *path;
};

// A slot whose GPU work is done. The game records frames ahead, so the slots of lists it has not
// submitted yet are skipped rather than waited for. A slot recorded into a game list that was never
// submitted is free again after two seconds: the game reset that list, so the core's work in it never
// runs.
int pick_slot()
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
    report.stats.busy++;
    report.trace.tried = postfx::IDLE_BUSY;
    report.trace.why = postfx::idle_text(report.trace.tried);
    return -1;
}

bool addon_off(const std::string &id)
{
    return std::any_of(toggles.begin(), toggles.end(), [&](const AddonToggle &t) { return t.off && t.addon_id == id; });
}

ID3D12PipelineState *effect_pso(const std::string &path)
{
    auto it = effect_psos.find(path);
    if (it != effect_psos.end())
        return it->second;
    ID3D12PipelineState *pso = nullptr;
    std::string bytes = runtime::read_file(runtime::under_swofty(path), size_t(4) << 20);
    if (bytes.size() < 32 || memcmp(bytes.data(), "DXBC", 4) != 0)
        swofty::log("[shaders] %s %s", path.c_str(), bytes.empty() ? "has no DXIL" : "is not a DXIL container");
    else {
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = gpu.root;
        pd.CS.pShaderBytecode = bytes.data();
        pd.CS.BytecodeLength = bytes.size();
        HRESULT hr = gpu.device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pso));
        if (FAILED(hr))
            swofty::log("[shaders] the pipeline of %s could not be made (0x%08lx)", path.c_str(), hr);
    }
    effect_psos[path] = pso;
    return pso;
}

void drop_instance(int inst)
{
    for (auto &p : passes)
        if (p.inst == inst)
            p.off = true;
    swofty::log("[shaders] instance %d (%s) is off: a pipeline of its effect could not be made", inst,
                inst >= 0 && size_t(inst) < plan.instances.size()
                    ? postfx::instance_name(plan.instances[size_t(inst)].effect, plan.instances[size_t(inst)].label).c_str()
                    : "?");
}

std::vector<int> live_passes(const view::State &v, unsigned *idle = nullptr)
{
    std::vector<int> live;
    if (shaders_off)
        return live;
    auto consider = [&](size_t i) {
        RenderPass &p = passes[i];
        if (p.inst >= 0 && !p.pso && !p.off) {
            p.pso = effect_pso(p.shader);
            if (!p.pso)
                drop_instance(p.inst);
        }
        const char *why = p.off            ? "off"
                          : p.ruled_off    ? "a rule turned it off"
                          : !p.on          ? "its option turns it off"
                          : addon_off(p.addon_id) ? "its addon's key turned it off"
                                           : postfx::ineligible(p.when, v.kind, v.flags, v.valid);
        if (!why) {
            live.push_back(int(i));
            return;
        }
        if (idle)
            *idle |= 1u << (p.off                   ? postfx::IDLE_PASS_OFF
                            : p.ruled_off           ? postfx::IDLE_RULE
                            : !p.on                 ? postfx::IDLE_OPTION
                            : addon_off(p.addon_id) ? postfx::IDLE_ADDON_KEY
                                                    : postfx::IDLE_VIEW);
        if (!p.off && report.tracing())
            report.trace.skipped += (report.trace.skipped.empty() ? "" : ", ") + p.name + " (" + why + ")";
    };
    for (size_t i = legacy_passes; i < passes.size() && live.size() < MAX_PASSES; i++)
        consider(i);
    for (size_t i = 0; i < legacy_passes && live.size() < MAX_PASSES; i++)
        consider(i);
    return live;
}

ID3D12Resource *input_res(int in, int k, DXGI_FORMAT &f)
{
    f = frame.view;
    if (in == IN_ORIGINAL)
        return frame.work[k].orig;
    if (in == IN_PREVIOUS)
        return frame.prev;
    if (in >= 0 && size_t(in) < textures.size()) {
        f = textures[size_t(in)].format;
        return tex_of(size_t(in), k);
    }
    f = DXGI_FORMAT_R8G8B8A8_UNORM;
    return nullptr;
}

void upload(ID3D12GraphicsCommandList *l, PassTexture &t)
{
    D3D12_RESOURCE_DESC d = t.res->GetDesc();
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    UINT rows = 0;
    UINT64 row_bytes = 0, total = 0;
    gpu.device->GetCopyableFootprints(&d, 0, 1, 0, &fp, &rows, &row_bytes, &total);
    if (!t.upload && !(t.upload = gpu.buffer(D3D12_HEAP_TYPE_UPLOAD, total, D3D12_RESOURCE_STATE_GENERIC_READ)))
        return;
    uint8_t *mapped = nullptr;
    if (FAILED(t.upload->Map(0, nullptr, reinterpret_cast<void **>(&mapped))))
        return;
    for (UINT y = 0; y < t.height; y++)
        memcpy(mapped + fp.Offset + size_t(y) * fp.Footprint.RowPitch, t.pixels.data() + size_t(y) * t.width * 4,
               size_t(t.width) * 4);
    t.upload->Unmap(0, nullptr);
    D3D12_TEXTURE_COPY_LOCATION to{}, from{};
    to.pResource = t.res;
    to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    from.pResource = t.upload;
    from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    from.PlacedFootprint = fp;
    transition(l, t.res, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    transition(l, t.res, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
    t.uploaded = true;
    t.pixels.clear();
    t.pixels.shrink_to_fit();
    retire({t.upload});
    t.upload = nullptr;
}


// Records this frame's passes at w into slot k's tables. Returns false when nothing was recorded.
bool record(const Where &w, int k)
{
    if (!gpu.device && !init(w.back)) {
        broken = true;
        return false;
    }
    if (gpu.device->GetDeviceRemovedReason() != S_OK) {
        broken = true;
        swofty::log("[postfx] the GPU device was removed; post-processing is off");
        return false;
    }
    capture.land(slots, frame.desc, frame.view);
    retired.free(frame_count);
    Slot &s = slots[k];
    postfx::hold(s.depth, w.depth);
    read_times(s);
    s.passes = 0;
    D3D12_RESOURCE_DESC d = w.back->GetDesc();
    if (!frame.heap || d.Width != frame.desc.Width || d.Height != frame.desc.Height || d.Format != frame.desc.Format)
        if (!make_frame(d)) {
            broken = true;
            return false;
        }
    if (!ensure_work(k)) {
        broken = true;
        return false;
    }
    if (s.views_made != textures_made) {
        for (auto &view : s.views)
            view.set = false;
        s.views_made = textures_made;
    }
    Work &wk = frame.work[k];
    view::State v = view::current();
    report.trace.view = v;
    unsigned idle = 0;
    std::vector<int> live = live_passes(v, &idle);
    report.trace.live = unsigned(live.size());
    if (live.empty()) {
        report.trace.tried = postfx::idle_of_passes(idle);
        report.trace.why = postfx::idle_text(report.trace.tried);
        return false;
    }
    d3d12hooks::ListState saved;
    if (w.game && !d3d12hooks::list_state(w.list, saved)) {
        report.stats.unknown_state++;
        report.trace.tried = postfx::IDLE_LIST_STATE;
        report.trace.why = postfx::idle_text(report.trace.tried);
        return false;
    }
    ID3D12GraphicsCommandList *l = w.list;
    ULONGLONG now = GetTickCount64();
    capture.arm(now, ran_total);
    if (frame.prev_serial != v.serial) {
        frame.prev_valid = false;
        for (auto &t : textures)
            t.clear |= t.persistent;
    }
    bool had_previous = frame.prev && frame.prev_valid && frame.prev_serial == v.serial;
    bool has_previous = had_previous && frame_count - frame.prev_frame <= 1;
    report.stats.no_previous += !has_previous;
    report.stats.prev_gap += had_previous && !has_previous;
    UINT pw = UINT(d.Width), ph = d.Height;

    ClipRecorder::Entry *clip_entry = clip.begin(gpu, d, k, frame.view, retired, frame_count);
    transition(l, w.back, w.back_state, D3D12_RESOURCE_STATE_COPY_SOURCE);
    transition(l, wk.orig, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyResource(wk.orig, w.back);
    if (clip_entry)
        clip.copy(l, w.back, *clip_entry);
    if (capture.armed())
        capture.copy(l, w.back, 0);
    transition(l, wk.orig, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
    for (auto &t : textures)
        if (!t.file.empty() && !t.uploaded && t.res)
            upload(l, t);
    l->SetComputeRootSignature(gpu.root);
    l->SetDescriptorHeaps(1, &frame.heap);
    mip_steps_used = 0;
    UINT clear_at = FRAMES * MAX_PASSES * TABLE, ntex = UINT(textures.size());
    for (size_t i = 0; i < textures.size(); i++) {
        PassTexture &t = textures[i];
        bool own = !shared(t);
        ID3D12Resource *r = tex_of(i, k);
        if (!r || !(own ? wk.clear : t.clear))
            continue;
        UINT at = own ? ntex * (1 + UINT(k)) + UINT(i) : UINT(i);
        gpu.uav(r, t.format, cpu_at(frame.heap, clear_at + at));
        gpu.uav(r, t.format, cpu_at(frame.cpu, at));
        transition(l, r, HOME, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        const float zero[4] = {};
        l->ClearUnorderedAccessViewFloat(gpu_at(frame.heap, clear_at + at), cpu_at(frame.cpu, at), r, zero, 0, nullptr);
        transition(l, r, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
        if (levels_of(t) > 1)
            build_mips(l, r, t.format, t.w, t.h, levels_of(t), k);
        if (!own)
            t.clear = false;
    }
    wk.clear = false;

    uint32_t c[CONSTANTS] = {};
    float t_now = float(double(now - started) / 1000.0);
    memcpy(&c[4], &t_now, 4);
    c[5] = frame_count;
    c[6] = format_kind(frame.view);
    memcpy(&c[27], &frametime, 4);
    c[28] = pw;
    c[29] = ph;
    c[30] = frame_count * 2654435761u + 0x9e3779b9u;
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
        depth_copy_until.store(frame_count + DEPTH_COPY_FRAMES, std::memory_order_relaxed);
        ID3D12Resource *dc = depth_copy.load(std::memory_order_acquire);
        if (dc && frame_count - depth_copy_frame.load(std::memory_order_relaxed) <= 2) {
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
        RenderPass &p = passes[size_t(index)];
        UINT data_size = data_sizes[live_i], data_offset = data_offsets[live_i];
        live_i++;
        ID3D12Resource *out = p.output < 0 ? wk.pic[next] : tex_of(size_t(p.output), k);
        DXGI_FORMAT of = p.output < 0 ? frame.view : textures[size_t(p.output)].format;
        UINT ow = p.output < 0 ? pw : textures[size_t(p.output)].w, oh = p.output < 0 ? ph : textures[size_t(p.output)].h;
        UINT at = (UINT(k) * MAX_PASSES + s.passes) * TABLE;
        auto *view = &s.views[s.passes * TABLE];
        auto put_srv = [&](UINT i, ID3D12Resource *r, DXGI_FORMAT f) {
            if (postfx::rewrite(view[i], r, f))
                gpu.srv(r, f, cpu_at(frame.heap, at + i));
        };
        put_srv(0, cur, frame.view);
        put_srv(1, depth, depth_fmt);
        for (UINT i = 0; i < INPUTS; i++) {
            DXGI_FORMAT f;
            ID3D12Resource *r = input_res(p.inputs[i], k, f);
            put_srv(2 + i, r, f);
        }
        if (data_size) {  // written every frame: the bytes move within the buffer
            gpu.data_srv(s.data, data_offset, data_size, cpu_at(frame.heap, at + DATA_AT));
            view[DATA_AT] = {};
        } else if (postfx::rewrite(view[DATA_AT], static_cast<ID3D12Resource *>(nullptr), DXGI_FORMAT_UNKNOWN)) {
            gpu.data_srv(nullptr, 0, 0, cpu_at(frame.heap, at + DATA_AT));
        }
        c[postfx::DATA_SIZE_AT] = data_size;
        if (postfx::rewrite(view[TABLE - 1], out, of))
            gpu.uav(out, of, cpu_at(frame.heap, at + TABLE - 1));
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
        l->SetComputeRootDescriptorTable(1, gpu_at(frame.heap, at));
        l->Dispatch(postfx::groups(ow, p.threads[0]), postfx::groups(oh, p.threads[1]), 1);
        if (timed)
            l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, qbase + s.passes * 2 + 1);
        transition(l, out, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, HOME);
        if (p.output >= 0 && levels_of(textures[size_t(p.output)]) > 1)
            build_mips(l, out, of, ow, oh, levels_of(textures[size_t(p.output)]), k);
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
    if (capture.armed())
        capture.copy(l, w.back, 1);
    transition(l, w.back, D3D12_RESOURCE_STATE_COPY_SOURCE, w.back_state);
    if (w.hudless && cur != wk.orig && fg_on.load()) {
        D3D12_RESOURCE_DESC hd = w.hudless->GetDesc();
        if (hd.Width == d.Width && hd.Height == d.Height && view_format(hd.Format) == frame.view) {
            transition(l, w.hudless, w.hudless_state, D3D12_RESOURCE_STATE_COPY_DEST);
            l->CopyResource(w.hudless, cur);
            transition(l, w.hudless, D3D12_RESOURCE_STATE_COPY_DEST, w.hudless_state);
            if (!hudless_logged)
                swofty::log("[postfx] the game's HUD-less copy of the picture gets the passes too, for Frame Generation");
            hudless_logged = true;
        }
    }
    if (frame.prev) {
        transition(l, frame.prev, HOME, D3D12_RESOURCE_STATE_COPY_DEST);
        l->CopyResource(frame.prev, cur);
        transition(l, frame.prev, D3D12_RESOURCE_STATE_COPY_DEST, HOME);
        frame.prev_valid = true;
        frame.prev_serial = v.serial;
        frame.prev_frame = frame_count;
    }
    transition(l, cur, D3D12_RESOURCE_STATE_COPY_SOURCE, HOME);
    if (w.game)
        d3d12hooks::restore_list_state(l, saved);

    s.pending = true;
    s.armed_at = now;
    capture.ran(k, now);
    report.trace.runs++;
    report.stats.runs++;
    report.trace.ran = s.passes;
    report.trace.path = w.path;
    report.trace.back = w.back;
    report.trace.why = nullptr;
    if (++ran_total == 1) {
        capture.first_run(now);
        swofty::log("[postfx] the picture now goes through %u pass(es) before the HUD, %s", s.passes,
                    w.game ? "inside the game's list at its HUD-less step" : "in a list between the game's lists");
    }
    return true;
}

// The list of the core's own for a frame whose picture list ends with the back buffer still a render
// target and draws no HUD, for one whose HUD-less step another list closed, or at Present. True when it ran.
bool run_between(ID3D12CommandQueue *q, ID3D12Resource *back, D3D12_RESOURCE_STATES state, ID3D12Resource *hudless,
                 D3D12_RESOURCE_STATES hudless_state, const char *path)
{
    if (!gpu.device && !init(back)) {
        broken = true;
        return false;
    }
    int k = pick_slot();
    if (k < 0)
        return false;
    Slot &s = slots[k];
    if (FAILED(s.alloc->Reset()) || FAILED(s.list->Reset(s.alloc, nullptr))) {
        broken = true;
        swofty::log("[postfx] could not record the passes' command list; post-processing is off");
        return false;
    }
    Where w{s.list, false, back, state, hudless, hudless_state, depth_res, depth_state, path};
    bool made = record(w, k);
    if (FAILED(s.list->Close())) {
        broken = true;
        s.pending = false;
        swofty::log("[postfx] the passes' command list did not close; post-processing is off");
        return false;
    }
    if (!made)
        return false;
    read_frequency(q);
    ID3D12CommandList *one[] = {s.list};
    d3d12hooks::exec_original(q, 1u, one);
    q->Signal(s.fence, ++s.fence_value);
    s.pending = false;
    capture.signalled(k, s.fence_value);
    return true;
}

void log_diag(ID3D12GraphicsCommandList *list, const Mark &m)
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

bool interesting(LPCWSTR name)
{
    if (!wcsstr(name, L"eaid://") || wcsncmp(name, UI_PREFIX, wcslen(UI_PREFIX)) == 0)
        return false;
    std::wstring lower(name);
    for (auto &c : lower)
        c = wchar_t(towlower(c));
    for (auto *k : INTERESTING)
        if (lower.find(k) != std::wstring::npos)
            return true;
    return false;
}

void log_named(ID3D12Object *o, LPCWSTR name)
{
    if (named_seen.size() >= NAMED_LINES ||
        std::find(named_seen.begin(), named_seen.end(), name) != named_seen.end())
        return;
    ID3D12Resource *res = nullptr;
    if (FAILED(o->QueryInterface(__uuidof(ID3D12Resource), reinterpret_cast<void **>(&res))) || !res)
        return;
    D3D12_RESOURCE_DESC d = res->GetDesc();
    res->Release();
    if (d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D)
        return;
    named_seen.emplace_back(name);
    swofty::log("[postfx] diag: named texture %s %llux%u fmt %d flags 0x%x", runtime::narrow(name).c_str(),
                (unsigned long long)d.Width, d.Height, int(d.Format), unsigned(d.Flags));
}

void on_setname(ID3D12Object *o, LPCWSTR name)
{
    static const uint32_t probe = perf::probe("core", "postfx SetName");
    if (!name)
        return;
    perf::Scope p(probe);
    auto *res = reinterpret_cast<ID3D12Resource *>(o);
    std::lock_guard<std::recursive_mutex> g(lock);
    if (watched.bits(res)) {
        names.erase(res);
        watched.watch(res, 0);
    }
    drop_depths(wcscmp(name, DEPTH_NAME) == 0 ? res : nullptr, res);
    bool tonemap = wcscmp(name, TONEMAP_NAME) == 0, back = wcscmp(name, BACKBUFFER_NAME) == 0;
    if (back)
        for (auto &v : recent_views)
            if (v.res.load(std::memory_order_relaxed) == res)
                pin_view(v.at.load(std::memory_order_relaxed), res);
    uint8_t bits = (tonemap ? W_TONEMAP : 0) | (back ? W_BACK : 0) | (wcscmp(name, HUDLESS_NAME) == 0 ? W_HUDLESS : 0) |
                   (wcscmp(name, DEPTH_NAME) == 0 ? W_DEPTH : 0) |
                   (wcsncmp(name, UI_PREFIX, wcslen(UI_PREFIX)) == 0 ? W_UI : 0);
    if (tonemap || back || (!diag_done && wcsstr(name, L"eaid://")))
        names[res] = name;
    if (back && !saw_back) {
        saw_back = true;
        swofty::log("[postfx] the game names its back buffers \"BackBuffer\"");
    }
    if (tonemap && !tonemap_named) {
        tonemap_named = true;
        swofty::log("[postfx] the game named its tonemap output %p", static_cast<void *>(res));
    }
    if (interesting(name))
        log_named(o, name);
    if (bits)
        watched.watch(res, bits);
}

// While the game reads the finished picture for its HUD (paused, panels over the race), that read is the one
// place the passes run; without such reads they run at the HUD-less step or between the game's lists
// (postfx::HudRead).
bool hud_reads()
{
    return hud_read.holds(frame_count);
}

// Batches that move only depth and UI images, most watched batches, are marked without the lock by
// the thread recording the list; the locked paths take the side marks into the list's Mark before they
// read it.
using Side = postfx::SideMark<ID3D12GraphicsCommandList, ID3D12Resource, D3D12_RESOURCE_STATES>;
postfx::SideMarks<ID3D12GraphicsCommandList, ID3D12Resource, D3D12_RESOURCE_STATES> side_marks;

// A list's Mark, made when it has none; its side entry notes that it has one.
Mark &mark_of(ID3D12GraphicsCommandList *list)
{
    if (Side *s = side_marks.find(list, true))
        s->marked.store(true, std::memory_order_relaxed);
    return marks[list];
}

void unmark(ID3D12GraphicsCommandList *list)
{
    marks.erase(list);
    if (Side *s = side_marks.find(list, false)) {
        s->depth.store(nullptr, std::memory_order_relaxed);
        s->bound.store(nullptr, std::memory_order_relaxed);
        s->ui.store(false, std::memory_order_relaxed);
        s->marked.store(false, std::memory_order_relaxed);
    }
}

void pin_view(SIZE_T at, ID3D12Resource *res)
{
    if (!at)
        return;
    for (auto &v : back_views)
        if (v.at.load(std::memory_order_relaxed) == at) {
            v.res.store(res, std::memory_order_relaxed);
            return;
        }
    if (!res)
        return;
    for (auto &v : back_views) {
        SIZE_T none = 0;
        if (v.at.compare_exchange_strong(none, at)) {
            v.res.store(res, std::memory_order_relaxed);
            SIZE_T lo = views_lo.load(std::memory_order_relaxed), hi = views_hi.load(std::memory_order_relaxed);
            while (at < lo && !views_lo.compare_exchange_weak(lo, at))
                ;
            while (at > hi && !views_hi.compare_exchange_weak(hi, at))
                ;
            return;
        }
    }
}

void on_create_rtv(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at)
{
    ViewAt &r = recent_views[recent_next.fetch_add(1, std::memory_order_relaxed) % 64];
    r.at.store(at.ptr, std::memory_order_relaxed);
    r.res.store(res, std::memory_order_relaxed);
    pin_view(at.ptr, res && (watched.bits(res) & W_BACK) ? res : nullptr);
}

// The picture and the HUD draw into the back buffer as the first render target. Binds of anything else
// fall outside the back buffers' views and return at the range check.
void on_render_targets(ID3D12GraphicsCommandList *list, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtvs, BOOL)
{
    if (!n || !rtvs)
        return;
    SIZE_T at = rtvs[0].ptr;
    if (at < views_lo.load(std::memory_order_relaxed) || at > views_hi.load(std::memory_order_relaxed))
        return;
    for (auto &v : back_views)
        if (v.at.load(std::memory_order_relaxed) == at) {
            if (ID3D12Resource *res = v.res.load(std::memory_order_relaxed))
                if (Side *s = side_marks.find(list, true))
                    s->bound.store(res, std::memory_order_relaxed);
            return;
        }
}

// Takes a list's side marks into its Mark m. A depth barrier makes the Mark when the list has none, as
// it does on the locked path.
Mark *absorb(ID3D12GraphicsCommandList *list, Mark *m)
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
bool mark_aside(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers, bool depth)
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
        uint8_t c = watched.bits(b.Transition.pResource);
        postfx::note(*s, marked, (c & W_DEPTH) != 0, (c & W_UI) != 0, b.Transition.pResource, b.Transition.StateAfter);
    }
    return true;
}

// Before the game records a barrier batch that takes its depth buffer out of DEPTH_WRITE: copies the
// buffer into depth_copy, from the state the barrier names as its StateBefore, and puts it back in that
// state, so the game's barrier then runs as it would have. Whole-resource, unsplit barriers in direct
// lists only; one copy per batch.
void copy_depth(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION || !(watched.bits(b.Transition.pResource) & W_DEPTH) ||
            b.Flags != D3D12_RESOURCE_BARRIER_FLAG_NONE ||
            b.Transition.Subresource != D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES ||
            !(b.Transition.StateBefore & D3D12_RESOURCE_STATE_DEPTH_WRITE) ||
            (b.Transition.StateAfter & D3D12_RESOURCE_STATE_DEPTH_WRITE))
            continue;
        if (list->GetType() != D3D12_COMMAND_LIST_TYPE_DIRECT)
            return;
        ID3D12Resource *src = b.Transition.pResource;
        D3D12_RESOURCE_DESC d = src->GetDesc();
        if (d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || d.SampleDesc.Count != 1 ||
            depth_view(d.Format) == DXGI_FORMAT_UNKNOWN)
            return;
        ID3D12Resource *copy;
        {
            std::lock_guard<std::recursive_mutex> g(lock);
            copy = depth_copy_for(src, d);
        }
        if (!copy)
            return;
        D3D12_RESOURCE_BARRIER to[2]{}, back[2]{};
        for (auto *t : {to, back})
            for (int j = 0; j < 2; j++) {
                t[j].Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
                t[j].Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
            }
        to[0].Transition.pResource = back[0].Transition.pResource = src;
        to[0].Transition.StateBefore = back[0].Transition.StateAfter = b.Transition.StateBefore;
        to[0].Transition.StateAfter = back[0].Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_SOURCE;
        to[1].Transition.pResource = back[1].Transition.pResource = copy;
        to[1].Transition.StateBefore = back[1].Transition.StateAfter = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        to[1].Transition.StateAfter = back[1].Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
        d3d12hooks::barrier_original(list, 2u, to);
        list->CopyResource(copy, src);
        d3d12hooks::barrier_original(list, 2u, back);
        depth_copy_frame.store(frame_count, std::memory_order_relaxed);
        return;
    }
}

// Marks what each game list does with the watched resources. At the HUD-less step (the HUD-less copy
// leaves UAV while this list has made the back buffer a render target) the passes are recorded into the
// list, before the game's barrier, with every state as it was before this batch.
bool on_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    static const uint32_t probe = perf::probe("core", "postfx ResourceBarrier");
    if (!barriers || !n || broken)
        return false;
    perf::Scope p(probe);
    uint8_t seen = 0;
    for (UINT i = 0; i < n; i++)
        if (barriers[i].Type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            seen |= watched.bits(barriers[i].Transition.pResource);
    if (!seen)
        return false;
    static const uint32_t marking = perf::probe("core", "postfx ResourceBarrier (watched)");
    perf::Scope m_scope(marking);
    if ((seen & W_DEPTH) && gpu.device && frame_count <= depth_copy_until.load(std::memory_order_relaxed))
        copy_depth(list, n, barriers);
    if (!(seen & (W_TONEMAP | W_BACK | W_HUDLESS)) && mark_aside(list, n, barriers, seen & W_DEPTH))
        return false;
    std::lock_guard<std::recursive_mutex> g(lock);
    auto it = marks.find(list);
    Mark *m = absorb(list, it == marks.end() ? nullptr : &it->second);
    ID3D12Resource *hudless = nullptr, *leaving = nullptr;
    D3D12_RESOURCE_STATES hudless_now{};
    bool moves_back = false;
    for (UINT i = 0; i < n; i++) {
        const D3D12_RESOURCE_BARRIER &b = barriers[i];
        if (b.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION)
            continue;
        uint8_t c = watched.bits(b.Transition.pResource);
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
    if (burst.active() && gpu.device)
        burst.record(list, n, barriers, gpu, watched, frame_count);
    // The game records a frame's lists in parallel, so the core cannot tell at one place whether another
    // place of the same frame runs the passes too. On track the game reads the finished picture once
    // per frame for its HUD, as the first back buffer barrier of the HUD's list (or the first one that
    // leaves the render target in the list that drew it): the passes run there and nowhere else. In
    // menus there is no such read; the passes run at the HUD-less step of the list that made the back
    // buffer a render target, once per back buffer and frame.
    if ((hudless || leaving) && state_armed && list->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT && !killed_now() &&
        !passes.empty()) {
        Where w{list, true, nullptr, D3D12_RESOURCE_STATE_RENDER_TARGET, hudless, hudless_now, nullptr,
                D3D12_RESOURCE_STATE_COMMON, "in-list"};
        bool split_step = false, at_read = false;
        if (leaving && (!m || !m->left_rt) && !postfx::placed_lately(frame_count, back_use(leaving).place.placed_at)) {
            BackUse &u = back_use(leaving);
            hud_read.read(frame_count);
            w.back = leaving;
            w.hudless = m ? m->hudless : nullptr;
            w.hudless_state = m ? m->hudless_state : D3D12_RESOURCE_STATE_COMMON;
            w.depth = m && m->depth ? m->depth : u.depth;
            w.depth_state = m && m->depth ? m->depth_state : u.depth_state;
            w.path = "in-list (HUD read)";
            at_read = true;
        } else if (hudless && !hud_reads() && !moves_back && m && m->target &&
                   m->state == D3D12_RESOURCE_STATE_RENDER_TARGET && m->slot < 0 && back_use(m->target).place.fresh) {
            w.back = m->target;
            w.depth = m->depth;
            w.depth_state = m->depth_state;
        } else if (hudless && !hud_reads() && !moves_back && (!m || !m->target)) {
            Side *sm = side_marks.find(list, false);
            ID3D12Resource *bound = sm ? sm->bound.load(std::memory_order_relaxed) : nullptr;
            if (bound && back_use(bound).place.split_step(frame_count)) {
                BackUse &u = back_use(bound);
                w.back = bound;
                w.depth = m && m->depth ? m->depth : u.depth;
                w.depth_state = m && m->depth ? m->depth_state : u.depth_state;
                w.path = "in-list (split step)";
                split_step = true;
            }
        }
        if (w.back) {
            back_use(w.back).place.place(frame_count);
            if (!gpu.device && !init(w.back)) {
                broken = true;
                return false;
            }
            int k = pick_slot();
            if (k >= 0 && record(w, k)) {
                if (!m)
                    m = &mark_of(list);
                m->slot = k;
                m->ran_back = w.back;
                report.stats.in_list++;
                report.stats.split_step += split_step;
                report.stats.hud_read += at_read;
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
        uint8_t c = watched.bits(r);
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
                !(b.Transition.StateAfter & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) &&
                b.Flags == D3D12_RESOURCE_BARRIER_FLAG_NONE)
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
            if (b.Transition.StateBefore == D3D12_RESOURCE_STATE_RENDER_TARGET &&
                (b.Transition.StateAfter & READ_STATES))
                m->left_rt = true;
            m->target = r;
            m->state = b.Transition.StateAfter;
            if (b.Transition.StateAfter == D3D12_RESOURCE_STATE_RENDER_TARGET &&
                !(b.Transition.StateBefore & (READ_STATES | D3D12_RESOURCE_STATE_RENDER_TARGET))) {
                BackUse &u = back_use(r);
                u.place.drawn(frame_count);
                m->drew = r;
            }
        }
    }
    return false;
}

// A list reset before it was submitted throws its commands away, the core's passes with them: their
// slot is free again and the frame's back buffer still waits for them.
void on_reset(ID3D12GraphicsCommandList *list)
{
    std::lock_guard<std::recursive_mutex> g(lock);
    auto it = marks.find(list);
    if (it != marks.end() && it->second.slot >= 0) {
        slots[it->second.slot].pending = false;
        if (it->second.ran_back) {
            BackUse &u = back_use(it->second.ran_back);
            u.place.unplace();
        }
        report.stats.dropped++;
    }
    unmark(list);
}

void game_frame(const void *was, ULONGLONG now);

// The back buffer whose passes have waited longest for the list that closes its HUD-less step.
BackUse *waiting_back()
{
    BackUse *at = nullptr;
    for (auto &u : back_uses)
        if (u.back && u.place.waits(frame_count) &&
            (!at || frame_count - u.place.opened_at > frame_count - at->place.opened_at))
            at = &u;
    return at;
}

// Submits the game's lists, the core's own list after a picture list that ended before the HUD or after the list
// that closed a HUD-less step the picture's list left open, and a fence signal after any list the passes were
// recorded into, so their slot is known done. A list that draws into the game's next back buffer closes the
// passes' frame while the game's own frames do (postfx::FrameClock).
void on_exec(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists, d3d12hooks::ExecPlan &plan)
{
    static const uint32_t probe = perf::probe("core", "postfx ExecuteCommandLists");
    if (!lists || !n || broken)
        return;
    perf::Scope p(probe);
    UINT split = n;
    bool after_close = false, before = false;
    ID3D12Resource *res = nullptr, *hudless = nullptr, *later_depth = nullptr;
    D3D12_RESOURCE_STATES state{}, hudless_state{}, later_depth_state{};
    int signal[FRAMES];
    UINT signals = 0;
    {
        std::lock_guard<std::recursive_mutex> g(lock);
        for (UINT i = 0; i < n; i++) {
            auto *gl = reinterpret_cast<ID3D12GraphicsCommandList *>(lists[i]);
            auto it = marks.find(gl);
            Mark *mark = absorb(gl, it == marks.end() ? nullptr : &it->second);
            if (!mark)
                continue;
            Mark &m = *mark;
            if (m.drew) {
                const void *was = frame_clock.drawn;
                ULONGLONG now = GetTickCount64();
                if (frame_clock.submit(m.drew, now, started))
                    game_frame(was, now);
            }
            if (m.slot >= 0 && signals < FRAMES)
                signal[signals++] = m.slot;
            report.trace.picture |= m.tonemap;
            if (m.tonemap && !diag_done) {
                log_diag(reinterpret_cast<ID3D12GraphicsCommandList *>(lists[i]), m);
                diag_done = true;
            }
            // A list that ends with the back buffer out of render target while the passes still wait for the
            // step's closing list draws the HUD and hands the frame to Present: they run right before it.
            if (m.target && m.state != D3D12_RESOURCE_STATE_RENDER_TARGET && split == n && !killed_now() &&
                !passes.empty() &&
                back_use(m.target).place.target_left(frame_count, gl->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT)) {
                split = i;
                before = true;
                res = m.target;
                state = D3D12_RESOURCE_STATE_RENDER_TARGET;
            }
            if (m.target && !m.tonemap)
                back_use(m.target).place.moved(m.state == D3D12_RESOURCE_STATE_RENDER_TARGET);
            if (m.tonemap && m.slot < 0 && split == n && !hud_reads() && !killed_now() && !passes.empty()) {
                if (m.ui) {
                    report.stats.ui_together++;
                    report.trace.tried = postfx::IDLE_UI_TOGETHER;
                    report.trace.why = postfx::idle_text(report.trace.tried);
                } else if (m.target && m.state == D3D12_RESOURCE_STATE_RENDER_TARGET) {
                    // The picture is in the back buffer. A list that ends with the HUD-less step still open
                    // leaves the passes to the list that closes the step, which may draw the rest of the
                    // picture first; while the game's own frames close the passes' frames there is no Present
                    // to fall back on, and the passes run right after this list.
                    bool open = m.hudless && (m.hudless_state & D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
                    bool may_defer = open && !frame_clock.game_frames(GetTickCount64(), started);
                    postfx::PictureEnd end = back_use(m.target).place.picture_submitted(frame_count, open, may_defer);
                    report.stats.deferred += end == postfx::END_DEFER;
                    if (end == postfx::END_BETWEEN) {
                        split = i;
                        res = m.target;
                        state = m.state;
                        hudless = m.hudless;
                        hudless_state = m.hudless_state;
                    }
                }
            } else if (m.closes && !(m.hudless_state & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) && split == n &&
                       !killed_now() && !passes.empty()) {
                // The list that closed a HUD-less step the picture's list left open, without running the passes
                // in it: they run right after it while the back buffer is still a render target.
                BackUse *u = waiting_back();
                if (u && u->place.step_closed(frame_count, gl->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT)) {
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
                            split != i ? "" : before ? " [BETWEEN before this list]" : " [BETWEEN after this list]");
            }
            unmark(gl);
        }
    }
    if (split == n && !signals) {
        if (later_depth) {
            std::lock_guard<std::recursive_mutex> g(lock);
            postfx::hold(depth_res, later_depth);
            depth_state = later_depth_state;
        }
        return;
    }
    auto take_later_depth = [later_depth, later_depth_state](ID3D12CommandQueue *) {
        if (!later_depth)
            return;
        std::lock_guard<std::recursive_mutex> g(lock);
        postfx::hold(depth_res, later_depth);
        depth_state = later_depth_state;
    };
    if (split < n) {
        auto between = [res, state, hudless, hudless_state, after_close, before](ID3D12CommandQueue *queue) {
            std::lock_guard<std::recursive_mutex> g(lock);
            if (run_between(queue, res, state, hudless, hudless_state,
                            before        ? "between (before the list that takes the back buffer to the HUD)"
                            : after_close ? "between (after the list that closed the step)"
                                          : "between")) {
                report.stats.between++;
                report.stats.after_close += after_close;
                report.stats.before_hud += before;
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
        plan.at_end([done, signals](ID3D12CommandQueue *queue) {
            std::lock_guard<std::recursive_mutex> g(lock);
            read_frequency(queue);
            for (UINT i = 0; i < signals; i++) {
                Slot &s = slots[done[i]];
                queue->Signal(s.fence, ++s.fence_value);
                s.pending = false;
                capture.signalled(done[i], s.fence_value);
            }
        });
    }
}

// Frame generation from the game's hardware settings, read every two seconds; on while unknown.
bool frame_gen_on()
{
    static ULONGLONG checked;
    static bool on = true;
    static std::wstring hardware_settings;
    ULONGLONG now = GetTickCount64();
    if (checked && now - checked < 2000)
        return on;
    checked = now;
    if (hardware_settings.empty()) {
        wchar_t docs[MAX_PATH] = {};
        using GetFolder = HRESULT(WINAPI *)(HWND, int, HANDLE, DWORD, LPWSTR);
        HMODULE shell = LoadLibraryW(L"shell32.dll");
        auto get = shell ? reinterpret_cast<GetFolder>(GetProcAddress(shell, "SHGetFolderPathW")) : nullptr;
        if (get && SUCCEEDED(get(nullptr, 0x0005, nullptr, 0, docs)))
            hardware_settings = std::wstring(docs) + L"\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml";
    }
    on = hud::guard::fg_setting(runtime::read_file(hardware_settings, size_t(1) << 20)).on();
    return on;
}

bool setting_value(const std::string &addon, const std::string &option, std::string &text)
{
    size_t current = 0;
    std::vector<std::string> values;
    if (!settings_state(addon, option, current, nullptr, nullptr, &values) || current >= values.size())
        return false;
    text = values[current];
    return true;
}

// Options the passes read live, their on/off options and the addons' toggle keys, from the settings
// ADDON SETTINGS changes in game.
void refresh_live()
{
    std::string text;
    float v = 0;
    for (auto &p : passes) {
        for (auto &l : p.live)
            if (l.slot < 16 && setting_value(p.addon_id, l.option, text) && postfx::option_value(text, v))
                p.options[l.slot] = v;
        if (!p.enabled.empty() && setting_value(p.addon_id, p.enabled, text) && postfx::option_value(text, v))
            p.on = v != 0;
    }
    for (auto &t : toggles)
        if (setting_value(t.addon_id, t.option, text)) {
            t.vk = postfx::key_code(text);
            t.key = text;
        }
}


void drop_textures(std::vector<IUnknown *> &dead, PassTexture &t)
{
    dead.push_back(t.res);
    dead.push_back(t.upload);
    t.res = t.upload = nullptr;
}

PassTexture fresh_texture(const postfx::PlanTex &pt)
{
    PassTexture t;
    t.name = pt.name;
    t.file = pt.decl.file;
    t.scale = pt.decl.scale;
    t.width = pt.decl.width;
    t.height = pt.decl.height;
    t.persistent = pt.decl.persistent;
    t.mips = pt.decl.mips;
    t.format = DXGI_FORMAT(pt.decl.format);
    if (!t.file.empty() && !read_tex_file(t)) {
        swofty::log("[shaders] texture %s is left out: %s is missing or not a texture", t.name.c_str(), t.file.c_str());
        t.file.clear();
        t.name += " (missing)";
    }
    return t;
}

// Makes what the frame needs for the textures a new plan added, and a descriptor heap sized for them.
bool fit_frame()
{
    UINT w = UINT(frame.desc.Width), h = frame.desc.Height;
    for (size_t i = legacy_textures; i < textures.size(); i++) {
        PassTexture &t = textures[i];
        postfx::Size sz = postfx::texture_size(t.scale, t.width, t.height, w, h);
        t.w = sz.w;
        t.h = sz.h;
        if (shared(t) && !t.res) {
            t.res = gpu.texture(t.w, t.h, t.format, t.file.empty(), levels_of(t));
            t.clear = t.file.empty();
            if (!t.res)
                return false;
        }
    }
    for (auto &wk : frame.work) {
        if (!wk.orig)
            continue;
        for (size_t i = 0; i < textures.size(); i++)
            if (!shared(textures[i]) && !wk.tex[i] && !(wk.tex[i] = gpu.texture(textures[i].w, textures[i].h, textures[i].format, true, levels_of(textures[i]))))
                return false;
        wk.clear = true;
    }
    if (uses_previous && !frame.prev && !(frame.prev = gpu.texture(w, h, frame.view, false)))
        return false;
    retire({frame.heap, frame.cpu});
    frame.heap = frame.cpu = nullptr;
    textures_made++;
    return make_heaps();
}

// Replaces the instance passes and textures with a new plan's, keeping the textures of instances that
// run the same effect where they did.
void install_plan(postfx::Plan next)
{
    if (postfx::plan_shape(next) == postfx::plan_shape(plan) && next.instances.size() == plan.instances.size()) {
        for (size_t i = 0; i < next.instances.size(); i++)
            memcpy(plan.instances[i].options, next.instances[i].options, sizeof plan.instances[i].options);
        for (size_t i = legacy_passes; i < passes.size(); i++)
            memcpy(passes[i].options, plan.instances[size_t(passes[i].inst)].options, sizeof passes[i].options);
        return;
    }
    std::vector<int> from = postfx::reuse_textures(plan, next);
    std::vector<PassTexture> old_tex(std::make_move_iterator(textures.begin() + long(legacy_textures)),
                             std::make_move_iterator(textures.end()));
    std::vector<std::vector<ID3D12Resource *>> old_own(FRAMES);
    for (UINT k = 0; k < FRAMES; k++)
        if (frame.work[k].orig)
            old_own[k].assign(frame.work[k].tex.begin() + long(legacy_textures), frame.work[k].tex.end());
    textures.resize(legacy_textures);
    passes.resize(legacy_passes);
    std::vector<IUnknown *> dead;
    std::vector<bool> taken(old_tex.size(), false);
    for (size_t i = 0; i < next.textures.size(); i++) {
        if (from[i] >= 0) {
            taken[size_t(from[i])] = true;
            textures.push_back(std::move(old_tex[size_t(from[i])]));
        } else
            textures.push_back(fresh_texture(next.textures[i]));
    }
    for (size_t j = 0; j < old_tex.size(); j++)
        if (!taken[j])
            drop_textures(dead, old_tex[j]);
    for (UINT k = 0; k < FRAMES; k++) {
        if (old_own[k].empty() && !frame.work[k].orig)
            continue;
        Work &wk = frame.work[k];
        std::vector<ID3D12Resource *> own(wk.tex.begin(), wk.tex.begin() + long(legacy_textures));
        for (size_t i = 0; i < next.textures.size(); i++)
            own.push_back(from[i] >= 0 ? old_own[k][size_t(from[i])] : nullptr);
        for (size_t j = 0; j < old_own[k].size(); j++)
            if (!taken[j])
                dead.push_back(old_own[k][j]);
        wk.tex = std::move(own);
    }
    retire(std::move(dead));
    for (auto &pp : next.passes) {
        RenderPass p;
        p.inst = pp.instance;
        const postfx::PlanInstance &pi = next.instances[size_t(pp.instance)];
        p.addon_id = pi.decl->addon;
        p.effect = pi.effect;
        p.label = pi.label;
        p.name = pp.decl->name;
        p.shader = pp.decl->dxil;
        p.budget_ms = pp.decl->budget_ms;
        p.threads[0] = pp.decl->threads[0];
        p.threads[1] = pp.decl->threads[1];
        memcpy(p.options, pi.options, sizeof p.options);
        for (size_t i = 0; i < pp.n_inputs; i++)
            p.inputs[i] = pp.inputs[i] >= 0 ? int(legacy_textures) + pp.inputs[i] : pp.inputs[i];
        p.n_inputs = UINT(pp.n_inputs);
        p.output = pp.output >= 0 ? int(legacy_textures) + pp.output : -1;
        passes.push_back(std::move(p));
    }
    plan = std::move(next);
    uses_previous = legacy_previous || plan.uses_previous();
    for (auto &sl : slots)
        sl.passes = 0;
    if (frame.heap && !fit_frame()) {
        broken = true;
        swofty::log("[shaders] the textures of the new config could not be made; post-processing is off");
    }
}

RuleDriver rule_driver;


void on_flow_frame(double, void *);

void subscribe_rule_events()
{
    const SwoftyEvents *ev = runtime::events_table();
    if (!ev || !ev->subscribe)
        return;
    uint32_t frame_token = 0;
    if (!ev->on_frame || ev->on_frame(on_flow_frame, nullptr, &frame_token) != SWOFTY_ASSET_OK)
        swofty::log("[postfx] the game's flow tick is not read: while no Present reaches the core, the toggle keys and "
                    "live settings wait for one");
    rule_driver.subscribe(ev);
}

void apply_active(postfx::ActiveSpec next)
{
    postfx::Plan p = postfx::make_plan(next, effects);
    for (auto &d : p.dropped)
        swofty::log("[shaders] %s; left out", d.c_str());
    size_t n_passes = p.passes.size(), n_effects = p.instances.size();
    shader_vk = postfx::key_code(next.toggle_key);
    install_plan(std::move(p));
    active = std::move(next);
    rule_driver.reset(active, plan, passes);
    swofty::log("[shaders] config \"%s\" (serial %llu): %zu effect(s), %zu pass(es), toggle key %s", active.name.c_str(),
                (unsigned long long)active.serial, n_effects, n_passes,
                shader_vk ? active.toggle_key.c_str() : "none");
}

void check_active(ULONGLONG now, bool force)
{
    if (active_file.empty() || (!force && now - active_checked < 250))
        return;
    active_checked = now;
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (!GetFileAttributesExW(active_file.c_str(), GetFileExInfoStandard, &fa))
        return;
    if (active_told && !CompareFileTime(&fa.ftLastWriteTime, &active_at))
        return;
    postfx::ActiveSpec next;
    if (!postfx::parse_active(runtime::read_file(active_file, size_t(8) << 20), next))
        return;
    active_at = fa.ftLastWriteTime;
    active_told = true;
    apply_active(std::move(next));
}

void load_effects()
{
    effects = postfx::parse_effects(runtime::read_file(runtime::under_swofty("shaders/effects.json"), size_t(16) << 20));
    size_t n = 0;
    for (auto &e : effects)
        n += e.passes.size();
    if (!effects.empty())
        swofty::log("[shaders] effects: %zu effect(s), %zu pass(es) ready to load", effects.size(), n);
}

bool game_in_front()
{
    DWORD pid = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &pid);
    return pid == GetCurrentProcessId();
}

void poll_toggles()
{
    bool front = game_in_front();
    for (auto &t : toggles) {
        bool raw = t.vk && front && (GetAsyncKeyState(t.vk) & 0x8000);
        if (raw && !t.down && !runtime::typing("a shader key")) {
            t.off = !t.off;
            swofty::log("[postfx] %s's passes are %s (its key %s)", t.addon.c_str(), t.off ? "off" : "on", t.key.c_str());
            runtime::hud_toast(t.off ? "SHADERS OFF" : "SHADERS ON", t.addon, 2.0f, t.addon_id);
        }
        t.down = raw;
    }
    bool raw = shader_vk && front && (GetAsyncKeyState(shader_vk) & 0x8000);
    if (raw && !shader_key_down && !runtime::typing("a shader key")) {
        shaders_off = !shaders_off;
        swofty::log("[shaders] shaders are %s (the toggle key %s)", shaders_off ? "off" : "on", active.toggle_key.c_str());
        runtime::hud_toast(shaders_off ? "SHADERS OFF" : "SHADERS ON", active.name, 2.0f, "");
    }
    shader_key_down = raw;
}


void check_trace_file(ULONGLONG now)
{
    if (now - trace_checked < 1000)
        return;
    trace_checked = now;
    std::wstring order_file = swofty_root_dir + L"postfx.order";
    if (GetFileAttributesW(order_file.c_str()) != INVALID_FILE_ATTRIBUTES) {
        std::string text = runtime::read_file(order_file, 64);
        DeleteFileW(order_file.c_str());
        unsigned lists = unsigned(std::strtoul(text.c_str(), nullptr, 10));
        order_left = lists ? lists : 200;
        swofty::log("[postfx] logging the barrier order of the next %u list(s) that draw the picture", order_left);
    }
    clip.check_file();
    burst.check_file(swofty_root_dir);
    if (GetFileAttributesW(trace_file.c_str()) == INVALID_FILE_ATTRIBUTES)
        return;
    std::string text = runtime::read_file(trace_file, 64);
    DeleteFileW(trace_file.c_str());
    unsigned frames = unsigned(std::strtoul(text.c_str(), nullptr, 10));
    report.trace_next(frames ? frames : 600);
    swofty::log("[postfx] tracing the next %u presented frame(s)", frames ? frames : 600);
}


void note_idle()
{
    if (passes.empty())
        return;
    report.note_idle(killed, shaders_off, hud_reads(), frame_count, active.toggle_key);
}

// The frame closes: the passes' frame clock, its trace line and its counts.
void end_frame(ULONGLONG now)
{
    frametime = frame_at ? float(double(now - frame_at) / 1000.0) : 0.0f;
    frame_at = now;
    note_idle();
    frame_count++;
    report.stats.presented++;
    report.stats.without += report.trace.runs == 0;
    report.stats.doubled += report.trace.runs > 1;
    burst.land(frame_count, frame.view);
    clip.land(slots, frame_count);
    report.log_trace(frame_count, fg_on.load());
    report.trace = FrameReport::Trace{};
}

// The toggle keys, the live settings, the shader config and the rules.
void tick(ULONGLONG now)
{
    check_trace_file(now);
    poll_toggles();
    check_active(now, false);
    rule_driver.live_frame(now, swofty_root_dir);
    rule_driver.frame(view::current(), now, active, plan, passes);
    if (now - live_at >= 250) {
        live_at = now;
        refresh_live();
    }
}

void stat_line(ULONGLONG now)
{
    report.stat_line(now, frame_clock.game_frames(now, started), !passes.empty(), ran_total);
}

// The frame generation module that presents frames of its own on a swapchain (postfx::presents_own_frames): the
// one its creation named, or for a swapchain the core does not know, the module that calls Present.
std::string own_frames_by(const d3d12hooks::Swapchain *known, void *caller)
{
    if (known)
        return postfx::presents_own_frames(known->fg) ? known->fg : std::string();
    static void *seen;
    static std::string by;
    if (caller != seen) {
        seen = caller;
        by = d3d12hooks::module_of(caller);
        if (!postfx::presents_own_frames(by))
            by.clear();
    }
    return by;
}

// The passes of a frame that reached Present with its HUD-less step open and no list the core saw taking the back
// buffer out of render target run on the presented back buffer, on the swapchain's queue: over the HUD for that
// frame, so the picture never goes without them. A frame generation swapchain shows a buffer of its own that it
// copied the game's back buffer into: the passes run on that copy while it holds the waiting picture
// (postfx::shown_is_waiting).
void present_fallback(IDXGISwapChain3 *sc, const d3d12hooks::Swapchain *known)
{
    BackUse *waiting = waiting_back();
    if (!waiting || !known || !known->queue || killed_now() || passes.empty())
        return;
    ID3D12Resource *shown = nullptr;
    if (FAILED(sc->GetBuffer(sc->GetCurrentBackBufferIndex(), IID_PPV_ARGS(&shown))) || !shown)
        return;
    bool direct = known->queue->GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT;
    BackUse *own = nullptr;
    for (auto &u : back_uses)
        if (u.back == shown)
            own = &u;
    if (own) {
        if (direct && own->place.presented(frame_count) &&
            run_between(known->queue, shown, D3D12_RESOURCE_STATE_PRESENT, nullptr, D3D12_RESOURCE_STATE_COMMON,
                        "at Present"))
            report.stats.at_present++;
    } else {
        D3D12_RESOURCE_DESC a = shown->GetDesc(), b = waiting->back->GetDesc();
        bool same_shape = a.Width == b.Width && a.Height == b.Height && a.Format == b.Format;
        bool generating = frame_gen_on();
        static bool told;
        if (!std::exchange(told, true))
            swofty::log("[postfx] swapchain %p shows buffer %p, which is not the back buffer %p the game drew into "
                        "(a frame generation swapchain copies the game's frames into its own); frame generation is "
                        "%s in the game's settings, so passes that wait for Present %s",
                        static_cast<void *>(sc), static_cast<void *>(shown), static_cast<void *>(waiting->back),
                        generating ? "on" : "off",
                        generating   ? "do not run on that copy"
                        : same_shape ? "run on that copy"
                                     : "do not run on that copy, which has another size or format");
        if (direct &&
            postfx::shown_is_waiting(generating, report.trace.runs, frame_count, waiting->place.opened_at, same_shape) &&
            waiting->place.presented(frame_count) &&
            run_between(known->queue, shown, D3D12_RESOURCE_STATE_PRESENT, nullptr, D3D12_RESOURCE_STATE_COMMON,
                        "at Present (the swapchain's copy)")) {
            report.stats.at_present++;
            report.stats.copied++;
        }
    }
    shown->Release();
}

// Present closes the frame (its trace line and counts) and runs the toggle keys and the live settings. While
// frame generation presents frames of its own, a Present is not one of the game's frames: it only runs the
// toggle keys and the live settings, and the game's own frames close the frame where their lists are submitted.
void on_present(IDXGISwapChain3 *sc, const d3d12hooks::Swapchain *known, void *caller)
{
    static const uint32_t probe = perf::probe("core", "postfx Present");
    perf::Scope p(probe);
    std::lock_guard<std::recursive_mutex> g(lock);
    if (broken)
        return;
    fg_on = !known || !known->queue || !known->fg.empty() || (known->streamline && frame_gen_on());
    ULONGLONG now = GetTickCount64();
    present_at.store(now, std::memory_order_relaxed);
    std::string by = own_frames_by(known, caller);
    bool generated = !by.empty();
    report.stats.generated += generated;
    if (frame_clock.present(now, generated, started)) {
        present_fallback(sc, known);
        end_frame(now);
        tick(now);
        stat_line(now);
        return;
    }
    static bool told;
    if (generated && !std::exchange(told, true))
        swofty::log("[postfx] %s presents the game's frames and its own generated ones on swapchain %p; the passes' "
                    "frames close once per frame the game draws, where it submits the list that draws into its next "
                    "back buffer",
                    by.c_str(), static_cast<void *>(sc));
    tick(now);
}

// The game drew into its next back buffer while its own frames close the passes' frames: the frame before closes,
// counted as skipped when the passes did not run on its back buffer.
void game_frame(const void *was, ULONGLONG now)
{
    for (auto &u : back_uses)
        if (was && u.back == was && !postfx::placed_lately(frame_count, u.place.placed_at))
            report.stats.unplaced++;
    end_frame(now);
    stat_line(now);
}

// While no Present reaches the core's hooks the game's flow tick runs the toggle keys and the live settings. It
// does not close the passes' frames: the game's own frames do, where their lists are submitted.
void on_flow_frame(double, void *)
{
    if (!device_armed.load(std::memory_order_relaxed))
        return;
    ULONGLONG now = GetTickCount64(), presented = present_at.load(std::memory_order_relaxed);
    if (!postfx::presents_gone(now, presented, started))
        return;
    std::lock_guard<std::recursive_mutex> g(lock);
    if (broken || !frame_clock.flow(now, started))
        return;
    static bool told;
    if (!std::exchange(told, true))
        swofty::log("[postfx] no Present has reached the core's swapchain hooks for %llu ms; the game's own frames "
                    "close the passes' frames where it submits the list that draws into its next back buffer, and the "
                    "game's flow tick runs the toggle keys and live settings",
                    static_cast<unsigned long long>(now - std::max(frame_clock.present_at, started)));
    tick(now);
}

int texture_named(const std::string &name)
{
    if (name == "original")
        return IN_ORIGINAL;
    if (name == "previous")
        return IN_PREVIOUS;
    for (size_t i = 0; i < textures.size(); i++)
        if (textures[i].name == name)
            return int(i);
    return IN_NONE;
}

bool load_passes()
{
    const json::Value &cfg = runtime::config();
    for (auto &e : cfg["postfx_textures"].a) {
        PassTexture t;
        t.name = e["name"].str();
        t.file = e["file"].str();
        if (e["scale"].type == json::Value::Num)
            t.scale = e["scale"].n;
        if (e["width"].type == json::Value::Num && e["height"].type == json::Value::Num) {
            t.width = UINT(e["width"].n);
            t.height = UINT(e["height"].n);
        }
        t.persistent = e["persistent"].type == json::Value::Bool && e["persistent"].b;
        int f = postfx::format_of(e["format"].str("rgba16f"));
        t.format = f ? DXGI_FORMAT(f) : DXGI_FORMAT_R16G16B16A16_FLOAT;
        if (!t.file.empty() && !read_tex_file(t)) {
            swofty::log("[postfx] texture %s is left out: %s is missing or not a texture", t.name.c_str(), t.file.c_str());
            t.file.clear();
            t.name += " (missing)";
        }
        textures.push_back(std::move(t));
    }
    for (auto &e : cfg["postfx"].a) {
        RenderPass p;
        p.addon = e["addon"].str("an addon");
        p.addon_id = e["addon_id"].str();
        p.name = e["name"].str("pass");
        p.shader = e["shader"].str();
        p.enabled = e["enabled"].str();
        const json::Value &b = e["budget_ms"];
        if (b.type == json::Value::Num && b.n > 0)
            p.budget_ms = b.n;
        size_t k = 0;
        for (auto &o : e["options"].a)
            if (k < 16)
                p.options[k++] = o.type == json::Value::Num ? float(o.n) : 0.0f;
        for (auto &l : e["live"].a)
            if (l["slot"].type == json::Value::Num)
                p.live.push_back({UINT(l["slot"].n), l["option"].str()});
        const char *bad = nullptr;
        for (auto &in : e["inputs"].a) {
            int t = texture_named(in.str());
            if (t == IN_NONE || p.n_inputs >= INPUTS)
                bad = "names an input that is not there";
            else
                p.inputs[p.n_inputs++] = t;
        }
        std::string out = e["output"].str();
        if (!out.empty()) {
            p.output = texture_named(out);
            if (p.output < 0 || !textures[size_t(p.output)].file.empty())
                bad = "writes a texture that is not there or comes from a file";
            for (UINT i = 0; i < p.n_inputs; i++)
                if (p.output >= 0 && p.inputs[i] == p.output)
                    bad = "reads the texture it writes";
        }
        const auto &th = e["threads"].a;
        if (th.size() == 2 && th[0].n >= 1 && th[1].n >= 1) {
            p.threads[0] = UINT(th[0].n);
            p.threads[1] = UINT(th[1].n);
        }
        for (auto &v : e["when"]["views"].a)
            p.when.views |= postfx::view_bit(v.str());
        for (auto &f : e["when"]["skip"].a)
            p.when.skip |= postfx::flag_bit(f.str());
        std::string bytes = runtime::read_file(runtime::under_swofty(p.shader), size_t(4) << 20);
        if (bytes.size() < 32 || memcmp(bytes.data(), "DXBC", 4) != 0)
            bad = bytes.empty() ? "has no DXIL" : "is not a DXIL container";
        if (bad) {
            swofty::log("[postfx] %s's pass %s is off: it %s (%s)", p.addon.c_str(), p.name.c_str(), bad,
                        p.shader.c_str());
            continue;
        }
        p.dxil.assign(bytes.begin(), bytes.end());
        uses_previous |= std::find(p.inputs, p.inputs + p.n_inputs, IN_PREVIOUS) != p.inputs + p.n_inputs;
        passes.push_back(std::move(p));
    }
    for (auto &e : cfg["postfx_toggles"].a) {
        AddonToggle t;
        t.addon_id = e["addon_id"].str();
        t.option = e["option"].str();
        t.addon = t.addon_id;
        for (auto &p : passes)
            if (p.addon_id == t.addon_id)
                t.addon = p.addon;
        if (!t.addon_id.empty() && !t.option.empty())
            toggles.push_back(std::move(t));
    }
    legacy_passes = passes.size();
    legacy_textures = textures.size();
    legacy_previous = uses_previous;
    return !passes.empty();
}

// The setup is done with the profile's passes (it read them, or stopped before and there are none): the bytes
// addons' code gave before then to a pass that isn't there go, and from now on only these passes get any.
void passes_listed()
{
    std::lock_guard<std::recursive_mutex> g(lock);
    std::lock_guard<std::mutex> d(data_lock);
    pass_data.listed([](const std::string &k) {
        for (const RenderPass &p : passes)
            if (postfx::DataStore::key(p.addon_id, p.name) == k)
                return true;
        return false;
    });
    any_data.store(!pass_data.empty(), std::memory_order_relaxed);
}

}

namespace runtime {

void postfx_setup()
{
    // however the setup ends, the passes it leaves are the profile's from then on
    struct Listed {
        ~Listed() { passes_listed(); }
    } listed;
    if (!enabled("postfx", nullptr))
        return;
    char v[260] = {};
    if (GetEnvironmentVariableA("SWOFTY_POSTFX", v, sizeof v) && v[0] == '0') {
        swofty::log("[postfx] SWOFTY_POSTFX=0; every pass is off");
        return;
    }
    off_file = swofty_root() + L"postfx.off";
    trace_file = swofty_root() + L"postfx.trace";
    swofty_root_dir = swofty_root();
    bool legacy = load_passes();
    load_effects();
    active_file = swofty_root() + L"shaders\\active.json";
    if (!legacy && effects.empty() && GetFileAttributesW(active_file.c_str()) == INVALID_FILE_ATTRIBUTES) {
        swofty::log("[postfx] the postfx feature is on, but assets.json names no usable pass and there is no shader config");
        return;
    }
    check_active(GetTickCount64(), true);
    subscribe_rule_events();
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_CAPTURE", v, sizeof v) && v[0]) {
        capture.configure(v, swofty_root() + L"postfx.capture");
    }
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_CLIP", v, sizeof v) && v[0]) {
        clip.configure(v, swofty_root() + L"postfx.clip");
    }
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_BURST_DUMP", v, sizeof v) && v[0])
        burst.configure(v);
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_TRACE", v, sizeof v) && v[0])
        report.trace_next(unsigned(std::strtoul(v, nullptr, 10)));
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_ORDER", v, sizeof v) && v[0])
        order_left = unsigned(std::strtoul(v, nullptr, 10));
    refresh_live();
    d3d12hooks::on_setname(on_setname);
    d3d12hooks::on_barrier(on_barrier);
    d3d12hooks::on_exec(on_exec);
    d3d12hooks::on_present(on_present);
    d3d12hooks::on_reset(on_reset);
    d3d12hooks::on_render_targets(on_render_targets);
    d3d12hooks::on_create_rtv(on_create_rtv);
    d3d12hooks::ProbeScope probe;
    bool hooked = d3d12hooks::arm_device();
    if (!hooked) {
        swofty::log("[postfx] the core's D3D12 device hooks did not arm; post-processing is off");
        return;
    }
    device_armed = true;
    state_armed = d3d12hooks::arm_list_state();
    if (!state_armed)
        swofty::log("[postfx] the command list state hooks did not arm; passes run only between the game's lists");
    if (!d3d12hooks::arm_swapchain())
        swofty::log("[postfx] the swapchain hooks did not arm; toggle keys, live settings and traces are off");
    swofty::log("[postfx] %zu pass(es) and %zu texture(s) wait for the game's picture%s%s", passes.size(),
                textures.size(), capture.dir().empty() ? "" : "; captures go to ", capture.dir().c_str());
}

}

namespace {

// A module's path; empty if it doesn't fit.
std::wstring module_path(HMODULE m)
{
    wchar_t path[1024];
    DWORD n = GetModuleFileNameW(m, path, DWORD(std::size(path)));
    return n && n < std::size(path) ? std::wstring(path, n) : std::wstring();
}

// The id of the addon whose native code is at `at` (postfx::addon_of_module); empty for code that isn't an addon's.
std::string addon_at(const void *at)
{
    HMODULE m = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            static_cast<LPCWSTR>(at), &m) ||
        !m)
        return {};
    return postfx::addon_of_module(module_path(m), module_path(nullptr));
}

std::string lower(std::string s)
{
    for (auto &ch : s)
        ch = char(tolower(static_cast<unsigned char>(ch)));
    return s;
}

}

// The bytes an addon's code gives one of its own passes, read by the pass as t10 from the next frame on until it
// gives others; size 0 takes them away. Only the addon's own DLL can give them (the caller's DLL lies under
// <game folder>\swofty\addons\<addon_id>\), and only to a pass its manifest lists, so one addon can't feed another's
// passes and the core keeps at most 64 KB a pass. 0 when taken and that pass is running, 1 when taken but it isn't
// (switched off, over its budget, post-processing off, or the core hasn't read the profile's passes yet: an addon's
// on-load can come first, and the bytes wait for them), -1 for a missing addon id or pass name or bytes, -2 above
// 64 KB, -3 when the caller isn't that addon's code, -4 when this profile has no pass of that name for the addon (not
// in its manifest, or the build left it out, e.g. its shader didn't compile). swofty/postfx.hpp looks it up by name:
// a core without it reads as unavailable.
extern "C" __declspec(dllexport) int SwoftyCorePostfxSetData(const char *addon_id, const char *pass, const void *data,
                                                             uint32_t size)
{
    if (!addon_id || !*addon_id || !pass || !*pass || (size && !data))
        return -1;
    if (size > DATA_MAX)
        return -2;
    if (addon_at(__builtin_return_address(0)) != lower(addon_id))
        return -3;
    std::string key = postfx::DataStore::key(addon_id, pass);
    {
        std::lock_guard<std::mutex> g(data_lock);
        if (!pass_data.is_listed()) {
            bool kept = pass_data.set_early(key, data, size);
            any_data.store(!pass_data.empty(), std::memory_order_relaxed);
            return kept ? 1 : -4;
        }
    }
    bool known = false, running = false;
    {
        std::lock_guard<std::recursive_mutex> g(lock);
        for (const RenderPass &p : passes)
            if (p.addon_id == addon_id && p.name == pass) {
                known = true;
                running = running ||
                          (device_armed && !broken && !p.off && !p.ruled_off && p.on && !addon_off(p.addon_id));
            }
    }
    if (!known)
        return -4;
    {
        std::lock_guard<std::mutex> g(data_lock);
        pass_data.set(key, data, size);
        any_data.store(!pass_data.empty(), std::memory_order_relaxed);
    }
    return running ? 0 : 1;
}
