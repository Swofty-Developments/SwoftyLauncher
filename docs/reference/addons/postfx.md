---
title: Post-process passes
description: Run your own HLSL shaders inside the game's frame, before the HUD, with textures, several passes and live settings.
---

# Post-process passes

A post-process pass is an HLSL compute shader in an addon. The launcher compiles it to DXIL with dxc when it builds the profile, and the in-game core runs it on every frame of the 3D picture inside the game's own frame, before the HUD draws. The HUD then draws over the processed picture.

Vignettes, grading, sharpening, grain, bloom and lens distortion are passes. Passes need launcher 1.5.4 or later; an earlier launcher rejects the manifest. Textures, inputs, raw passes, `when`, `enabled` and `postfx_toggle` need the launcher that documents them here.

## A pass in full

This is [`templates/postfx-pass`](https://github.com/Swofty-Developments/SwoftyLauncher/tree/main/templates/postfx-pass): a vignette with saturation, and a glow that blurs the bright parts at half resolution and adds them back.

```json [swofty.json]
{
  "schema_version": 1,
  "id": "example.vignette-pass",
  "name": "Vignette Pass",
  "version": "0.1.0",
  "author": "Your Name",
  "category": "shaders",
  "experimental_features": ["postfx"],
  "compat": { "game": "f1_25" },
  "options": [
    { "id": "vignette", "type": "slider", "label": "Vignette", "min": 0, "max": 100, "step": 5, "default": 60, "binds": "db" },
    { "id": "saturation", "type": "slider", "label": "Saturation", "min": 0, "max": 200, "step": 10, "default": 100, "binds": "db" },
    { "id": "glow", "type": "slider", "label": "Glow", "min": 0, "max": 100, "step": 5, "default": 30, "binds": "db" }
  ],
  "postfx_textures": [
    { "name": "half", "scale": 0.5, "format": "rgba16f" }
  ],
  "postfx": [
    { "shader": "fx/vignette.hlsl", "name": "vignette", "options": ["{{vignette}}", "{{saturation}}"] },
    { "shader": "fx/glow.hlsl", "entry": "down", "name": "glow-down", "inputs": ["original"], "output": "half" },
    { "shader": "fx/glow.hlsl", "entry": "up", "name": "glow-up", "options": ["{{glow}}"], "inputs": ["half"] }
  ]
}
```

```hlsl [fx/vignette.hlsl]
#include "swofty/postfx.hlsli"

float4 effect(float2 uv, uint2 pixel)
{
    float4 c = scene_at(int2(pixel));
    float3 rgb = to_linear(c.rgb);
    float strength = option(0) / 100, saturation = option(1) / 100;
    rgb = lerp(luminance(rgb).xxx, rgb, saturation);
    float2 d = uv - 0.5;
    d.x *= float(swofty_size.x) / float(swofty_size.y);
    rgb *= 1 - strength * smoothstep(0.3, 0.9, length(d));
    return float4(from_linear(rgb), c.a);
}
```

The build prints one line per pass:

```text
postfx: Vignette Pass's pass vignette runs as pass 1 before the HUD (6336 bytes of DXIL)
postfx: Vignette Pass's pass glow-down runs as pass 2 before the HUD (7300 bytes of DXIL)
postfx: Vignette Pass's pass glow-up runs as pass 3 before the HUD (6464 bytes of DXIL)
```

The **Addons** page tags the addon **Changes the picture**.

## Passes

`postfx` is a list of passes, run in order: every addon's passes sorted by `order`, equal values in load order. It needs `"postfx"` in `experimental_features`.

```jsonc
"postfx": [
  {
    "shader": "fx/glow.hlsl",         // the .hlsl file, a path inside the addon
    "name": "glow-up",                // the name in the build output and loader.log; defaults to the file name without .hlsl
    "entry": "up",                    // the function to run; defaults to effect
    "options": ["{{glow}}", 100],     // up to 16 values, read with option(0) to option(15)
    "order": 0,                       // lower runs first; default 0
    "budget_ms": 1.5                  // GPU milliseconds a frame, above 0 and at most 16; default 1.5
  }
]
```

One file holds any number of passes. Each pass names its function in `entry`, and the header calls it for every pixel of the pass's output:

```hlsl
float4 down(float2 uv, uint2 pixel) { /* ... */ }
float4 up(float2 uv, uint2 pixel) { /* ... */ }
```

An `options` value is a number, or <span v-pre>`"{{id}}"`</span> naming one of the addon's options: a slider, a choice with number values, or a toggle (1 or 0).

## Textures and several passes

A pass reads the picture with `scene(uv)` and writes the picture. `postfx_textures` adds textures of the addon's own, which passes write with `output` and read with `inputs`. Bloom is a downsample into a small texture, a blur there, and an upsample back onto the picture:

```json [swofty.json]
"postfx_textures": [
  { "name": "half",    "scale": 0.5 },
  { "name": "quarter", "scale": 0.25 }
],
"postfx": [
  { "shader": "fx/bloom.hlsl", "entry": "down",    "name": "bloom-half",    "inputs": ["original"], "output": "half" },
  { "shader": "fx/bloom.hlsl", "entry": "down",    "name": "bloom-quarter", "inputs": ["half"],     "output": "quarter" },
  { "shader": "fx/bloom.hlsl", "entry": "upwards", "name": "bloom-up",      "inputs": ["half", "quarter"],
    "options": ["{{bloom}}"] }
]
```

```hlsl [fx/bloom.hlsl]
#include "swofty/postfx.hlsli"

float4 down(float2 uv, uint2 pixel)
{
    float2 t = 1.0 / float2(input_size(0));
    float3 sum = input(0, uv + float2(-1, -1) * t).rgb + input(0, uv + float2(1, -1) * t).rgb
               + input(0, uv + float2(-1, 1) * t).rgb + input(0, uv + float2(1, 1) * t).rgb;
    return float4(sum * 0.25, 1);
}

float4 upwards(float2 uv, uint2 pixel)
{
    float4 c = scene_at(int2(pixel));
    float3 rgb = to_linear(c.rgb);
    rgb += (input(0, uv).rgb + input(1, uv).rgb) * (option(0) / 100);
    return float4(from_linear(rgb), c.a);
}
```

The manifest, per texture:

```jsonc
"postfx_textures": [
  { "name": "half",  "scale": 0.5, "format": "rgba16f" },                                    // the picture's size times scale: above 0, at most 2
  { "name": "accum", "width": 512, "height": 288, "format": "rgba16f", "persistent": true },  // a fixed size from 1 to 8192; persistent keeps it from frame to frame
  { "name": "dirt",  "file": "tex/dirt.png" }                                                // a read-only image from the addon
]
```

- A name is 1 to 32 of `a-z`, `0-9` and `_`, and unique in the addon. `scene`, `original`, `previous` and `depth` are taken.
- A texture takes one of `scale`, `width` and `height` together, or `file`.
- `format` is `rgba8`, `rgba16f` (the default), `rg16f`, `r16f`, `r32f`, `rgb10a2` or `r11g11b10f`. An image is always RGBA8.
- Textures belong to the addon: two addons can both name one `half`.

A pass lists up to eight `inputs`, read in the shader by their place in the list:

```hlsl
float4 input(uint i, float2 uv);        // filtered, uv from 0 to 1
float4 input_at(uint i, int2 pixel);    // one texel, clamped to the edges
uint2  input_size(uint i);
```

An input is an addon texture, or one of two the core keeps:

| Input | Holds |
|---|---|
| `original` | The picture before any pass this frame |
| `previous` | The last frame's picture after every pass |

`previous` is empty on the first frame and after a camera cut, a resize or a frame without passes. `has_previous()` says whether it holds a picture.

`output` is an addon texture that is not an image, or omitted for the picture. A pass that writes a texture is sized by it: `swofty_size` is the texture's size, and `scene(uv)` is still the picture.

## Raw compute passes

A pass with `"raw": true` defines its own compute entry point and writes `swofty_output` itself. It needs `threads`, the thread group size from 1 to 1024 in total. `entry` names the function and defaults to `main`.

```json [swofty.json]
{ "shader": "fx/histogram.hlsl", "raw": true, "entry": "fill", "threads": [16, 16], "output": "accum" }
```

```hlsl [fx/histogram.hlsl]
#include "swofty/postfx.hlsli"

[numthreads(16, 16, 1)] void fill(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= swofty_size))
        return;
    float3 rgb = to_linear(scene_at(int2(id.xy)).rgb);
    swofty_output[id.xy] = float4(rgb, luminance(rgb));
}
```

The core runs the group count that covers the output. A pass that is not raw runs 8×8 threads.

## When a pass runs

`when` limits a pass by the game's view. The core tracks the camera and the state of the session; a pass with `views` runs only while the core knows the view and it is one of them.

```json [swofty.json]
{ "shader": "fx/fisheye.hlsl", "options": ["{{strength}}"],
  "when": { "views": ["cockpit", "tcam", "tcam_offset"],
            "skip": ["paused", "replay", "photo", "menu", "cutscene"] } }
```

```text
views  cockpit  tcam  tcam_offset  chase_near  chase_far  other
skip   paused  replay  photo  menu  cutscene  spectating
```

- `menu` is every state without a player-driven session view: the frontend, the garage and the hub.
- A pass without `when` runs in every view.
- A shader reads the same state with `view_kind()` and `view_is(SWOFTY_VIEW_PAUSED)`, to change what it draws rather than whether it runs.

## Live options and enabled

A `{{id}}` option that binds `native` is read from the addon's settings four times a second, so moving the slider in **ADDON SETTINGS** in game changes the picture straight away. A `db` option is fixed when the profile builds.

```json [swofty.json]
"options": [
  { "id": "strength", "type": "slider", "label": "Strength", "min": 0, "max": 100, "step": 1, "default": 35, "binds": "native" },
  { "id": "fisheye_on", "type": "toggle", "label": "Fisheye", "default": true, "binds": "native" }
],
"postfx": [
  { "shader": "fx/fisheye.hlsl", "options": ["{{strength}}"], "enabled": "{{fisheye_on}}" }
]
```

`enabled` is <span v-pre>`"{{id}}"`</span> naming a toggle option, and turns the pass on and off:

- A `native` toggle is live: the pass is built, and the core skips it while the toggle is off.
- A `db` toggle is read at build: when it is off the pass is left out, and the build prints `postfx: Vignette Pass's pass fisheye is switched off`.

## A key for the passes

`postfx_toggle` names a native `choice` option whose values are key names. While F1 25 is in front, the core toggles that addon's passes when the key goes down and shows a HUD toast.

```json [swofty.json]
"options": [
  { "id": "toggle_key", "type": "choice", "label": "Toggle key", "default": "F9", "binds": "native",
    "values": ["F9", "F10", "Home", "None"] }
],
"postfx_toggle": "{{toggle_key}}"
```

The values are `A` to `Z`, `0` to `9`, `F1` to `F24`, `Insert`, `Home`, `End`, `Page Up`, `Page Down` and `None`. `None` has no key.

## Installing checks

Installing checks every pass and texture and refuses the manifest with the field that is wrong:

```text
invalid manifest: example.vignette-pass: postfx[0]: list "postfx" in experimental_features to use it
invalid manifest: example.vignette-pass: postfx[1]: input quarter names no postfx_textures entry
invalid manifest: example.vignette-pass: postfx[2]: a raw pass needs threads
invalid manifest: example.vignette-pass: postfx[2]: output dirt is an image, which is read-only
invalid manifest: example.vignette-pass: postfx_textures[0]: scale 3 is not above 0 and at most 2
invalid manifest: example.vignette-pass: postfx_toggle: option toggle_key must be a native choice
```

At build, a `{{id}}` option whose value is not a number stops the build: <span v-pre>`Vignette Pass: postfx option {{vignette}} is "abc", not a number`</span>. A `db` option changes from the next launch.

## The shader

A pass includes `swofty/postfx.hlsli`. The header is in the SDK at `packages/sdk/include/swofty/postfx.hlsli`, and the build puts it on dxc's include path. Files next to the shader can be included with `#include "name.hlsli"`.

```hlsl
// the picture, after the passes before this one
float4 scene(float2 uv);           // at uv (0 to 1), filtered
float4 scene_at(int2 pixel);       // at a pixel, clamped to the edges
float  scene_depth(float2 uv);     // the game's depth buffer as stored: 1 at the camera, falling towards 0 with distance
bool   has_depth();                // whether the depth buffer was readable this frame

// inputs
float4 input(uint i, float2 uv);
float4 input_at(uint i, int2 pixel);
uint2  input_size(uint i);
bool   has_previous();

// values
float  option(uint i);             // the pass's options[i], 0 when not set
uint2  swofty_size;                // the output's size in pixels
float2 swofty_texel;               // 1 over it
uint2  swofty_scene_size;          // the picture's size
uint2  swofty_depth_size;          // the depth buffer's size: the render resolution, below the picture's with upscaling on
float  swofty_time;                // seconds since the passes started
float  swofty_frametime;           // seconds the last frame took
uint   swofty_frame;               // frames since the game started
uint   swofty_encoding;            // SWOFTY_POSTFX_SDR, SWOFTY_POSTFX_HDR10 or SWOFTY_POSTFX_SCRGB
uint   swofty_pass;                // the pass's place in this frame's chain, from 0
float  random(uint2 pixel, uint salt); // a hash of pixel, frame and salt, from 0 to 1, for grain and dithering

// the view
uint   view_kind();                // SWOFTY_VIEW_COCKPIT, _TCAM, _TCAM_OFFSET, _CHASE_NEAR, _CHASE_FAR, _OTHER or _UNKNOWN
bool   view_is(uint flag);         // SWOFTY_VIEW_PAUSED, _REPLAY, _PHOTO, _MENU, _CUTSCENE or _SPECTATING

// colour
float3 to_linear(float3 c);        // decode to linear light: sRGB in SDR, PQ in HDR10, unchanged in scRGB
float3 from_linear(float3 c);      // encode back. In linear light 1 is SDR white, and 80 nits in HDR
float  luminance(float3 linear_rgb); // Rec. 709 luminance of a linear colour
float3 srgb_to_linear(float3 c), linear_to_srgb(float3 c), pq_to_linear(float3 c), linear_to_pq(float3 c);
```

- The picture is the game's encoded picture: sRGB in SDR (R8G8B8A8), PQ in HDR (R10G10B10A2).
- A pass reads the result of the pass before it.
- A pass that writes the picture writes the picture's own format, so values outside 0 to 1 clip in SDR.
- A pass that works in `to_linear` and `from_linear` looks the same in SDR and HDR.
- The header runs the pass's function from an 8×8 `main` compute shader, once per pixel of `swofty_size`.

## Build

Launcher releases include dxc in a `dxc/` folder beside `bin/`, built from the open-source DirectX Shader Compiler with its DXIL validator. The build finds dxc in this order:

```text
SWOFTY_DXC                 another dxc
<launcher>/dxc/bin/dxc     the release's own
dxc on PATH
```

A pass must compile to signed DXIL. dxc without its validator (`dxil.dll`, `libdxil.so`) beside it writes unsigned DXIL, which the build refuses.

To check a pass while you write it, compile the addon's passes the way the build does. The command prints dxc's errors and warnings in full and exits non-zero when a pass fails:

```sh
swofty postfx compile path/to/my-addon [--out dxil/]
```

```text
dxc: /home/me/SwoftyLauncher/dxc/bin/dxc
ok    vignette (fx/vignette.hlsl): 5984 bytes of signed DXIL
ok    glow-down (fx/glow.hlsl, down): 7300 bytes of signed DXIL
```

The build compiles each pass with `dxc -T cs_6_0 -HV 2021 -O3` and the pass's entry (`-D SWOFTY_EFFECT=up -E main`, or `-D SWOFTY_RAW=1 -E fill` for a raw pass) and keeps the DXIL in the addon's cache, so an unchanged pass is not compiled again. A pass that does not compile is left out, and the build warns with dxc's first error:

```text
Vignette Pass: pass vignette is skipped: dxc: fx/vignette.hlsl:6:12: error: use of undeclared identifier 'nope'
```

The compiled passes are installed as `swofty/postfx/00.dxil` onwards, and `swofty/assets.json` lists them under `postfx` in the order they run, with `postfx_textures` and `postfx_toggles` beside it. A `.png` texture is installed as `swofty/postfx/tex/00.tex` onwards.

## In game

The passes run between the game's picture and its HUD. The HUD and the menus draw over the processed picture with their own colours and transparency. A pass that moves pixels, such as a lens, bends the picture under a translucent pause menu as well as on track.

`swofty/loader.log` shows the passes getting ready and where they first run:

```text
[d3d12] command list state hooks armed: Reset, ClearState, SetPipelineState(1), SetDescriptorHeaps and the compute root bindings
[postfx] 3 pass(es) and 1 texture(s) wait for the game's picture
[postfx] Vignette Pass's pass vignette is ready (postfx/00.dxil, 6336 bytes of DXIL)
[postfx] passes run on the game's 1920x1080 picture (format 28) with 1 texture(s) of their own
[postfx] the picture now goes through 3 pass(es) before the HUD, inside the game's list at its HUD-less step
```

Most frames run inside the game's own command list, at the step where it keeps a copy of the picture without the HUD. The core records the passes there and then puts back the pipeline and bindings it changed. On frames where the game ends the picture's list before the HUD, the passes run in a list of the core's own between the game's lists. Every 60 s for the first 30 minutes, the log counts both:

```text
[postfx] last 60 s: 7210 frame(s) presented; passes ran in the game's list on 6950 and between its lists on 255; 5 without passes (0 with the picture and the HUD in one list, 0 with list state from before the hooks, 0 with the GPU behind); 0 frame(s) ran them twice
```

Frames without passes are loading screens and menus without a 3D picture. With Frame Generation on in the game's settings, the passes' result also goes into the game's HUD-less copy, which Frame Generation reads. Frame Generation still uses the game's own motion vectors, so a pass that moves pixels can smear in the generated frames.

A key from `postfx_toggle` logs and shows a toast:

```text
[postfx] PolyphoniX Native's passes are off (its key)
```

A pass is turned off when its pipeline cannot be made, or when its average GPU time over 120 frames stays above `budget_ms` three times in a row. The core measures each pass's GPU time.

```text
[postfx] Vignette Pass's pass vignette is off: its pipeline could not be made (0x80070057)
[postfx] Vignette Pass's pass vignette is off: it took 2.410 ms of GPU time a frame, over its 1.50 ms budget
```

Switches:

```sh
# in the game folder: every pass off within a second; remove the file to turn them on again
touch swofty/postfx.off

# in the game's environment: every pass off for the session
SWOFTY_POSTFX=0

# write the picture before and after the passes six times in the first minute,
# and once more each time a swofty/postfx.capture file appears in the game folder
SWOFTY_POSTFX_CAPTURE=C:\captures

# log one line for each of the next N presented frames: where the passes ran, how many,
# the view, and why a frame had none; a swofty/postfx.trace file holding N does the same in game
SWOFTY_POSTFX_TRACE=600
```

```text
[postfx] trace present=41 path=in-list runs=1 passes=3/3 view=cockpit flags=- valid=1 serial=7 fg=off back=000000006E2A1C10
[postfx] trace present=42 path=none runs=0 passes=0/1 view=chase_far flags=- valid=1 serial=8 fg=off back=0000000000000000 why=no pass runs in this view skipped=fisheye (not in one of its views)
```

`SWOFTY_FRAME_CAPTURE` writes the presented frame with the HUD ([logging switches](/reference/std/logging)).

```text
[postfx] SWOFTY_POSTFX=0; every pass is off
```
