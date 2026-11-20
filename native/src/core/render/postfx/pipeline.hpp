#pragma once
#include "core/hud/hardware_settings.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/postfx/capture.hpp"
#include "core/render/postfx/depth_copy.hpp"
#include "core/render/postfx/frame_textures.hpp"
#include "core/render/postfx/gpu.hpp"
#include "core/render/postfx/list_placement.hpp"
#include "core/render/postfx/pass_recorder.hpp"
#include "core/render/postfx/report.hpp"
#include "core/render/postfx/resource_watch.hpp"
#include "core/render/postfx/rules.hpp"
#include "core/render/postfx/shader_config.hpp"
#include "core/render/postfx/frame_logic.hpp"

#include <windows.h>

#include <atomic>
#include <mutex>
#include <string>
#include <vector>

namespace render::postfx {

// The post-processing pipeline: everything the D3D12 hooks, the flow tick and the addons' calls reach, under one
// recursive lock. There is one, a namespace-scope object of postfx.cpp; its parts reach each other through it.
class Pipeline {
public:
    std::recursive_mutex lock;
    ResourceWatch watched;
    ShaderConfig config{*this};
    ListPlacement placement{*this};
    BurstProbe burst;
    GpuContext gpu;
    PassRecorder recorder{*this};
    FrameTextures frame{*this};
    RetireQueue retired;
    FrameReport report;
    PictureCapture capture;
    ClipRecorder clip;
    DepthCopy depth_copy;
    RuleDriver rules;

    bool killed = false, broken = false;
    std::wstring off_file, trace_file, swofty_root_dir;
    std::atomic<bool> fg_on{true};
    ULONGLONG started = 0, last_check = 0, live_at = 0, frame_at = 0;
    std::atomic<ULONGLONG> present_at{0};
    FrameClock frame_clock;
    float frametime = 0;
    unsigned frame_count = 0, ran_total = 0;
    ULONGLONG trace_checked = 0;
    std::atomic<bool> device_armed{false}; // the device hooks armed: passes can run at all

    void setup();
    bool killed_now();
    void retire(std::vector<IUnknown *> objects);
    void on_present(IDXGISwapChain3 *sc, const d3d12::Swapchain *known, void *caller);
    void game_frame(const void *was, ULONGLONG now);
    void flow_frame(double, void *);

private:
    void subscribe_rule_events();
    void check_trace_file(ULONGLONG now);
    void note_idle();
    void end_frame(ULONGLONG now);
    void tick(ULONGLONG now);
    void stat_line(ULONGLONG now);
    bool frame_gen_on(ULONGLONG now);
    std::string own_frames_by(const d3d12::Swapchain *known, void *caller);
    void present_fallback(IDXGISwapChain3 *sc, const d3d12::Swapchain *known);
    void passes_listed();

    hud::hardware_settings::Poll frame_gen_poll;
    bool frame_gen = true;
    void *own_frames_caller = nullptr;
    std::string own_frames_module;
    bool told_copied = false, told_generated = false, told_flow = false;
};

}
