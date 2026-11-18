#pragma once
#include <windows.h>

#include "core/hud/hud_drawlist.hpp"
#include "core/hud/overlay/before_fg.hpp"
#include "core/hud/overlay/capture.hpp"
#include "core/hud/overlay/input.hpp"
#include "core/hud/overlay/renderer.hpp"
#include "core/hud/overlay/settings.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <atomic>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace hud::overlay {

constexpr ULONGLONG STATS_MS = 60000;

struct Stats {
    ULONGLONG since = 0;
    unsigned drawn = 0, busy = 0;
};

// The present hook draws only on a swapchain whose creation it saw (the DXGI factory's CreateSwapChain* slots are
// hooked before the game makes its window) and only on the queue that swapchain was created with, so
// its work runs after the game's frame and before the present on the same queue. Swapchains that a
// frame-generation module created or presents are left alone. A device removal stops the overlay for the rest of
// the session.
struct Overlay {
    std::mutex reg_lock;
    hud::drawlist::Registry reg;
    // The id|path keys overlay_image could not load; they are not tried again.
    std::mutex image_failed_lock;
    std::vector<std::string> image_failed;
    bool implemented = false;
    std::string device_hook_rival_seen;
    std::vector<std::pair<void *, bool>> told;
    std::vector<void *> unknown_told;
    Renderer renderer;
    std::mutex render_lock;
    bool dropped_told = false;
    std::atomic<bool> armed{false}, off{false}, lost_once{false};
    // From setup deciding to arm until the arm is done, which may be on the arm worker; ov_available counts it as armed.
    std::atomic<bool> arming{false};
    std::atomic<ID3D12Device *> live_dev{nullptr};
    std::atomic<ULONGLONG> last_draw_ms{0};
    std::string drawn_by;
    Stats stats;
    FgWatch fg_watch;
    OutputMode output;
    FrameCapture capture;
    Input input;
    BeforeFrameGen before_fg;
    bool device_hooked = false;
    bool before_fg_tried = false;
    bool menu_frame_told = false;

    void setup();
    void arm_decided();
    void device_lost(HRESULT present_hr);
    void draw_target(IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const Renderer::Target &t, const std::vector<Vertex> &verts,
                     const std::vector<hud::drawlist::Batch> &batches, UINT color_space, ULONGLONG now);
    void draw_frame(IDXGISwapChain3 *sc, const render::d3d12::Swapchain *known, void *);
    void presented(IDXGISwapChain3 *, HRESULT hr);
    bool arm_for_unread_queue(IDXGISwapChain3 *sc, void *caller);

private:
    void flush_stats(ULONGLONG now);
    hud::drawlist::Frame &build_frame(const std::vector<hud::drawlist::Paintsub> &subs, UINT width, UINT height, ULONGLONG now);
    void paint_frame(IDXGISwapChain3 *sc, ID3D12CommandQueue *q, const std::vector<hud::drawlist::Paintsub> &subs,
                     const DXGI_SWAP_CHAIN_DESC &desc, ULONGLONG now, UINT color_space);
    bool resolve_d3d12();
    bool arm();
    void arm_before_fg(const char *why);
};

extern Overlay overlay;

}
