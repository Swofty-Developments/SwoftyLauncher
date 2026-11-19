#include "core/render/postfx/pipeline.hpp"

#include "core/diagnostics/perf.hpp"
#include "core/frontend/view_state.hpp"
#include "core/hud/arm_policy.hpp"
#include "core/hud/hardware_settings.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

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
//   when the core saw no such list (render::postfx::BackPlace).
//
// In the game's lists the core records its passes before the game's barrier and then puts back the
// pipeline, descriptor heaps, compute bindings and graphics descriptor tables it changed
// (render::d3d12::list_state). The passes copy the back buffer, run as DXIL compute shaders
// (swofty/postfx.hlsli) through a slot's own textures, and copy the result back; with frame generation
// on, the result goes into the game's HUD-less copy too. Each slot has its own fence and working
// textures, so frames the GPU runs at once on different queues never share them; every texture rests
// in NON_PIXEL_SHADER_RESOURCE between recordings. A pass whose pipeline cannot be made, or whose GPU
// time stays above its budget, is turned off and logged; SWOFTY_POSTFX=0 or a swofty/postfx.off file
// turns every pass off.

namespace render::postfx {

namespace {

Pipeline pipeline;

void hook_setname(ID3D12Object *o, LPCWSTR name) { pipeline.placement.on_setname(o, name); }

bool hook_barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *barriers)
{
    return pipeline.placement.on_barrier(list, n, barriers);
}

void hook_exec(ID3D12CommandQueue *q, UINT n, ID3D12CommandList *const *lists, d3d12::ExecPlan &plan)
{
    pipeline.placement.on_exec(q, n, lists, plan);
}

void hook_present(IDXGISwapChain3 *sc, const d3d12::Swapchain *known, void *caller) { pipeline.on_present(sc, known, caller); }

void hook_reset(ID3D12GraphicsCommandList *list) { pipeline.placement.on_reset(list); }

void hook_render_targets(ID3D12GraphicsCommandList *list, UINT n, const D3D12_CPU_DESCRIPTOR_HANDLE *rtvs, BOOL single)
{
    pipeline.placement.on_render_targets(list, n, rtvs, single);
}

void hook_create_rtv(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at) { pipeline.placement.on_create_rtv(res, at); }

void hook_flow_frame(double dt, void *user) { pipeline.flow_frame(dt, user); }

}

// Frame generation from the game's hardware settings, read every two seconds; on while unknown.
bool Pipeline::frame_gen_on(ULONGLONG now)
{
    if (frame_gen_poll.due(now))
        frame_gen = hud::hardware_settings::frame_gen(frame_gen_poll.file_path()).on();
    return frame_gen;
}

bool Pipeline::killed_now()
{
    ULONGLONG now = GetTickCount64();
    if (now - last_check < 1000)
        return killed;
    last_check = now;
    bool was = killed;
    killed = GetFileAttributesW(off_file.c_str()) != INVALID_FILE_ATTRIBUTES;
    if (killed != was)
        swofty::log(killed ? "[postfx] swofty/postfx.off exists; every pass is off"
                           : "[postfx] swofty/postfx.off is gone; passes run again");
    return killed;
}

void Pipeline::retire(std::vector<IUnknown *> objects) { retired.retire(std::move(objects), frame_count); }

void Pipeline::subscribe_rule_events()
{
    const SwoftyEvents *ev = runtime::events_table();
    if (!ev || !ev->subscribe)
        return;
    uint32_t frame_token = 0;
    if (!ev->on_frame || ev->on_frame(hook_flow_frame, nullptr, &frame_token) != SWOFTY_ASSET_OK)
        swofty::log("[postfx] the game's flow tick is not read: while no Present reaches the core, the toggle keys and "
                    "live settings wait for one");
    rules.subscribe(ev);
}

void Pipeline::check_trace_file(ULONGLONG now)
{
    if (now - trace_checked < 1000)
        return;
    trace_checked = now;
    std::wstring order_file = swofty_root_dir + L"postfx.order";
    if (GetFileAttributesW(order_file.c_str()) != INVALID_FILE_ATTRIBUTES) {
        std::string text = runtime::read_file(order_file, 64);
        DeleteFileW(order_file.c_str());
        unsigned lists = unsigned(std::strtoul(text.c_str(), nullptr, 10));
        placement.order_left = lists ? lists : 200;
        swofty::log("[postfx] logging the barrier order of the next %u list(s) that draw the picture", placement.order_left);
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

void Pipeline::note_idle()
{
    if (config.passes.empty())
        return;
    report.note_idle(killed, config.shaders_off, placement.hud_reads(), frame_count, config.active.toggle_key);
}

// The frame closes: the passes' frame clock, its trace line and its counts.
void Pipeline::end_frame(ULONGLONG now)
{
    frametime = frame_at ? float(double(now - frame_at) / 1000.0) : 0.0f;
    frame_at = now;
    note_idle();
    frame_count++;
    report.stats.presented++;
    report.stats.without += report.trace.runs == 0;
    report.stats.doubled += report.trace.runs > 1;
    burst.land(frame_count, frame.view);
    clip.land(recorder.slots, frame_count);
    report.log_trace(frame_count, fg_on.load());
    report.trace = FrameReport::Trace{};
}

// The toggle keys, the live settings, the shader config and the rules.
void Pipeline::tick(ULONGLONG now)
{
    check_trace_file(now);
    config.poll_toggles();
    config.check_active(now, false);
    rules.live_frame(now, swofty_root_dir);
    rules.frame(frontend::view::current(), now, config.active, config.plan, config.passes);
    if (now - live_at >= 250) {
        live_at = now;
        config.refresh_live();
    }
}

void Pipeline::stat_line(ULONGLONG now)
{
    report.stat_line(now, frame_clock.game_frames(now, started), !config.passes.empty(), ran_total);
}

// The frame generation module that presents frames of its own on a swapchain (postfx::presents_own_frames): the
// one its creation named, or for a swapchain the core does not know, the module that calls Present.
std::string Pipeline::own_frames_by(const d3d12::Swapchain *known, void *caller)
{
    if (known)
        return postfx::presents_own_frames(known->fg) ? known->fg : std::string();
    if (caller != own_frames_caller) {
        own_frames_caller = caller;
        own_frames_module = d3d12::module_of(caller);
        if (!postfx::presents_own_frames(own_frames_module))
            own_frames_module.clear();
    }
    return own_frames_module;
}

// The passes of a frame that reached Present with its HUD-less step open and no list the core saw taking the back
// buffer out of render target run on the presented back buffer, on the swapchain's queue: over the HUD for that
// frame, so the picture never goes without them. A frame generation swapchain shows a buffer of its own that it
// copied the game's back buffer into: the passes run on that copy while it holds the waiting picture
// (postfx::shown_is_waiting).
void Pipeline::present_fallback(IDXGISwapChain3 *sc, const d3d12::Swapchain *known)
{
    ListPlacement::BackUse *waiting = placement.waiting_back();
    if (!waiting || !known || !known->queue || killed_now() || config.passes.empty())
        return;
    ID3D12Resource *shown = nullptr;
    if (FAILED(sc->GetBuffer(sc->GetCurrentBackBufferIndex(), IID_PPV_ARGS(&shown))) || !shown)
        return;
    bool direct = known->queue->GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT;
    ListPlacement::BackUse *own = nullptr;
    for (auto &u : placement.back_uses)
        if (u.back == shown)
            own = &u;
    if (own) {
        if (direct && own->place.presented(frame_count) &&
            recorder.run_between(known->queue, shown, D3D12_RESOURCE_STATE_PRESENT, nullptr, D3D12_RESOURCE_STATE_COMMON,
                                 "at Present"))
            report.stats.at_present++;
    } else {
        D3D12_RESOURCE_DESC a = shown->GetDesc(), b = waiting->back->GetDesc();
        bool same_shape = a.Width == b.Width && a.Height == b.Height && a.Format == b.Format;
        bool generating = frame_gen_on(GetTickCount64());
        if (!std::exchange(told_copied, true))
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
            recorder.run_between(known->queue, shown, D3D12_RESOURCE_STATE_PRESENT, nullptr, D3D12_RESOURCE_STATE_COMMON,
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
void Pipeline::on_present(IDXGISwapChain3 *sc, const d3d12::Swapchain *known, void *caller)
{
    static const uint32_t probe = perf::probe("core", "postfx Present");
    perf::Scope p(probe);
    std::lock_guard<std::recursive_mutex> g(lock);
    if (broken)
        return;
    fg_on = !known || !known->queue || !known->fg.empty() || (known->streamline && frame_gen_on(GetTickCount64()));
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
    if (generated && !std::exchange(told_generated, true))
        swofty::log("[postfx] %s presents the game's frames and its own generated ones on swapchain %p; the passes' "
                    "frames close once per frame the game draws, where it submits the list that draws into its next "
                    "back buffer",
                    by.c_str(), static_cast<void *>(sc));
    tick(now);
}

// The game drew into its next back buffer while its own frames close the passes' frames: the frame before closes,
// counted as skipped when the passes did not run on its back buffer.
void Pipeline::game_frame(const void *was, ULONGLONG now)
{
    for (auto &u : placement.back_uses)
        if (was && u.back == was && !postfx::placed_lately(frame_count, u.place.placed_at))
            report.stats.unplaced++;
    end_frame(now);
    stat_line(now);
}

// While no Present reaches the core's hooks the game's flow tick runs the toggle keys and the live settings. It
// does not close the passes' frames: the game's own frames do, where their lists are submitted.
void Pipeline::flow_frame(double, void *)
{
    if (!device_armed.load(std::memory_order_acquire))
        return;
    ULONGLONG now = GetTickCount64(), presented = present_at.load(std::memory_order_relaxed);
    if (!postfx::presents_gone(now, presented, started))
        return;
    std::lock_guard<std::recursive_mutex> g(lock);
    if (broken || !frame_clock.flow(now, started))
        return;
    if (!std::exchange(told_flow, true))
        swofty::log("[postfx] no Present has reached the core's swapchain hooks for %llu ms; the game's own frames "
                    "close the passes' frames where it submits the list that draws into its next back buffer, and the "
                    "game's flow tick runs the toggle keys and live settings",
                    static_cast<unsigned long long>(now - std::max(frame_clock.present_at, started)));
    tick(now);
}

// The setup is done with the profile's passes (it read them, or stopped before and there are none): the bytes
// addons' code gave before then to a pass that isn't there go, and from now on only these passes get any.
void Pipeline::passes_listed()
{
    std::lock_guard<std::recursive_mutex> g(lock);
    std::lock_guard<std::mutex> d(recorder.data_lock);
    recorder.pass_data.listed([this](const std::string &k) {
        for (const RenderPass &p : config.passes)
            if (postfx::DataStore::key(p.addon_id, p.name) == k)
                return true;
        return false;
    });
    recorder.any_data.store(!recorder.pass_data.empty(), std::memory_order_relaxed);
}

void Pipeline::setup()
{
    // however the setup ends, the passes it leaves are the profile's from then on
    struct Listed {
        Pipeline &pipeline;
        ~Listed() { pipeline.passes_listed(); }
    } listed{*this};
    if (!runtime::enabled("postfx", nullptr))
        return;
    char v[260] = {};
    if (GetEnvironmentVariableA("SWOFTY_POSTFX", v, sizeof v) && v[0] == '0') {
        swofty::log("[postfx] SWOFTY_POSTFX=0; every pass is off");
        return;
    }
    off_file = runtime::swofty_root() + L"postfx.off";
    trace_file = runtime::swofty_root() + L"postfx.trace";
    swofty_root_dir = runtime::swofty_root();
    bool legacy = config.load_passes();
    config.load_effects();
    config.active_file = runtime::swofty_root() + L"shaders\\active.json";
    if (!legacy && config.effects.empty() && GetFileAttributesW(config.active_file.c_str()) == INVALID_FILE_ATTRIBUTES) {
        swofty::log("[postfx] the postfx feature is on, but assets.json names no usable pass and there is no shader config");
        return;
    }
    config.check_active(GetTickCount64(), true);
    subscribe_rule_events();
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_CAPTURE", v, sizeof v) && v[0]) {
        capture.configure(v, runtime::swofty_root() + L"postfx.capture");
    }
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_CLIP", v, sizeof v) && v[0]) {
        clip.configure(v, runtime::swofty_root() + L"postfx.clip");
    }
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_BURST_DUMP", v, sizeof v) && v[0])
        burst.configure(v);
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_TRACE", v, sizeof v) && v[0])
        report.trace_next(unsigned(std::strtoul(v, nullptr, 10)));
    if (GetEnvironmentVariableA("SWOFTY_POSTFX_ORDER", v, sizeof v) && v[0])
        placement.order_left = unsigned(std::strtoul(v, nullptr, 10));
    config.refresh_live();
    d3d12::on_setname(hook_setname);
    d3d12::on_barrier(hook_barrier);
    d3d12::on_exec(hook_exec);
    d3d12::on_present(hook_present);
    d3d12::on_reset(hook_reset);
    d3d12::on_render_targets(hook_render_targets);
    d3d12::on_create_rtv(hook_create_rtv);
    d3d12::ProbeScope probe;
    bool hooked = d3d12::arm_device();
    if (!hooked) {
        swofty::log("[postfx] the core's D3D12 device hooks did not arm; post-processing is off");
        return;
    }
    device_armed = true;
    placement.state_armed = d3d12::arm_list_state();
    if (!placement.state_armed)
        swofty::log("[postfx] the command list state hooks did not arm; passes run only between the game's lists");
    if (!d3d12::arm_swapchain())
        swofty::log("[postfx] the swapchain hooks did not arm; toggle keys, live settings and traces are off");
    swofty::log("[postfx] %zu pass(es) and %zu texture(s) wait for the game's picture%s%s", config.passes.size(),
                config.textures.size(), capture.dir().empty() ? "" : "; captures go to ", capture.dir().c_str());
}

}

namespace runtime {

void postfx_setup() { render::postfx::pipeline.setup(); }

}

namespace render::postfx {

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
    using namespace render::postfx;
    if (!addon_id || !*addon_id || !pass || !*pass || (size && !data))
        return -1;
    if (size > DATA_MAX)
        return -2;
    if (addon_at(__builtin_return_address(0)) != lower(addon_id))
        return -3;
    std::string key = DataStore::key(addon_id, pass);
    {
        std::lock_guard<std::mutex> g(pipeline.recorder.data_lock);
        if (!pipeline.recorder.pass_data.is_listed()) {
            bool kept = pipeline.recorder.pass_data.set_early(key, data, size);
            pipeline.recorder.any_data.store(!pipeline.recorder.pass_data.empty(), std::memory_order_relaxed);
            return kept ? 1 : -4;
        }
    }
    bool known = false, running = false;
    {
        std::lock_guard<std::recursive_mutex> g(pipeline.lock);
        for (const RenderPass &p : pipeline.config.passes)
            if (p.addon_id == addon_id && p.name == pass) {
                known = true;
                running = running || (pipeline.device_armed && !pipeline.broken && !p.off && !p.ruled_off && p.on &&
                                      !pipeline.config.addon_off(p.addon_id));
            }
    }
    if (!known)
        return -4;
    {
        std::lock_guard<std::mutex> g(pipeline.recorder.data_lock);
        pipeline.recorder.pass_data.set(key, data, size);
        pipeline.recorder.any_data.store(!pipeline.recorder.pass_data.empty(), std::memory_order_relaxed);
    }
    return running ? 0 : 1;
}
