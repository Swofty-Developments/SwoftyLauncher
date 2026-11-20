#include "core/runtime/runtime.hpp"

#include "core/hud/overlay/overlay.hpp"
#include "core/frontend/main_menu.hpp"
#include "core/hud/arm_policy.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/diagnostics/steps.hpp"
#include "core/hud/room_cover.hpp"
#include "core/services/lap_host.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <swofty/mixin.hpp>

#include <d3d12.h>
#include <dxgi1_6.h>

#include <algorithm>
#include <atomic>
#include <mutex>
#include <optional>
#include <vector>

namespace hud::overlay {

constinit Overlay overlay;

namespace {

using CreateFactoryFn = HRESULT(WINAPI *)(REFIID, void **);

void present_hook(IDXGISwapChain3 *sc, const render::d3d12::Swapchain *known, void *caller)
{
    overlay.draw_frame(sc, known, caller);
}

void presented_hook(IDXGISwapChain3 *sc, HRESULT hr) { overlay.presented(sc, hr); }

bool unread_queue_hook(IDXGISwapChain3 *sc, void *caller) { return overlay.arm_for_unread_queue(sc, caller); }

}

void Overlay::device_lost(HRESULT present_hr)
{
    if (lost_once.exchange(true))
        return;
    off.store(true);
    ID3D12Device *dev = live_dev.load();
    HRESULT reason = dev ? dev->GetDeviceRemovedReason() : S_OK;
    ULONGLONG now = GetTickCount64(), last = last_draw_ms.load();
    if (!hud::arm_policy::removal_after_draw(now, last)) {
        swofty::log("[overlay] the GPU was removed (present 0x%08lx, reason 0x%08lx) %s; the overlay stops drawing",
                    static_cast<unsigned long>(present_hr), static_cast<unsigned long>(reason),
                    last ? "more than 10 s after the overlay last drew" : "and the overlay had not drawn");
        return;
    }
    swofty::log("[overlay] the GPU was removed (present 0x%08lx, reason 0x%08lx) %.1f s after the overlay last drew "
                "(swapchain by %s); the overlay stops drawing",
                static_cast<unsigned long>(present_hr), static_cast<unsigned long>(reason), double(now - last) / 1000,
                drawn_by.c_str());
}

void Overlay::flush_stats(ULONGLONG now)
{
    std::lock_guard<std::mutex> g(render_lock);
    if (!stats.since)
        stats.since = now;
    if (now - stats.since < STATS_MS)
        return;
    if (stats.drawn || stats.busy)
        swofty::log("[overlay] last %llu s: %u frame(s) drawn, %u skipped while the GPU still held the slot; GPU %.3f ms "
                    "per drawn frame (%u timed)",
                    static_cast<unsigned long long>((now - stats.since) / 1000), stats.drawn, stats.busy,
                    renderer.gpu_n ? renderer.gpu_ms / renderer.gpu_n : 0.0, renderer.gpu_n);
    renderer.gpu_ms = 0;
    renderer.gpu_n = 0;
    stats = {now, 0, 0};
}

void Overlay::draw_target(IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const Renderer::Target &t, const std::vector<Vertex> &verts,
                          const std::vector<hud::drawlist::Batch> &batches, UINT color_space, ULONGLONG now)
{
    static const uint32_t probe = perf::probe("core", "overlay record");
    perf::Scope timed(probe);
    std::lock_guard<std::mutex> g(render_lock);
    UINT mode = settings().killed("hdr") ? 0 : output.pick(sc, t.fmt, color_space);
    switch (renderer.render(t, q, verts, batches, mode, settings().paper_white)) {
    case Renderer::drawn:
        if (!stats.drawn && !last_draw_ms.load())
            swofty::log("[overlay] first overlay frame submitted: %zu vertices", verts.size());
        last_draw_ms.store(now);
        stats.drawn++;
        break;
    case Renderer::busy:
        stats.busy++;
        break;
    case Renderer::broken:
        renderer.failed = true;
        swofty::log("[overlay] recording the overlay for swapchain %p failed (format %d); the overlay stops drawing",
                    static_cast<void *>(sc), int(t.fmt));
        break;
    }
}

hud::drawlist::Frame &Overlay::build_frame(const std::vector<hud::drawlist::Paintsub> &subs, UINT width, UINT height,
                                           ULONGLONG now)
{
    static const uint32_t paint_probe = perf::probe("core", "overlay paint callbacks");
    // Not a member: the frame's text cache (std::unordered_map) has no constexpr constructor and the overlay is constinit.
    static hud::drawlist::Frame frame;
    {
        perf::Scope painting(paint_probe);
        input.frame(subs, float(width), float(height), now);
        hud::drawlist::build(subs, frame, float(width), float(height), settings().off);
        input.draw_cursor(frame);
        hud::room_cover::paint(frame, now);
        frontend::main_menu::paint(frame, now);
        runtime::lap_host::notice_paint(frame, now);
    }
    flush_stats(now);
    if (frame.dropped && !dropped_told) {
        dropped_told = true;
        swofty::log("[overlay] a frame's paint callbacks drew more than %u vertices; %u primitive(s) past that were "
                    "dropped",
                    hud::drawlist::MAX_VERTS, frame.dropped);
    }
    return frame;
}

void Overlay::paint_frame(IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const std::vector<hud::drawlist::Paintsub> &subs,
                          const DXGI_SWAP_CHAIN_DESC &desc, ULONGLONG now, UINT color_space)
{
    hud::drawlist::Frame &frame = build_frame(subs, desc.BufferDesc.Width, desc.BufferDesc.Height, now);
    if (frame.verts.empty())
        return;
    ID3D12Resource *buf = nullptr;
    if (FAILED(sc->GetBuffer(sc->GetCurrentBackBufferIndex(), IID_PPV_ARGS(&buf))) || !buf) {
        renderer.failed = true;
        swofty::log("[overlay] recording the overlay for swapchain %p failed (format %d); the overlay stops drawing",
                    static_cast<void *>(sc), int(desc.BufferDesc.Format));
        return;
    }
    Renderer::Target t{buf, desc.BufferDesc.Format, desc.BufferDesc.Width, desc.BufferDesc.Height, D3D12_RESOURCE_STATE_PRESENT};
    draw_target(sc, q, t, frame.verts, frame.batches, color_space, now);
    buf->Release();
}

void Overlay::draw_frame(IDXGISwapChain3 *sc, const render::d3d12::Swapchain *known, void *)
{
    if (off.load() || renderer.failed)
        return;
    static const uint32_t probe = perf::probe("core", "overlay present");
    perf::Scope timed(probe);
    std::vector<hud::drawlist::Paintsub> subs;
    if (implemented) {
        std::lock_guard<std::mutex> g(reg_lock);
        subs = reg.subs();
    }
    bool cover = hud::room_cover::active();
    bool menu = frontend::main_menu::drawing();
    bool barred = runtime::lap_host::notice_drawing();
    if (menu) {
        if (!std::exchange(menu_frame_told, true))
            swofty::log(
                "[main-menu] the present hook passed its first frame with the main menu panel due (%zu paint callback(s))",
                subs.size());
    }
    if (subs.empty() && !capture.on && !cover && !menu && !barred) {
        before_fg.drop();
        return;
    }
    if (!known) {
        if (std::find(unknown_told.begin(), unknown_told.end(), sc) == unknown_told.end()) {
            unknown_told.push_back(sc);
            if (render::d3d12::learning(sc))
                swofty::log("[overlay] swapchain %p was created outside the core's swapchain creation hooks and its "
                            "present queue is not known yet; the overlay draws on it once ExecuteCommandLists names "
                            "the queue",
                            static_cast<void *>(sc));
            else
                swofty::log("[overlay] swapchain %p was created before the core hooked swapchain creation, so its "
                            "present queue is unknown; the overlay does not draw on it",
                            static_cast<void *>(sc));
        }
        return;
    }
    const render::d3d12::Swapchain &c = *known;
    std::pair<void *, bool> seen{sc, !c.fg.empty()};
    bool fresh = std::find(told.begin(), told.end(), seen) == told.end();
    if (fresh)
        told.push_back(seen);
    ULONGLONG now = GetTickCount64();
    // FidelityFX's loader, like Streamline's hook, passes frames through while frame generation is off.
    const bool ffx = hud::arm_policy::ffx_loader(c.fg);
    const bool fg_named = !c.fg.empty() && !ffx;
    const hud::arm_policy::Gate gate =
        hud::arm_policy::gate_for_swapchain(c.fg, c.streamline, c.armed_late, [&] { return fg_watch.frame_gen_off(now); });
    const UINT color_space = c.color_space_set ? c.color_space : hud::arm_policy::COLOR_SPACE_UNSET;
    if (!hud::arm_policy::draws_at_present(gate)) {
        if (!hud::arm_policy::draws_before_present(gate, device_hooked)) {
            before_fg.set_active(false);
            before_fg.drop();
            std::string why = device_hook_rival_seen.empty()
                                  ? "frame generation was off when the game started, or SWOFTY_OVERLAY_FG=0 / "
                                    "\"overlay_fg\": false"
                                  : device_hook_rival_seen + " hooks the same D3D12 entry points";
            if (fg_named) {
                if (fresh)
                    swofty::log("[overlay] frame generation (%s) owns swapchain %p and the core's D3D12 device hooks "
                                "did not arm (drawing before frame generation is off: %s); the overlay does not draw on it",
                                c.fg.c_str(), static_cast<void *>(sc), why.c_str());
            } else if (!fg_watch.told || fresh) {
                swofty::log("[overlay] swapchain %p was created through %s, frame generation is not off in the game's "
                            "settings and drawing before frame generation is off (%s); the overlay does not draw on it",
                            static_cast<void *>(sc),
                            ffx ? "FidelityFX (amd_fidelityfx_dx12.dll)" : "Streamline's frame generation hook (sl.dlss_g.dll)",
                            why.c_str());
            }
            return;
        }
        DXGI_SWAP_CHAIN_DESC desc{};
        if (FAILED(sc->GetDesc(&desc)))
            return;
        if (fresh) {
            swofty::log("[overlay] frame generation owns swapchain %p; the overlay draws into the game's back buffer "
                        "before Present instead",
                        static_cast<void *>(sc));
            if (implemented)
                input.hook_window(desc.OutputWindow);
        }
        before_fg.set_active(true);
        bool addons = !subs.empty() && !settings().killed("paint");
        if (addons || cover || menu || barred) {
            hud::drawlist::Frame &frame = build_frame(addons ? subs : std::vector<hud::drawlist::Paintsub>(),
                                                      desc.BufferDesc.Width, desc.BufferDesc.Height, now);
            before_fg.keep(sc, frame, desc, color_space, now);
        } else {
            before_fg.drop();
        }
        return;
    }
    before_fg.set_active(false);
    before_fg.drop();
    if (!renderer.ready) {
        if (!renderer.init(c.queue)) {
            renderer.failed = true;
            renderer.shutdown();
            swofty::log("[overlay] renderer init failed; the overlay stays armed but draws nothing");
            return;
        }
        live_dev.store(renderer.dev);
    }
    if (!renderer.same_device(c.queue)) {
        renderer.failed = true;
        swofty::log("[overlay] swapchain %p presents on queue %p of another device than the overlay renderer's; the "
                    "overlay stops drawing",
                    static_cast<void *>(sc), static_cast<void *>(c.queue));
        return;
    }
    if (renderer.dev->GetDeviceRemovedReason() != S_OK) {
        device_lost(S_OK);
        return;
    }
    DXGI_SWAP_CHAIN_DESC desc{};
    if (FAILED(sc->GetDesc(&desc)))
        return;
    if (!(desc.BufferUsage & DXGI_USAGE_RENDER_TARGET_OUTPUT)) {
        if (fresh)
            swofty::log("[overlay] swapchain %p back buffers are not render targets (usage 0x%x); the overlay does not "
                        "draw on it",
                        static_cast<void *>(sc), unsigned(desc.BufferUsage));
        return;
    }
    if (fresh && implemented)
        input.hook_window(desc.OutputWindow);
    if (fresh) {
        drawn_by = c.by;
        swofty::log("[overlay] drawing on swapchain %p with the direct queue %p from %s (%ux%u, format %d, %u "
                    "buffer(s), created by %s)",
                    static_cast<void *>(sc), static_cast<void *>(c.queue), c.how.c_str(), desc.BufferDesc.Width,
                    desc.BufferDesc.Height, int(desc.BufferDesc.Format), desc.BufferCount, c.by.c_str());
    }
    bool addons = !subs.empty() && !settings().killed("paint");
    if (addons || cover || menu || barred)
        paint_frame(sc, c.queue, addons ? subs : std::vector<hud::drawlist::Paintsub>(), desc, now, color_space);
    if (capture.on && !settings().killed("capture"))
        capture.step(renderer, sc, c.queue, desc, now);
}

void Overlay::presented(IDXGISwapChain3 *, HRESULT hr)
{
    if ((hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET) && armed.load())
        device_lost(hr);
}

// d3d12.dll / dxgi.dll are resolved at arm time so the core keeps no hard import on them: it loads
// unchanged where the overlay feature is off or D3D12 is absent.
bool Overlay::resolve_d3d12()
{
    HMODULE d3d12 = LoadLibraryA("d3d12.dll");
    HMODULE dxgi = LoadLibraryA("dxgi.dll");
    auto create_device = d3d12 ? reinterpret_cast<PFN_D3D12_CREATE_DEVICE>(GetProcAddress(d3d12, "D3D12CreateDevice")) : nullptr;
    renderer.serialize_root =
        d3d12 ? reinterpret_cast<PFN_D3D12_SERIALIZE_ROOT_SIGNATURE>(GetProcAddress(d3d12, "D3D12SerializeRootSignature"))
              : nullptr;
    auto create_factory = dxgi ? reinterpret_cast<CreateFactoryFn>(GetProcAddress(dxgi, "CreateDXGIFactory1")) : nullptr;
    if (create_device && renderer.serialize_root && create_factory)
        return true;
    swofty::log("[overlay] d3d12.dll / dxgi.dll entry points are unavailable; the present hook is not armed");
    return false;
}

bool Overlay::arm_for_unread_queue(IDXGISwapChain3 *sc, void *caller)
{
    if (device_hooked || before_fg_tried)
        return device_hooked;
    std::string by = render::d3d12::module_of(caller);
    hud::arm_policy::OnDemandArm on = hud::arm_policy::on_demand_arm(loaded_modules(), by, settings().before_fg_forced);
    if (!on.arms()) {
        swofty::log("[overlay] swapchain %p presents from %s and its present queue cannot be read; the core's device "
                    "hooks stay off (%s), so the overlay does not draw on it",
                    static_cast<void *>(sc), by.empty() ? "an unknown module" : by.c_str(), on.refused.c_str());
        return false;
    }
    swofty::log("[overlay] frame generation source: %s; swapchain %p presents from %s and its present queue cannot be "
                "read, so the core's device hooks arm now to learn the queue from ExecuteCommandLists, and the overlay "
                "draws on it from the next frame (SWOFTY_OVERLAY_FG=0 / \"overlay_fg\": false keeps them off)",
                on.source.c_str(), static_cast<void *>(sc), by.empty() ? "an unknown module" : by.c_str());
    std::string why = "a swapchain whose present queue cannot be read reached Present with " + on.source + " in the process";
    arm_before_fg(why.c_str());
    return device_hooked;
}

bool Overlay::arm()
{
    perf::Stopwatch steps;
    bool resolved = resolve_d3d12();
    swofty::log(perf::STEP_LINE, steps.lap_ms(), "overlay arm resolve d3d12");
    if (!resolved)
        return false;
    render::d3d12::on_present(present_hook);
    render::d3d12::on_presented(presented_hook);
    render::d3d12::on_unread_queue(unread_queue_hook);
    std::optional<render::d3d12::ProbeScope> probe(std::in_place);
    bool swapchain = render::d3d12::arm_swapchain();
    swofty::log(perf::STEP_LINE, steps.lap_ms(), "overlay arm swapchain (probe device made)");
    if (before_fg_wanted()) {
        arm_before_fg("frame generation is on in the game's settings, or SWOFTY_OVERLAY_FG / \"overlay_fg\"");
    } else if (std::vector<std::string> mods = loaded_modules(); hud::arm_policy::device_hooks_wanted_for(mods)) {
        swofty::log("[overlay] %s is in the process; the game's swapchain may be made outside the creation hooks, so "
                    "the core's device hooks arm to learn its present queue from ExecuteCommandLists",
                    hud::arm_policy::present_layer_in(mods).c_str());
        arm_before_fg("the NVIDIA app's present layer is in the process");
    }
    swofty::log(perf::STEP_LINE, steps.lap_ms(), "overlay arm device hooks");
    probe.reset();
    swofty::log(perf::STEP_LINE, steps.lap_ms(), "overlay arm probe device release");
    swofty::log(perf::STEP_LINE, steps.total_ms(), "overlay arm total");
    return swapchain;
}

void Overlay::arm_before_fg(const char *why)
{
    if (std::exchange(before_fg_tried, true))
        return;
    std::string rival = device_hook_rival();
    if (!hud::arm_policy::device_hooks_allowed(rival, settings().before_fg_forced)) {
        device_hook_rival_seen = rival;
        swofty::log("[overlay] %s hooks the D3D12 entry points the core's device hooks use, so they are not armed: the "
                    "overlay draws at Present as before 1.8.38 and does not draw while frame generation is on "
                    "(SWOFTY_OVERLAY_FG=1 / \"overlay_fg\": true arms them anyway)",
                    rival.c_str());
        return;
    }
    if (!rival.empty())
        swofty::log("[overlay] %s is in the process; the core's device hooks arm anyway on SWOFTY_OVERLAY_FG / "
                    "\"overlay_fg\"",
                    rival.c_str());
    before_fg.subscribe();
    device_hooked = render::d3d12::arm_device();
    if (!device_hooked)
        swofty::log("[overlay] the core's D3D12 device hooks did not arm; the overlay cannot draw when frame "
                    "generation owns the swapchain");
    else
        swofty::log("[overlay] drawing before frame generation is on (%s)", why);
}

void Overlay::setup()
{
    if (settings().killed("all")) {
        swofty::log("[overlay] off: SWOFTY_OVERLAY_KILL or \"overlay_kill\" in swofty/assets.json lists all");
        return;
    }
    capture.setup();
    bool on = settings().gate;
    bool cover = hud::room_cover::setup();
    bool menu = frontend::main_menu::wants_overlay();
    bool barred = runtime::lap_host::notice_wanted();
    hud::arm_policy::ArmReasons reasons{on, capture.on, cover, menu || barred};
    if (!reasons.any()) {
        swofty::log("[overlay] the overlay feature is experimental and off; the HUD present hook is not armed");
        return;
    }
    std::string rival = device_hook_rival();
    if (hud::arm_policy::present_hook_yields(reasons, rival, settings().rival_forced)) {
        if (reasons.menu_only())
            swofty::log("[main-menu] %s hooks the game's Present, so the present hook is not armed for the main menu "
                        "panel and video card: they do not draw while RivaTuner Statistics Server is running (the "
                        "home tiles still work)",
                        rival.c_str());
        else
            swofty::log("[overlay] off: %s hooks the game's Present, so the present hook is not armed and nothing "
                        "draws over the game while RivaTuner Statistics Server is running:%s%s%s the game "
                        "and the addons' other features run as before. Close RivaTuner Statistics Server and MSI "
                        "Afterburner to draw again (SWOFTY_OVERLAY_RIVAL=1 / \"overlay_rival\": true arms the hook "
                        "anyway)",
                        rival.c_str(), on ? " overlay addons (on_paint is unavailable)," : "",
                        cover ? " the direct start's cover," : "",
                        menu || barred ? " the main menu panel, video card and Time Trial warning," : "");
        return;
    }
    if (!rival.empty())
        swofty::log("[overlay] %s is in the process; the present hook arms anyway (%s)", rival.c_str(),
                    capture.on ? "SWOFTY_FRAME_CAPTURE" : "SWOFTY_OVERLAY_RIVAL / \"overlay_rival\"");
    if (menu)
        swofty::log("[main-menu] the present hook arms for the main menu panel and video card");
    if (barred)
        swofty::log("[tt-barred] the present hook arms for the Time Trial barred addons warning");
    if (cover)
        swofty::log("[cover] a direct start is requested; the present hook arms for its cover screen");
    if (!on && capture.on)
        swofty::log("[overlay] the overlay is off; the present hook arms for SWOFTY_FRAME_CAPTURE only");
    implemented = on;
    arming.store(true);
}

void Overlay::arm_decided()
{
    if (!arming.load())
        return;
    if (arm())
        armed.store(true);
    else
        swofty::log("[overlay] the present hook did not arm; on_paint callbacks will not be drawn");
    arming.store(false);
}

}

using hud::overlay::overlay;

namespace runtime {

void hud_setup() { overlay.setup(); }

void hud_arm() { overlay.arm_decided(); }

void hud_hook_window(HWND wnd) { overlay.input.hook_window(wnd); }

void hud_fatal()
{
    ID3D12Device *dev = overlay.live_dev.load();
    if (dev && !overlay.lost_once.load() && dev->GetDeviceRemovedReason() != S_OK)
        overlay.device_lost(S_OK);
}

bool frame_capture_request(const std::wstring &path) { return overlay.capture.request_frame(path); }

int frame_capture_poll(std::string *why) { return overlay.capture.poll_request(why); }

}
