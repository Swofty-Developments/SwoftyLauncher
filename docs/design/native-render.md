---
title: Native render and front end
---

# Native render and front end

The core draws over the game, runs post-process passes and adds screens to the front end. The code is in `native/src/core/render/`, `native/src/core/hud/` and `native/src/core/frontend/`. This page covers how the overlay arms, how a post-process pass is recorded, the probe device scope, and how to add a screen.

## How the overlay arms

The `hud` subsystem calls `Overlay::setup` when the core is ready (`hud/overlay.cpp`). It decides whether anything needs the present hook, then arms it:

```cpp
void Overlay::setup()
{
    if (settings().killed("all"))                  // SWOFTY_OVERLAY_KILL / "overlay_kill" lists all
        return;
    hud::guard::ArmReasons reasons{settings().gate, capture.on, room_cover::setup(), menu || barred};
    if (!reasons.any())                            // no overlay addon, capture, cover, menu panel or TT warning
        return;
    std::string rival = device_hook_rival();       // RivaTuner Statistics Server
    if (hud::guard::present_hook_yields(reasons, rival, settings().rival_forced))
        return;
    if (arm())
        armed.store(true);
}

bool Overlay::arm()
{
    resolve_d3d12();
    d3d12hooks::on_present(present_hook);
    d3d12hooks::on_presented(presented_hook);
    d3d12hooks::on_unread_queue(unread_queue_hook);
    std::optional<d3d12hooks::ProbeScope> probe(std::in_place);
    bool swapchain = d3d12hooks::arm_swapchain();
    if (before_fg_wanted())                        // frame generation on, or SWOFTY_OVERLAY_FG / "overlay_fg"
        arm_before_fg("...");                      // device hooks: draw in the game's lists, before frame generation
    else if (hud::guard::device_hooks_wanted_for(loaded_modules()))
        arm_before_fg("the NVIDIA app's present layer is in the process");
    probe.reset();
    return swapchain;
}
```

A swapchain whose present queue the core cannot read arms the device hooks late, at its first Present (`hud::guard::on_demand_arm`), and the overlay draws on it from the next frame.

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

`postfx::Pipeline` (`render/postfx/pipeline.hpp`) owns every part under one recursive lock. Its `PassRecorder` records a frame's passes into one of `FRAMES` slots, in one of two places:

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
    d3d12hooks::exec_original(q, 1u, one);
    q->Signal(s.fence, ++s.fence_value);
    return true;
}
```

`record(w, k)` writes the same commands in both places:

```cpp
l->CopyResource(work.orig, w.back);                // the frame before any pass
for (int index : live) {                           // config.live_passes(view::current())
    l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, ...);
    l->SetPipelineState(p.pso);
    l->SetComputeRoot32BitConstants(0, CONSTANTS, c, 0);
    l->SetComputeRootDescriptorTable(1, ...);      // t0 picture, t1 depth, t2..t9 inputs, t10 data, u0 output
    l->Dispatch(groups(ow, p.threads[0]), groups(oh, p.threads[1]), 1);
    l->EndQuery(queries, D3D12_QUERY_TYPE_TIMESTAMP, ...);
}
l->CopyResource(w.back, cur);                      // the last output back into the back buffer
```

Each resource goes back to the state `Where` gave for it. In the game's list, `d3d12hooks::list_state` saves the list's root signature, heaps and pipeline first, and the recorder restores them after.

## The probe device scope

Hooking a D3D12 vtable needs a device to read the vtable from. One probe device is shared by the ready subsystems from `hud` to `rtas guard` (`runtime/subsystems.cpp`):

```cpp
{"hud", Start::ready, hud_setup, Probe::open},
{"focus", Start::ready, focus_setup},
{"postfx", Start::ready, postfx_setup},
{"grade", Start::ready, grade_setup},
{"srv clamp", Start::ready, srv_clamp_setup},
{"rtas guard", Start::ready, rtas_guard_setup, Probe::close},
```

The runtime opens a `d3d12hooks::ProbeScope` at `Probe::open` and drops it after `Probe::close`. Inside it, the first `probe_device()` call makes the device and every later call gets the same one (`render/probe_logic.hpp`). Code that probes outside the table opens its own scope:

```cpp
d3d12hooks::ProbeScope probe;                       // nests: the device lives until the outermost scope closes
ID3D12Device *dev = d3d12hooks::probe_device();
void **vt = dev ? *reinterpret_cast<void ***>(dev) : nullptr;
release(dev);                                       // render/com.hpp
```

The log shows one pair per boot:

```
[perf] step 412.3 ms: d3d12 probe device made
[perf] step 3.1 ms: d3d12 probe device release
```

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
