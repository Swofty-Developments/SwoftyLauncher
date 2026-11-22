---
title: Native render and front end
---

# Native render and front end

The core draws over the game, runs post-process passes and adds screens to the front end. The code is in `native/src/core/render/`, `native/src/core/hud/` and `native/src/core/frontend/`. This page covers how the overlay arms, how a post-process pass is recorded, the probe device scope, the arm worker, and how to add a screen.

## How the overlay arms

The `hud` subsystem calls `Overlay::setup` when the core is ready (`hud/overlay.cpp`). It decides whether anything needs the present hook. The `hud arm` subsystem then arms it, in the stage or on the [arm worker](#the-arm-worker):

```cpp
void Overlay::setup()                              // "hud", always in the stage
{
    if (settings().killed("all"))                  // SWOFTY_OVERLAY_KILL / "overlay_kill" lists all
        return;
    hud::arm_policy::ArmReasons reasons{settings().gate, capture.on, hud::room_cover::setup(), menu || barred};
    if (!reasons.any())                            // no overlay addon, capture, cover, menu panel or TT warning
        return;
    std::string rival = device_hook_rival();       // RivaTuner Statistics Server
    if (hud::arm_policy::present_hook_yields(reasons, rival, settings().rival_forced))
        return;
    implemented = on;
    arming.store(true);
}

void Overlay::arm_decided()                        // "hud arm"
{
    if (!arming.load())
        return;
    if (arm())
        armed.store(true);
    arming.store(false);
}

bool Overlay::arm()
{
    resolve_d3d12();
    render::d3d12::on_present(present_hook);
    render::d3d12::on_presented(presented_hook);
    render::d3d12::on_unread_queue(unread_queue_hook);
    std::optional<render::d3d12::ProbeScope> probe(std::in_place);
    bool swapchain = render::d3d12::arm_swapchain();
    if (before_fg_wanted())                        // frame generation on, or SWOFTY_OVERLAY_FG / "overlay_fg"
        arm_before_fg("...");                      // device hooks: draw in the game's lists, before frame generation
    else if (hud::arm_policy::device_hooks_wanted_for(loaded_modules()))
        arm_before_fg("the NVIDIA app's present layer is in the process");
    probe.reset();
    return swapchain;
}
```

A swapchain whose present queue the core cannot read arms the device hooks late, at its first Present (`hud::arm_policy::on_demand_arm`), and the overlay draws on it from the next frame.

An addon's init can run while the arm is still on the worker. Every overlay call an addon can make then is safe before the arm finishes: none reads the renderer, the device or anything else the arm builds.

- `available()` counts an arm that is decided but not done (`arming`) as armed, so it gives what it gives after the arm. When the arm then fails, `on_paint` callbacks are never drawn, as after a device loss.
- `on_paint`, `remove`, `load_image` and `focus` check only `implemented`, which `setup` sets in the stage, and keep what they are given in the registry under `reg_lock`. The first frame after the arm draws it.
- The draw calls only write into a paint frame, and there is none before the arm.

A new call that reads what `arm()` builds must queue what it is given and replay it on the first frame, or answer as if the arm were done. It must never read a half-built object.

Every decision above is a pure function in `hud/arm_policy/`, included as `core/hud/arm_policy.hpp` and run by `native/test/hud_test.cpp`:

- `modules.hpp`: the frame generation, Streamline and FidelityFX modules, and which module made a call.
- `arm.hpp`: when the present hook and the device hooks arm, and which overlays they yield to.
- `present_queue.hpp`: learning a swapchain's queue from `ExecuteCommandLists`.
- `game_settings.hpp`: frame generation and HDR from the game's `hardware_settings_config.xml`.
- `gates.hpp`: the gate a swapchain draws through (`Direct`, `FrameGenNamed`, `FrameGenStreamline`).
- `draw_target.hpp`: when a back buffer can take the overlay, and the state it is left in.

The switches are read once into `hud::overlay::Settings` (`hud/overlay/settings.hpp`):

```cpp
const Settings &s = hud::overlay::settings();
s.gate;              // the overlay feature is on
s.killed("paint");   // SWOFTY_OVERLAY_KILL / "overlay_kill" lists this part, or all
s.before_fg_forced;  // SWOFTY_OVERLAY_FG / "overlay_fg": -1 unset, 0 off, 1 on
s.rival_forced;      // SWOFTY_OVERLAY_RIVAL / "overlay_rival"
s.paper_white;       // SWOFTY_OVERLAY_NITS, 350 by default
```

## How a post-process pass is recorded

`render::postfx::Pipeline` (`render/postfx/pipeline.hpp`) owns every part under one recursive lock. Its `PassRecorder` records a frame's passes into one of `FRAMES` slots, in one of two places:

- In the game's list. The barrier hook (`ListPlacement`) marks what each list does with the tonemap output, the back buffers, the HUD-less copy and the depth buffer, and records the passes at the HUD-less step.
- Between the game's lists. Where no list had such a step, the submission hook or Present records them into the slot's own list and submits it on the game's queue.

```cpp
bool PassRecorder::run_between(ID3D12CommandQueue *q, ID3D12Resource *back, D3D12_RESOURCE_STATES state,
                               ID3D12Resource *hudless, D3D12_RESOURCE_STATES hudless_state, const char *path)
{
    int k = pick_slot();
    Slot &s = slots[k];
    s.alloc->Reset();
    s.list->Reset(s.alloc, nullptr);
    Where w{s.list, false, back, state, hudless, hudless_state, pipeline.placement.depth_res,
            pipeline.placement.depth_state, path};
    bool made = record(w, k);
    s.list->Close();
    if (!made)
        return false;
    ID3D12CommandList *one[] = {s.list};
    render::d3d12::exec_original(q, 1u, one);
    q->Signal(s.fence, ++s.fence_value);
    return true;
}
```

`record(w, k)` writes the same commands in both places:

```cpp
l->CopyResource(work.orig, w.back);                // the frame before any pass
for (int index : live) {                           // config.live_passes(frontend::view::current())
    l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, ...);
    l->SetPipelineState(p.pso);
    l->SetComputeRoot32BitConstants(0, CONSTANTS, c, 0);
    l->SetComputeRootDescriptorTable(1, ...);      // t0 picture, t1 depth, t2..t9 inputs, t10 data, u0 output
    l->Dispatch(groups(ow, p.threads[0]), groups(oh, p.threads[1]), 1);
    l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, ...);
}
l->CopyResource(w.back, cur);                      // the last output back into the back buffer
```

Each resource goes back to the state `Where` gave for it. In the game's list, `render::d3d12::list_state` saves the list's root signature, heaps and pipeline first, and the recorder restores them after.

## The probe device scope

Hooking a D3D12 vtable needs a device to read the vtable from. One probe device is shared by the ready subsystems from `hud arm` to `rtas guard` (`runtime/subsystems.cpp`):

```cpp
{"hud", Start::ready, hud_setup},
{"focus", Start::ready, focus_setup},
{"hud arm", Start::ready, hud_arm, Probe::open},
{"postfx", Start::ready, postfx_setup},
{"grade", Start::ready, grade_setup},
{"srv clamp", Start::ready, srv_clamp_setup},
{"rtas guard", Start::ready, rtas_guard_setup, Probe::close},
```

The runtime opens a `render::d3d12::ProbeScope` at `Probe::open` and drops it after `Probe::close`. Inside it, the first `probe_device()` call makes the device and every later call gets the same one (`render/probe_logic.hpp`). Code that probes outside the table opens its own scope:

```cpp
render::d3d12::ProbeScope probe;                       // nests: the device lives until the outermost scope closes
ID3D12Device *dev = render::d3d12::probe_device();
void **vt = dev ? *reinterpret_cast<void ***>(dev) : nullptr;
release(dev);                                       // render/com.hpp
```

The log shows one pair per boot:

```
[perf] step 412.3 ms: d3d12 probe device made
[perf] step 3.1 ms: d3d12 probe device release
```

## The arm worker

The first D3D12 device in the process pays the driver load, so the block from `hud arm` to `rtas guard` can take seconds. `Runtime::start` asks `render::d3d12::arm_on_worker` (`render/d3d12/arm_gate.cpp`) to run the block on a worker, and the stage goes on to the next ready subsystem:

```cpp
std::span<const Subsystem> block = probe_block(all, i);   // hud arm .. rtas guard
bool early = runtime::load_stage() == "early" && !runtime::window_reached();
if (point == Start::ready && render::d3d12::arm_on_worker(ready_block_on_worker, early))
    ;                                                     // the worker runs the block, in the same order
else
    run_block(block, point, "");                          // in the stage, as before
```

Before the worker starts, `arm_on_worker` hooks the four creation exports the game makes its first device and factory with: `d3d12.dll!D3D12CreateDevice` and `dxgi.dll!CreateDXGIFactory`, `CreateDXGIFactory1` and `CreateDXGIFactory2`. Each hook asks the gate (`render/arm_gate_logic.hpp`) before calling the real function. While the worker runs:

| Caller | At the gate |
|---|---|
| The worker | Goes straight through: the probe device and factory |
| A thread that holds the loader lock | Goes straight through: the worker may need the lock to load the driver |
| A module under the Windows folder or the game's `D3D12` folder (the D3D12 and DXGI runtimes, the driver) | Goes straight through: the worker's device may wait for its threads |
| The core's own code on another thread (the profiler's adapter walk) | Waits for the worker, for at most 30 s |
| Anything else (the game, `sl.interposer.dll`, an overlay) | Waits for the worker, for at most 30 s |

Only the worker makes DXGI factories while it runs. Steam's overlay (`GameOverlayRenderer64.dll`) hooks the vtable of each new factory in its `CreateDXGIFactory` detours, and two threads that make their first factories at once both install that hook: its saved original can stay 0 or point back into its own detour, and the next `CreateSwapChainForHwnd` through it then calls address 0 or recurses until the stack runs out. The core's own waits are logged apart from the game's first call:

```
[d3d12] CreateDXGIFactory1 from the core's own code (thread 9124) while the D3D12 arm runs: waited for the arm, 486 ms
```

So the game sees what it sees with the block in the stage, in the same order: the same hooks, and the probe device made and released before its own device and factory exist. The factory hooks come out when the worker is done. The device hook stays until the game's first `D3D12CreateDevice`, which it times, and then comes out. A hook that something patched over meanwhile stays in. DRED's own `D3D12CreateDevice` hook sits behind the gate's and asks `render::d3d12::creation_caller` for the real caller, so it still tells the game's devices from the core's.

When the game calls `ExitProcess` while the worker runs, the `ExitProcess` hook waits, for at most 30 s, until the worker has finished and has taken the factory hooks out (`wait_arm_before_exit`, `Gate::wait_retired`). The wait is logged:

```
[d3d12] the game exits while the D3D12 arm runs on its worker; the arm finished after 147 ms
```

The block runs in the stage, with one line that says why, when any of these holds:

- `SWOFTY_ARM_SYNC=1` is set.
- An overlay that patches D3D12 entry points is loaded or running (`device_hook_rival`). The overlay's own yield rules ran in `hud` before this.
- The core did not load at the early stage, or the window stage has come: the game may hold a factory or device already, and the gate cannot hold calls on those.
- The stage holds the loader lock.
- An export hook did not install (every one already in comes out again), or the worker thread did not start.

```
[d3d12] the D3D12 arm runs on a worker; the game's D3D12CreateDevice and CreateDXGIFactory calls wait for it (SWOFTY_ARM_SYNC=1 runs it in the stage)
[d3d12] the D3D12 arm runs in the stage: SWOFTY_ARM_SYNC=1
```

The step lines split one boot's driver load between the probe and the game's first device:

```
[perf] step 2.4 ms: subsystems hud arm to rtas guard (on the arm worker), scheduled
[perf] step 412.3 ms: d3d12 probe device made
[perf] step 480.6 ms: subsystem hud arm (on the arm worker)
[perf] step 530.2 ms: d3d12 arm on its worker
[perf] step 120.4 ms: d3d12 first CreateDXGIFactory2 outside the arm, waiting for it
[perf] step 0.3 ms: d3d12 first CreateDXGIFactory2 outside the arm, the call
[perf] step 0.0 ms: d3d12 first D3D12CreateDevice outside the arm, waiting for it
[perf] step 95.1 ms: d3d12 first D3D12CreateDevice outside the arm, the call
```

What the block shares with the stage is safe to run on the worker: postfx and grade read the runtime config (read once), addon settings (`settings_state`, under its lock) and the events table (under its lock). Every D3D12 subscription is made in the block, in the same order. `SwoftyCorePostfxSetData` keeps an addon's bytes for later until postfx has listed its passes, as it does for an addon's on-load today.

## Adding a screen

A screen is a `.ui` package (a `!pkg` file) the game's screen system loads. An addon lists it in `operations.screens` of its `swofty.json` ([UI](/reference/std/ui)):

```json
"screens": [{ "name": "swofty_myprefix_page", "file": "ui/swofty_myprefix_page.ui", "flow": "flow_frontend", "view_model": true }]
```

The launcher writes the profile's screens into the `"ui"` block of `settings.json`, and `ScreenLoader::load_config` (`frontend/ui.cpp`) reads them at start. `flow` defaults to `flow_frontend`, `anchor` to `fe_home` (`settings_container` for `flow_options`), and `file` to `ui/<name>.ui`. `frontend/ui/` registers each screen with the game's screen system. When a route names a screen the game does not have and the name is declared or holds `swofty`, the route guard cancels the route and re-opens the last screen.

A screen with `"view_model": true` gets the core's view model whose prefix matches its name. To add one, put the class in `frontend/` and register it:

```cpp
#include <swofty/ui.hpp>

class MyPage : public swofty::ui::ViewModel {
public:
    void on_activated() override
    {
        set_text("title", "My addon");
        set_text("o0.label", "Field of view");
        set_int("o0.value", 3);
        set_bool("o0.is_slider", true);
    }

    bool on_command(const swofty::ui::Command &c) override
    {
        return c.name && std::string_view(c.name) == "apply";
    }
};

SWOFTY_VIEW_MODEL("swofty_myprefix_", MyPage)
```

The ADDON SETTINGS page (`frontend/addon_page.cpp`, prefix `swofty_addon_`) is the core's own example. `SWOFTY_UI_DIAG=1` logs each screen's build steps.
