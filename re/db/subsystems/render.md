# render: D3D12/DXGI present path and the swapchain overlay

Owner: see `subsystems.tsv`. Consumer: `native/src/core/hud.cpp` (the overlay standard library, experimental `overlay` feature gate).

## Map
- The game renders with Direct3D 12 (`d3d12core.dll` + `dxgi.dll`); there is no exe-side present call site catalogued yet.
- The overlay reaches the present path through the stable COM vtable ABI, not through any `F1_25.exe` address, so it needs no KB VA and survives game updates. At arm time it creates a throwaway `D3D12CreateDevice` + `CreateDXGIFactory1` + `CreateSwapChainForHwnd` probe, reads three vtable slots, installs safetyhook inline hooks on the shared code they point at, then destroys the probe:
  - `IDXGISwapChain::Present` — vtable slot 8 (vt+0x40). Draws the frame, then calls the original.
  - `IDXGISwapChain::ResizeBuffers` — vtable slot 13 (vt+0x68). Releases the overlay's back-buffer references before the game resizes.
  - `ID3D12CommandQueue::ExecuteCommandLists` — vtable slot 10 (vt+0x50). Captures the game's `DIRECT` command queue (the one Present needs to submit the overlay's command list).

## Open
- The exe-side present call site (the render-loop function that calls `swapchain->Present`): decompile and catalogue as `Render_*` once observed in game, for a lower-overhead exe hook than the vtable probe.
- Text and image rendering: the first landing paints solid rects / outlines / lines only; a font atlas and textured-quad path remain.

## Render pipeline toward an offline preview (engine-re 2026-11-17, static)

Written by engine-re at user request (lane owner review requested). Static only (Steam d873a1c8 exe); nothing here is runtime-confirmed. Blind verification: V10803-V10816 (10 agree, 3 partial, 0 disagree).

## Lighting

- **Two sources for the sun.**
  - **Time-of-day curves.** Two consumers bind the `sky/*` curves from the TOD data under `Game_TimeOfDay_LockAll`, each through a 0x30-byte curve binding (`Game_TodData_FindParam`):
    - `Render_SkyState_BindTodCurves` 0x142e106f0 binds 54 keys.
    - `Render_SkyRenderer_BindTodCurves` 0x144779fd0 binds 47 keys.
    - Both rebind when the TimeOfDaySystem `curve_version` (+0xb438) moves (`Render_SkyRenderer_Update` 0x14477d170).
    - The sun direction is the pair `sky/sunHeightAngle` (default 45) and `sky/sunRotationAngle` (default 90).
    - The sun light is `sky/sunLightIntensity` (default 3), `sky/sunLightColour` and `sky/maxSunColour`.
    - The ambient light is `sky/ambientLightColour`, `sky/maxAmbientColour` and `sky/maxAmbientOvercastColour`.
    - Night: `sky/nightShadowIntensity` and `sky/nightSpecularIntensity`.
    - Fog and reflection colours come from `weather/atmospherics/*`.
  - **Real solar position.** `Game_TimeOfDay_ComputeSolarAngles` 0x1446175d0 computes elevation, azimuth, sunrise and sunset from date, clock, latitude and longitude.
    - `Sess_GameWeatherSystem_SetupSession` reads latitude from the track weather config +0x30, longitude from +0x34 and the year from +0x3c. Month and day come from the session settings.
    - `Render_SkyState_UpdateSolarSun` 0x142e15970 (owner `*(gws+0x1db0)`, sun block gws+0x1e40) evaluates three key times and writes the results into the TimeOfDaySystem from +0x267c on.
    - The longitude correction has no rounding (`(lon - (lon/15)*15) * 4`), so longitude has no effect. Only latitude, date and clock time move the sun.
    - Which DB keys feed config +0x30 / +0x34 is OPEN.
  - **HYPOTHESIS:** the curves are authored over a normalised day and stretched to the real sunrise/sunset. The runtime test in T158 will tell.
- **Per-world override: SunOverrideComponent** (game lane, `Game_SunOverrideComponent_Initialise` 0x14099e9c0).
  - It can override sun azimuth (+0xf34), height angle (+0xf38, clamped 0..pi), colour, intensity and diameter, plus explicit luminance in cd/m2, colour grading, the weather preset and "Apply SSAO to lightmap".
  - It carries the bake set as asset-group paths:
    - AO .bake (+0x160)
    - Radiance .bake (+0x280)
    - Light list .bake (+0x3a0)
    - DDGI radiance (+0x4c0)
    - DDGI config (+0xa60)
    - IBL SH prefix (+0x5e0)
    - IBL probe prefix (+0x820)
    - LUT texture (+0xb80)
  - The track DB (`Sess_Load_TrackDatabase`) names the same groups: aoBakeAssetGroup, radianceBakeAssetGroup, lightListAssetGroup, ddgiAssetGroup, iblProbes.
- **Light entity components.** Load slots near `Render_ModelComponent_Load`:
  - sun: `Render_SunLightComponent_Load` 0x144485de0 (shadow cascades, diameter, ibl* scales)
  - spot: `Render_SpotLightComponent_Load` 0x14448cf20 (cone, IES, shadows)
  - omni: `Render_OmniLightComponent_Load` 0x14447fa50
  - IBL box: `Render_IblLightComponent_Load` 0x14447ba00 and its field table `Render_IblLightComponent_RegisterProperties` 0x14447b540, with `diffuseWithLightmap` +0xdb and `ignoreIfRadianceMap` +0xda
  - volumetrics/ambient: `Render_VolumetricsComponent_Load` 0x144464e60 (global_ambient_light_map, world-space lightmap intensity)
  - Every light has `lightmap_direct`, meaning it is already in the bake. That is HYPOTHESIS: the consumer was not read.
- **Other lighting pieces.**
  - DDGI: `Render_DDGIVolumeConfig_Parse` 0x143be6f00 (has `ignoreLightmaps`).
  - Ambient/sun LUT compute: `Render_AmbientLut_Generate` 0x143ee9a00.
  - Exposure and tonemap rows already exist: `Render_Tonemap_*`, `Render_PostFx_InitTonemapper`.

### ToD key table (Render_SkyState_BindTodCurves, owner offsets, defaults)

| key | off | default |
|---|---|---|
| sky/sunHeightAngle | +0x780 | 45 |
| sky/sunRotationAngle | +0x750 | 90 |
| sky/sunLightIntensity | +0x6c0 | 3 |
| sky/sunLightColour (vec) | +0x670 | const 0x1457e7ea0 |
| sky/sunAngularDiameter | +0x6f0 | 0.533 |
| sky/maxSunColour (vec) | +0x50 | - |
| sky/ambientLightColour (vec) | +0x140 | const 0x145614f80 |
| sky/maxAmbientColour (vec) | +0x840 | 0 |
| sky/maxAmbientOvercastColour (vec) | +0x890 | 0 |
| sky/nightShadowIntensity | +0x240 | 1 |
| sky/nightSpecularIntensity | +0x720 | 1 |
| sky/sunSize / sunLum / sunDiscColour | +0x300 / +0x2d0 / +0xf0 | 3 / 1 / - |
| sky/glowSize / glowIntensity / maxSunGlowColour | +0x2a0 / +0x270 / +0xa0 | 8 / 0.6 / 0.9 |
| sky/moonRotationAngle / moonHeighAngle | +0x7b0 / +0x7e0 | 0 / 0 |
| weather/atmospherics/reflection_fade_colour, sky_fog_clear_colour, sky_fog_overcast_colour | +0x8e0 / +0x930 / +0x980 | - |
| weather/atmospherics/volumetric_fog_scale, rain_volumetric_fog_scale | +0xb80 / +0xbb0 | 0 |
| sky/planarReflectionIntensity | +0xbe0 | 0.15 |

The cloud keys sit at +0x190..+0x640 and +0x9d0..+0xb20 (see the function's decomp).

## Lightmaps

- **Every model component holds the track's lightmap location pair** (`Render_ModelComponent_Load` 0x14446a6d0):
  - +0xb8 = `eaid://ego_package/lightmap_locations`
  - +0xc0 = `eaid://ego_package/lightmap_locations_radiance`
- **LightMapLocRes** (`Render_LightMapLocResFactory_Create` 0x143c06290, object vtable 0x1458c2550, descriptor 0x146441c38):
  - The reader takes `(size-4)/32` records of 32 bytes from byte 4 on. The leading u32 count is not used.
  - Each record is a 16-byte instance uniqueid key and a 16-byte value, held in a hash map. A duplicate key keeps its first entry.
  - Value layout (file-format side, F1_TrackLab decoder): u32 atlas slice, f32 size, f32 u, f32 v.
  - The atlases are `baked_maps_ao` (BC4) and `baked_maps_radiance` (BC6H), 4 slices. Slice 0 is the planar terrain map. `Render_Texture_CreateFromResource` special-cases both.
- **Fallbacks.** `Game_Read_RendererGfxConfig` names `eaid://default_lightmap`, `default_lightmap_array_ao` and `default_lightmap_array_radiance`.
- **OPEN:**
  - the shader-side lookup: which constant carries slice/size/u/v, and which UV set (terrain: the UVMacro / 2nd UV)
  - what an instance with no entry gets. Likely the default arrays; needs a capture (T159).

## Materials

- **Parse.** `Asset_MaterialFactory_Parse` 0x143c0dc80 (assets lane). File layout: `gfx_material.md`.
- **Bind layout.** `Render_Material_CountResourceSlots` 0x143c07890 (name kept; it builds the bind layout, it does not count):
  - Layout = constants (slot 0, mat+0x50), one static slot (slot 1) and one slot per texture record (kind 2).
  - Kind 1 (constants) and kind 3 (bindless heap index) get no slot.
  - The slot index is written back to record+0x20.
- **Resource types.** `Render_StaticInit_ResourceTypeDescriptors` 0x143f13130 maps every render resource type name (GfxModelRes, NeMaterialRes, IBLSHRes, DDGIRes, ...) to its descriptor.
- **OPEN:**
  - shader (`.nefx2`) permutation selection from the material's shader path
  - `Render_Material_ResolvePassSlot` 0x1401406e0 (pass mask from material flags) is the lead

## LOD

- **Instance LOD and fade use screen size, not distance.** `Render_Draw_ComputeInstanceLodScale` 0x143d41770 returns the projected size of the bounding sphere:
  - perspective: angle^2 * view+0x68
  - ortho: 2 * the projected radius
- **Per-model LOD switch values** are read and checked for ascending order by `Render_ModelResource_Parse` 0x143c16f40 (GfxModelRes v2).
- **Per-pass bias:** `Render_GeometryPass_Parse` 0x143b5c740 (lodbias, lodControlLod/Threshold/Threshold2).
- **Per-entity bias:** the "lod bias" and "min area bias" keys of `Game_Parse_ModelRenderFlags` 0x1444725e0.
- **OPEN:** the compare of the size against the model's LOD values, and the graphics-quality scale.

## Wetness

- The road wetness map is the top-down 75 x 75 m wetness render around the player (`Game_WetMap_*`, flags `render to wetness map` in the Render component).
- `Render_Bind_WetnessSamplers` 0x1447192f0 belongs to the skidmark/track-mark layer. It binds that layer's own albedo, normal, normalIntensity, surface, depth and wetness textures, so it is not the road material's wetness input.
- Track-level drive values: `weather/track/wetness`, `puddles` and `absorbtion` (see session.md#wetness), the ConstantsTODBuffer / `g_trackConstants` buffer (`Render_Gfx_CreateTrackConstantsAndPuddleBuffers`) and the puddle index buffer.
- **OPEN:** the material-side wet response (shader constants), T-request material.

## Next rounds (toward offline preview = game), by value

1. **Shader-side lightmap lookup:** where slice/size/u/v of an instance reach the draw constants, and which UV set is used. Without it, AO and radiance cannot be placed on objects (terrain slice 0 is already known offline).
2. **Sun vector and colour at draw time:** the consumer of `sky/sunHeightAngle` / `sunRotationAngle` (owner +0x780 / +0x750) to the light constant buffer, and the curve stretch to sunrise/sunset. Then a preview can use the TOD files directly.
3. **Material to shader permutation:** `.nefx2` resolution, pass mask (`Render_Material_ResolvePassSlot`), and the bindless `me_*HeapIndex` fill.
4. **Exposure and tonemap parameters** per time of day (`Render_Tonemap_*`, eye adaptation), and the colour-grading LUT per track/scenario.
5. **LOD selection compare**, quality scale and draw-distance settings.
6. **Wet road shading:** the material constants that read `g_trackConstants` and the wetness map.
7. **IBL / DDGI probe sampling** for ambient on dynamic objects (`IBLSHRes`, DDGI volumes).

## Inbox
- 2026-10-19 14:03 ui-re: new 'hud' lane holds the HUD view-model/logic side (what widgets publish + their setToOpen gates); render still owns the D3D present/draw path. Hud_Open_TyrePerformance (0x14152b5f0) publishes to view_model_data.hud_* registry nodes. HUD VM vtables cluster near 0x1456ed290 if you need draw-time addresses.
- 2026-10-28 22:57 re-import: (wx rounds 1-3, static, verified by wx-verify): render flags +0x32 live on the per-entity Render component (type 0x146cb7b70, vtable 0x145a90b10, parser 0x1444725e0); the Model component (type 0x146cb7da0, vtable 0x145a908a0) holds +0xb0 Render, +0x80 render instance (current world matrix inst+0x10..+0x3f, previous +0x40..+0x6f, generator mask +0x70), +0xd8 model path. Per-model setter Render_ModelComponent_ApplyRenderProps 0x14446ea70 -> 0x14446b9c0 (KB fix: not LitPlanarReflectionProxy). rainMask (0x20) = screen-space depth from the main camera that clips falling rain only; wetness map (0x40) = top-down ortho 75 x 75 m around the player, 256x256 (0x14478c050 / 0x14478e440); dry/wet meaning lives in material shaders. Quality handler 0x14015aa00 field map in game notes. | findings from PR #471 by vogtmarcel07-cloud
- 2026-10-28 09:45 engine-re: 2026-10-28 engine-re (wx rounds 1-3, static, verified by wx-verify): render flags +0x32 live on the per-entity Render component (type 0x146cb7b70, vtable 0x145a90b10, parser 0x1444725e0); the Model component (type 0x146cb7da0, vtable 0x145a908a0) holds +0xb0 Render, +0x80 render instance (current world matrix inst+0x10..+0x3f, previous +0x40..+0x6f, generator mask +0x70), +0xd8 model path. Per-model setter Render_ModelComponent_ApplyRenderProps 0x14446ea70 -> 0x14446b9c0 (KB fix: not LitPlanarReflectionProxy). rainMask (0x20) = screen-space depth from the main camera that clips falling rain only; wetness map (0x40) = top-down ortho 75 x 75 m around the player, 256x256 (0x14478c050 / 0x14478e440); dry/wet meaning lives in material shaders. Quality handler 0x14015aa00 field map in game notes.
- engine-re finding for Render_ModelComponent_Load 0x14446a6d0: Model component (type 0x146cb7da0, vtable 0x145a908a0, 0x118 bytes) slot 3: model+0xd8 = char* 'model' path (14446a7ec-800 via 0x143fa2de0), +0xe0 'mat binding'; +0xa8 entity; +0xb0 = Ne_Entity_FindComponent(entity,0x146cb7b70) Render component (14446a7d9/7e5); resource key '<model>?context=render?render' ids +0x68/+0x70, resource +0x78, lightmaps +0xb8/+0xc0 (wx-verify claim 5) | L2 2026-10-28: user OK for engine-re to raise render rows (render lane owner overlay); xref = caller or vtable slot pointer
- engine-re finding for Render_Apply_InstanceRenderProperties 0x14446b9c0: CORRECTION 2026-10-28 (wx): NOT LitPlanarReflectionProxy (that is type 0x146cb60a0, vtable 0x145a90870, factory 0x145a90880); this is generic Model component code. data=Render+0x18; byte [data+0x1a]=Render+0x32 tested 0x20 -> mask 0x146cb66f0 (rainMask), 0x40 -> 0x146cb6570 (wetnessMapGenerator), 0x80 -> 0x146cb65b0 (mirageGenerator) (14446d313/d393/d413), vpor/vpandn into render instance +0x70 (128-bit generator mask); +0x33 0x40 rainBeads 0x146cb6710; +0x34 0x10 -> service vtbl[+0x20](handle,12,0) (broadcast-only marker). Then cmpxchg inst+0x128 0->2, queue {0x143d3ce20,inst}, inst+0x12c|=0x20. Verified by wx-verify claims 1,2 | L2 2026-10-28: user OK for engine-re to raise render rows (render lane owner overlay); xref = caller or vtable slot pointer
- engine-re finding for Render_ModelComponent_ApplyRenderProps 0x14446ea70: Per-model render-property setter (wx 2026-10-28, static): rdx=[model+0xb0] (Render component found at load), add rdx,0x18, tail jmp 0x14446b9c0 (14446ea87/93/a0, only direct ref to 0x14446b9c0); called from AddToScene 0x14446e7b0 when model+0x10e set; without a Render component applies default masks staticGenerator+lowLODShadows. Rain-blocker recipe: rp=Ne_Entity_FindComponent(entity,0x146cb7b70); rp+0x32|=0x20 (rainMask) or &=~0xC0 (no wetness/mirage); call this(model) if model+0x80. Notes work/weather/findings_gfx.md | L2 2026-10-28: user OK for engine-re to raise render rows (render lane owner overlay); xref = caller or vtable slot pointer
- engine-re finding for Render_ModelComponent_OnTransform 0x144470870: Model secondary iface vtbl[1]: inst=[rcx+0x68] (=model+0x80); writes CURRENT world matrix as 3 transposed rows at inst+0x10/+0x20/+0x30 (14447092d-937); copies old to inst+0x40..+0x6f only if [inst+0x13c]&8; skips if counter +0x13a>=3 and unchanged; sets inst+0x12c|=1 (wx-verify claim 3) | L2 2026-10-28: user OK for engine-re to raise render rows (render lane owner overlay); xref = caller or vtable slot pointer
- 2026-10-29 11:48 engine-re: engine-re findings 2026-10-29 for the render lane owner (static, blind-verified v4-verify S2): camera 0x150 bytes: +0x70 projection 4x4 row-major (v' = v*M), +0xb0 view, +0xf0 inverse view, eye +0x120, near +0x2c, far +0x30; 0x143be2070(cam, view) writes +0xb0 and inverse +0xf0; Render_View_SetFromCamera 0x143be0360(V, cam) copies to V+0x10 (proj V+0x80, view V+0xc0), different projection branch when [0x14618caa0]+0x161; called per viewport at 0x1401583d0 in 0x140157d90 (unique site pattern E8 ?? ?? ?? ?? 42 83 BC BE F4 0A 00 00 00). Used by vmknolle race-start for a world-to-screen overlay.
- engine-re finding for Render_OnWeatherQualityChanged 0x14015aa00: Log channel WEATHER: 'GameRendererModule::onWeatherQualityChanged' | 'GameRendererModule::onWeatherQualityChanged: rainBeadsEnable %d' | 'GameRendererModule::onWeatherQualityChanged: rainSplashesEnable %d' | 'GameRendererModule::onWeatherQualityChanged: rainHighEnable %d' | 'GameRendererModule::onWeatherQualityChanged: heatHazeE
- engine-re finding for Render_Evaluate_CarDamageLevels 0x142f17170: damage level per part group for the renderer (cases 0..10: wheels, front wing, sidepods/mirrors, floor/bargeboards, rear wing = mean of rearwing_bl/br/top, diffuser).
- engine-re finding for Render_Particles_CreateCarEffect 0x143af5620: named from its log messages (channel PARTICLES): 'USED!! | Car start %u | Car stop %u | Couldn't create effect '%s'!'
- engine-re finding for Render_Particles_ApplySpawns 0x143b05a50: named from its log messages (channel PARTICLES): 'perform %d spawns and %d destroys and %d tints | trying to create an effect %s in a slot %04x where we have one already, ignoring (maybe we skipped a delete in replays?)'
- engine-re finding for Render_GroundCover_Construct 0x143ce0630: named from its log messages (channel GROUND_COVER): 'Constructed NeGroundCoverSystem | Enhanced ground cover is enabled (ultra quality is needed).'
- engine-re finding for Render_GroundCover_LoadCascadeModel 0x144467200: named from its log messages (channel GROUND_COVER): 'Ground cover cascade %d loading model: %s' lb22-verify partial
- engine-re finding for Render_Particles_OnVehicleEvent 0x14463da30: named from its log messages (channel PARTICLES): 'tyre burst | LOOK OUT - veh index not same as array index, should be OK, but may need testing %d %d | - | unknown surface '%s' | I expected a null handle but got %p | KILL EFFECTS at start of fast rew' lb22-verify partial
- engine-re finding for Render_Particles_SelectScrapeEffect 0x144704740: named from its log messages (channel PARTICLES_C): 'mat '%s' effect name '%s' scrape '%s' priority %d'
- engine-re finding for Render_Skidmarks_Init 0x144711290: trackinfo parser for skid marks and the wetness section: SKIDMARKS, SKIDMARKS_BLEED, TYRE_RESIDUE, SKIDMARKS_CONFIG, WETNESS (wet_range, duration_no_rain, duration_full_rain, puddle_range), CAR_MARKS_ENABLE. The car marks are what dries the racing line in the wetness map.
- engine-re finding for Render_Bind_WetnessSamplers 0x1447192f0: binds six named texture/sampler pairs of a pass, among them wetnessSamplerTex / wetnessSampler (the wetness map the surface shaders read); keeps a width/height ratio of the first texture in 0x145faa648.
- engine-re finding for Render_Skidmarks_Update 0x14471f650: Log channel SKIDMARKS: 'Something has gone wrong during the mark update code.'
- engine-re finding for Render_PuddleBuffer_Readback 0x14479f9c0: reads the GPU puddle index list back (count <= 0x10000, 'invalid puddle data count') and marks a 64 KB byte map per slot ([[+0x40]+0x40]+0x60 -> +8 + slot*0x10000, 256 x 256 cells) where puddles are.
- 2026-11-02 00:26 placed-record: placed-record: 0x1439b9390/0x1439b98c0/0x1439f4150 and site_Render_Gfx2Heap_RecordIndex raised at L1 from re/decomp (heap object layout in the Create notes; frame start 0x1439f4150 releases records 0..count-1 at 0x1439f5172). Owner: please review to L2 and add an independent agree. 0x143a06ea0 (GetImmediateCommandList) indexes lists by the +0x1490/+0x1494 counters.
- 2026-11-05 14:01 sponsor-studio: sponsor-studio: added at L1 with static L2 evidence (decomp read, callers listed, data flow in re/research/sponsors.md); please raise after verification: Render_CustomisationDecalsSet_OnProxyMessage 0x143c772f0.
- 2026-11-14 02:27 engine-re: engine-re 2026-11-14 weather to L2 (static, for the render owner): Game_TrackInformationSystem_Update 0x144795cf0 writes the PREVIOUS frame track wetness ws+0x3f0a38 (ws = *g_Game_WeatherSurfaceState) to [[ws+0xd08]+0x48]+0xa5b7e4 (0x144795f48) before it re-evaluates weather/track/wet_delta, and a temperature-derived value to +0xa5b7ec (0x144796052); rain intensity reaches the rain renderer as [TimeOfDayOwner+0x2c8]+0x75ca4 (0x14479c47e). Data flow and struct layout: kb:subsystems/session.md#wetness / #weather. Render rows (Render_Bind_WetnessSamplers 0x1447192f0, Render_Gfx_CreateTrackConstantsAndPuddleBuffers 0x1447889a0) were not touched.
- 2026-11-17 04:46 rr-bindings: Binding request (rr-bindings, 2026-11-17; doc:re/research/weather_strategy_bindings.md#functions): please review for L2 and export. (1) Render_SceneView_BuildViewParams 0x143be0360 (L1, sig render.scene_view_build_view_params): rcx (rsi) = view params block it fills, rdx (rdi) = source view; projection at +0x70 (0x143be0902), world-to-view +0xb0.. (0x143be0815/0x143be082d), camera-to-world +0xf0.. (0x143be0855/0x143be086d/0x143be08ad), second pass 0x143be1629..0x143be1684; 27 callers. (2) site_Render_MainViewParamsCall 0x1401583d0 (L1, sig render.main_view_params_call): the main 3D view's call to it, in 0x140157d90-0x140158a63. Realistic Rain draws its puddle pass from this camera (inline hook, return address = site+5); both moved +0x25d0 / +0 on Steam 1.0.145.
- 2026-11-17 14:16 engine-re: engine-re 2026-11-17 render-pipeline round 1 (static, blind-verified V10803-V10816): new sections Lighting / Lightmaps / Materials / LOD / Wetness / Next rounds above this inbox; 15 new render rows (6 at L2 forced), 3 raised to L2 with corrected notes (0x143c07890 builds the material bind layout, 0x143d41770 sphere screen size, 0x1447192f0 skidmark layer samplers). Lane owner review requested. Runtime questions: T158 (sun curves vs solar position), T159 (lightmap location lookup).
