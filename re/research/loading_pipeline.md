# F1 25 loading pipeline and what a hook can shorten (build d873a1c8)

Lane `load-game` (v1.10.0), 2026-11-17, rounds 1 and 2. **Static reading only**: no game run, no debugger, no trace. Every
address is a VA on the KB build (`F1_25.exe` sha256 `d873a1c8…`, image base `0x140000000`). Decompiles were made with
`re/tools/decomp` from the `f1-modding` Ghidra pool; the KB rows cited carry their own `decomp:` paths, and "own decomp" means
a decompile made for this note. Levels follow `re/PROTOCOL.md` §1. Nothing here is L3: no claim about seconds saved is
measured, and §6 lists the measurements that would make it so.

The game's own switches come first: every flag this lane ships forces a setting the game already reads (§3), and no game code
or data is patched.

## 1. The pipeline

### 1.1 Process start to the title screen

Boot steps run in a fixed order on the main thread from `Boot_Init_NeApplication` 0x1443c0560 (KB L2, step call sites
0x1443c0626..0x1443c0950).

| stage | driver | what ends it | level |
|---|---|---|---|
| log + Steam | step 1 `Boot_Init_LogAndSteam` 0x1443c0c80: log file, `SteamAPI_RestartAppIfNecessary`, `SteamAPI_Init` | Steam client reply (local IPC) | L2; platform start, not touched |
| file system | step 4 `Boot_Init_GameFileSystem` 0x1443c2320 mounts `game.dat` and the loose `/game/` | real work | L2 |
| engine config | step 5 `Boot_Load_SystemConfig` 0x1443c24b0: `system/config.xml`, `config_platform.xml`, `config_sku.xml`, `memory_map.xml`, `modules.xml` into `g_Boot_Config` | real work (small) | L1 |
| game database | step 10 `Boot_Load_GameDatabase_Step` 0x1443c45e0 → `Gdb_Load_GameDatabase`, main thread | real work | L2 |
| hardware settings | step 15 `Boot_Init_HardwareSettings` 0x1443c4fa0: presets, `-benchmark`, `Testing.OverrideGraphicsPreset` | real work | L1 |
| job system | step 17 `Boot_Init_JobQueues` 0x1443c8d70: worker count = min(logical processors, 32) unless `JobQueueSystem.workerThreadCount` (i32; a value below 1 is added to the processor count); `minWorkerThreadCount` 2, stack 1 MiB, spinning off | real work | L1 (own decomp) |
| modules | step 19 `Boot_Create_Modules` 0x1443cb030: Update/Render/Single thread contexts from `modules.xml` | real work | L2 |
| window + D3D12 device | step 22 `Boot_Create_Window` 0x1443cc250 reads every `Renderer.Gfx.*` hint, `maxPsoCreationThreads` (i32 at 0x1443cd336), `jitPsoCreation` (bool at 0x1443cd379, default 0), then `Render_Gfx2Device_Initialise` 0x1439bc2e0 (adapter, descriptor heaps, PSO worker threads `CreatePSOs_%d`) | real work | L2 for the device (KB), L1 for the two PSO values (§1.4) |
| ERP reader + asset groups | step 24 `Boot_AppModule_Connect` 0x1443db170: `ErpLoader.*`, `AssetGroupLoader.*`, `ResourceMessageHandler.*` (§1.4), `Asset_Create_GroupLoader` parses `fileList.manifest` | real work | L2 (KB) |
| shader packages, front-end groups | boot-resident asset groups (shader packages under `shader_package_2025`, front-end UI) load through the ERP reader; their PSOs are created on the `CreatePSOs_%d` workers as each shader group arrives | real work, asynchronous | L1 (no single "boot groups" list read) |
| splash, first-time boot | `Game_SplashTransitionViewModel_OnScreenActivated` 0x141e9b9f0 (`UI.skipFirstTimeBoot`, HDR calibration), `Game_FrontendTransitionVM_OnScreenActivated` 0x141bb7bf0 (first-boot slideshow), `FeFlow_StateFTUE_OnUpdate` 0x1436d80e0 | first boot only | L1 |
| boot sting | `Game_TitleScreenVM_OnScreenActivated` 0x141e9d630 binds `boot_video = cm_f1_sting.bk2` (loose file in `videos/`, 17.8 MB) and `attract_video = attract.bk2` (`UI.EnableAttract`, default 1); played through `Ne_Play_FullscreenVideo` 0x14416de50 | the video | L2 (KB rows) |
| title screen | `FeFlow_StateIntro_OnUpdate` 0x1436da4d0, §1.3 | the player's press | L1 |
| sign-in, EA login, entitlements | the title's press starts the platform sign-in (player manager +0x1f0); `Game_EALoginTask_*` 0x143372760.., `Game_PlatformAuthorisationTask_Update` 0x14334e760; `OnlineServices.OfflineMode` (default 0), `EADP.autoLoginEnabled` (default 0) | network | **out of scope**: cost noted, not touched |

No `Sleep`, frame-count wait or minimum-duration timer was found in the boot readers. The wall time before the title is the
game database, the device, and the boot-resident groups with their PSOs; the static read cannot rank those three (no trace),
which is what measurement M0 in §6 is for. Whether an offline start waits on a network timeout before the main menu is not
established: the EA login and authorisation tasks run as asynchronous online tasks, and the static read did not find the
main menu gated on them (L0; out of scope either way).

### 1.2 A session load (Grand Prix, Time Trial, career hub)

| stage | driver | what ends it | level |
|---|---|---|---|
| request | `Sess_Loader_RequestSessionLoad` 0x142d470c0 ("Session load requested for %s") | — | L2 |
| front-end assets, cars, drivers | `Sess_Loader_StartFrontendAssetLoad` 0x142d47210 → `Ne_AssetLoader_RequestGroupHandles` 0x1409a7510, `Sess_Loader_CollectVehicleAssetGroups` 0x140612a80, `Game_VehicleAssetManager_ChangeResidentAssets` 0x140818e30 | real work, asynchronous | L2 |
| loader state machine | `Sess_Loader_Update` 0x142d4a480 (per frame), `Game_StreamingAssetLoadTask_Update` 0x142d4b6a0 | real work, polled once per frame | L2 |
| track | `Sess_Loader_SetupTrackAssets` 0x142d48b70, `Sess_Loader_BuildTrackRegionList` 0x142d49050 | real work | L2 |
| track load state | `Sess_Loader_RequestTrackLoad` 0x142da7870: holds until **3.0 s** after its QPC stamp (state+0xea0) unless `Game.IgnoreMinimumLoadingTime` | **timer** and the weekend manager | L2 |
| grid thumbnails | §1.5 | thumbnails ready, or their timeout | L2 for the wait, L1 for which ids |
| cutscene prep | `Game_CutsceneManager_IsPreparingScenes` 0x140211e40, `Game_CutsceneManager_WaitTextureStreaming` 0x140212210 (gives up after 30.0 s) | streaming, 30 s cap | L2 |
| session transition | `Sess_SessionTransition_OnStateUpdate` 0x1436f9890: **pure timer**, 3.0 s since state+0xd98 | **timer** | L2 (V9348) |
| loading screen delay | §1.6 | **timer**, length from flow data | L2 for the parser, L1 for the update |
| intro video | `Sess_IntroVideo_OnStateEnter` 0x142d6cfc0 / `Sess_IntroVideo_CanFinish` 0x142d6d3b0; `Game.SkipIntroVideo` (state+0xdc6 from `Sess_IntroVideo_ParseParams` 0x142d6cc30) | the video | L2 |
| story hub | `Sess_StoryHub_OnStateEnter` 0x1436fb5e0: same 3.0 s minimum from state+0xe20 | **timer** and the scene | L2 (V9363) |
| end of race | `Game_SessionState_PurgeTexturesAndWait` 0x142dab950 | real work | L1 |

Every loader step advances from a flow-state update, run once per frame, so polled waits notice completion up to one frame
late (about 16 ms per step at 60 fps). The QPC timers are wall-clock; the dt-accumulated delay of §1.6 is not (see there).

### 1.3 The title screen and what `title_skip.cpp` does

`FeFlow_StateIntro_OnUpdate` 0x1436da4d0 (KB L1; slot 9 of `vt_Sess_Intro` 0x14585bff0; no signature, KB build only) is the
title's flow state. From the KB rows and `native/src/core/testing/title_skip.cpp`:

1. It stays (`g_Game_FlowLinkStay`) until the title's sync point (state+0xcd0, `Vm_SyncPoint_IsComplete` 0x1442626c0) has
   both bool nodes true: the request node +0x80 and the UI's node +0xa0. The UI sets +0xa0 on the button press.
2. Besides the press, the title waits for its own view to exist (`[[state+0xc88]+0x140]+0x198`): in game the view comes up
   only after a press, so writing the sync point alone never passes the title (the comment at `title_skip.cpp:48`).
3. The press makes the primary player in the player manager (`NePlayerManager`, +0x198; sign-in state +0x1f0: 1 or 4 starts
   the platform sign-in, 3 completes it) and the lead local player (+0x1b0). This is the sign-in path: noted, not touched.
4. The state sends its own request (kind 0x2e, `FeFlow_Send_SceneRequest_Thunk` 0x14388bbe0, flag state+0xef3) that loads the
   profile and settings save; its callback `FeFlow_StateIntro_OnEngaged` 0x1436daa90 sets state+0xef4.
5. The next update returns the front-end link (0x146cb2288); on a first boot `FeFlow_StateFTUE_OnUpdate` runs first, then the
   main menu. The menu is usable once the front-end scene's groups are resident; the EA login runs on afterwards as an online
   task.

What `title_skip.cpp` does, and which part could become a player-facing "start at the main menu":

| part | what it does | player-safe? |
|---|---|---|
| sync point writes | `Vm_NodeSet_Bool` 0x140973e20 on state+0xcd0 +0x80 and +0xa0 once the view exists | **the safe core**: the same writes the game's UI makes, through the game's own setter. Alone it does not pass the title (item 2) |
| the press | `core/testing/virtual_pad` holds A for 250 ms every 2 s from 3 s after the title until +0xef4 | **not as is.** The press decides which device becomes the primary player (item 3); a virtual pad pressing makes the virtual device the lead player, so the player's own controller may not drive the menus. A player feature has to deliver the press as the player's own device, or engage the title after the player's first real input |
| `SWOFTY_DIRECT_SIGNIN=1` | writes sign-in state 1 into the player manager on the first update | **test-only, out of scope** (sign-in) |
| `SWOFTY_TITLE_PRESS_AFTER_MS` | press delay | test tuning |
| the hook | `InlineHook` on `FeFlow_StateIntro_OnUpdate`, gated by `runtime::same_build()` | **not shippable to players yet**: the row is L1 with no signature or agree row; it needs a signature, L2 and an agree row first |
| the boot sting | `title_skip.cpp` does not skip it; its presses start 3 s into the title | the sting is a loose file, `videos/cm_f1_sting.bk2`: an addon can replace it through the Asset API file override (content, no hook). A hook that blanks `boot_video` in `Game_TitleScreenVM_OnScreenActivated` is possible but L1 |

So a player-facing "start at the main menu" saves the sting's length (content route) and the time the player takes to press;
it does not shorten any load. The core and launcher lanes own that toggle.

### 1.4 The ERP reader and pipeline states

**ERP reader** (KB L2 rows, `kb:subsystems/assets.md#erp-loader`, `#erp-read`; own decomps of `Asset_Erp_ReadNext` and the ring
constructor 0x143a61020):

- `Boot_AppModule_Connect` reads `ErpLoader.readBufferSize` (u32, 0x200000), `readBufferCount` (u32, 4), `forceSequentialReads`
  (bool, 0), `useApr` and `Storage.UserMarkers.decompressErp`, and builds `Asset_Erp_Create_Loader` 0x143a60bc0 with them. The
  loader has its own `NeERPLoader` thread and 8 handlers on `EGO_ERP_LOADER_QUEUE`.
- The read buffers are **one allocation of size × count bytes** (0x143a61020: allocator vt+0x18) from the heap
  `_DAT_14612a410->vt+8("EGO_ERP_WORKSPACE")`, a **fixed-size** heap from `memory_map.xml` (the core already works around its
  size for zstd windows, `core/assets/erp_zstd.hpp`). Stock: 8 MiB.
- `Asset_Erp_PopNextRead` 0x143a68490 takes one ERP file at a time (FIFO). `Asset_Erp_ReadNext` 0x143a69720 aligns the wanted
  offset down to the buffer size, advances the ring index (`(+0x1c + 1) % count`), **spins until that buffer's refcount drops
  to one** (sections still decompressing hold references), then issues one asynchronous read of the buffer on the same file
  handle (0x143f38900). Reads within a file are sequential, one buffer at a time; the ring lets reading run ahead of
  decompression by up to count − 1 buffers. Across files, reads are sequential.
- Decompression runs on `min(cores − 1, 32)` `NeErpRsDecompressor` worker threads (`Asset_Erp_Create_Decompressor`
  0x143a639b0, `Asset_Erp_Decompress_Worker` 0x143a6c700, zstd in 0x20000-byte chunks in 0x143a6d4b0) with the shared
  `combined.dict`. Resource creation is sent back with `Asset_Erp_Send_CreateResource` 0x143a6b400.
- The consumer of the resource-creation messages on the main thread is throttled by
  `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` (float, default 10.0) and `_master` (default = the
  first): `Boot_AppModule_Connect` stores them as doubles in a 0x58-byte object (vtable 0x1458a3440), and its slot 0x143a719c0
  adapts a per-frame budget in milliseconds (+0x28) between a floor (+0x40, the setting) and a ceiling (+0x38, cfg+0x108) from
  the measured frame time. L1: the caller and the meaning of the ceiling were not read.

Conclusion: the reader overlaps one read with decompression; a bigger ring could only help when the reader spins on a buffer
that decompression still holds, and a bigger buffer means fewer, larger reads. Both come out of a fixed heap whose size the
static read cannot see, so a wrong value can fail the allocation at boot. **No flag shipped**; the facility can force them
once M0 has logged the heap's size and the effective values.

**Pipeline states** (KB rows `Render_Gfx2Device_Initialise` L2, `Render_Gfx_ResolveDrawPipelineState` 0x143a01670 L1,
`Ne_Heap_OnCorruptionNotification` 0x1406cd740 L2; string xrefs this round):

- PSOs are built ahead of use on `CreatePSOs_%d` worker threads that `Render_Gfx2Device_Initialise` creates (string
  references at 0x1439be7e0 and 0x1439be9b3); `maxPsoCreationThreads` sets their number (its code default is a value already in
  the renderer description, not visible at the call). L1.
- With `jitPsoCreation` off (default), a draw whose PSO does not exist is fatal ("DX12: PSO doesn't exist -> shadergroup:%s",
  `Ne_FatalAbort`), so the shipped path is "every PSO before its first draw". `jitPsoCreation` presumably moves creation to the
  draw; that trades load time for in-session hitches and takes the game off its tested path. L1.
- No game-side PSO cache was found: no pipeline-cache file name and no pipeline-library use in the strings (the only
  `PIPELINE_LIBRARY` string is the D3D12 object-type name table). PSOs are compiled every boot and reused across boots only
  through the driver's shader cache (the heap-corruption handler's text names the NVIDIA shader-cache path), or vkd3d-proton's
  under Wine. L1.
- Verdict: `maxPsoCreationThreads` is worth a flag only if M0 shows the effective value is below the core count;
  `jitPsoCreation` is not worth a flag.

### 1.5 Do race loads wait on the grid thumbnails?

Yes, for the thumbnails the loader queued (own decomps of `Sess_Loader_Update` 0x142d4a480, `Sess_Loader_RequestLoadThumbnails`
0x142d4d040 and `Game_GridRundown_ShouldPlay` 0x140ac6b30):

- `Game_GridRundown_ShouldPlay` reads `Game.SkipGridRundown` first and returns false when it is on, before any session check
  (L2, agree V10470).
- `Sess_Loader_RequestLoadThumbnails` calls it for both variants; only when one is true does it enter loader step 0x1e, set the
  thumbnail system's reason 3 ("session loading") and queue a thumbnail for each of the 32 grid slots (session +0x988 + i) into
  the loader's +0x188 list (L2).
- `Sess_Loader_Update` holds the load until every id in its +0x168 list reports state 2 from 0x142e39bb0 before it sets
  +0x118, and the load's completion test needs +0x118; with feature-flag bits 45 and 53 allowing, it also holds the
  `VehicleSpawningSubTask` until every id in +0x1a8 reports 2 (L2 for the waits).
- Which ids reach +0x168 and +0x1a8 from +0x188 was not traced (L1). The thumbnail system's own timeouts are
  `Game.Thumbnails.preloadTimeoutInSeconds` 7.5 s and `driverPreloadTimeoutInSeconds` 20 s (reader 0x142e22c60), so a stuck
  render costs at most that.

`Game.SkipGridRundown` therefore removes the rundown thumbnails from the race load and the rundown itself. Shipped as
`loading_skip_grid_rundown` (§3).

### 1.6 Loading screen delay

`SessionStateDelayOnLoadingScreen` (vtable 0x1457de6c0; binding record `Sess_DelayOnLoadingScreen_ParseParams` added this round,
L2 own decomp, no agree row yet):

- ParseParams (slot 7, 0x142d58600) reads the flow node parameter `delayLength`, parses it as a float (0x145343ff4) and stores
  the seconds at state+0xf94.
- onStateEnter (slot 8, 0x142d586a0) zeroes the accumulator state+0xf90.
- `Sess_DelayOnLoadingScreen_Update` 0x142d586c0 (slot 9) adds the frame time to +0xf90 until it reaches +0xf94, then sets
  `ui_menu_data.loading_complete` and waits on the UI sync point.

The delay value is in the flow data in `game.dat`, which the static read cannot open, so the length is unknown. The frame time
is the game's dt, which `TimeManager.cap` (default on) limits to `TimeManager.capValue` (0.08 s): on a loading screen that
renders under 12.5 fps the delay runs longer than its nominal length in wall time (L1: that the state's dt is the capped one
is inferred from the slot's float argument). A flag that zeroes +0xf94 after ParseParams needs that function hooked, so it
waits for an agree row on the new record; it is not a setting, so the setting table cannot do it.

## 2. Ranked candidates

Ranked by likely seconds saved per load, then by safety. "Saved" is an upper bound from the static read.

| # | candidate | saves (inferred) | safety | evidence | status |
|---|---|---|---|---|---|
| 1 | `Game.IgnoreMinimumLoadingTime` | up to 3.0 s per session transition (always), per track load and story hub entry when the real load is shorter | high: the game's developer switch, three readers, no data changes | L2 ×3 + getter agree V0021 | **shipped**, `loading_skip_minimum_time` |
| 2 | `Game.SkipIntroVideo` | the intro video before a session | medium: the video may cover streaming | L2 | **shipped**, `loading_skip_session_intro_video` |
| 3 | `Game.SkipGridRundown` | the grid render of up to 32 driver thumbnails (capped at 7.5 s / 20 s by the thumbnail timeouts) and the rundown sequence | medium: removes a presentation sequence the player may want | L2 predicate and waits; L1 which ids | **shipped**, `loading_skip_grid_rundown` |
| 4 | loading screen delay (`delayLength`, state+0xf94) | that delay per use, more when frames are slow | medium | L2 parser record (no agree yet), L1 update | needs the agree row, then a hook on the parser |
| 5 | `Renderer.Gfx.maxPsoCreationThreads` | boot time spent creating PSOs, if the effective value is low | medium | L1 | M0 first (log the effective value) |
| 6 | ERP read buffers | unknown; fewer reader stalls on fast drives | low until the `EGO_ERP_WORKSPACE` size is known: a bad value fails the boot allocation | L2 mechanism | M0 first; then an i/u32 row |
| 7 | `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` | main-thread resource creation per frame during loads | medium: longer frames during loading screens | L1 (consumer caller not read) | M0 first; read the caller of slot 0x143a719c0 |
| 8 | the boot sting `cm_f1_sting.bk2` | its length on every boot | high as content | — | content route (Asset API file override); core/launcher lanes |
| 9 | `Game_CutsceneManager_WaitTextureStreaming` 30 s cap | up to 30 s where streaming never reports ready | medium | L2 | later, once a trace shows the cap hit |
| — | `jitPsoCreation` | boot PSO time, paid back as in-session hitches | low: off the tested path, missing-PSO is fatal otherwise | L1 | **not worth a flag** |
| — | sign-in, EA login, entitlement, online handshakes, `OnlineServices.OfflineMode` | network time, offline timeouts | — | — | out of scope by rule |

## 3. The game setting override table

Code: `native/src/core/loading/game_settings.cpp` and `game_settings_logic.hpp`, registered as the core set `loading` in
`native/src/core/runtime/loader/core_sets.cpp`; host test `native/test/game_settings_test.cpp` (Unit `game_settings` in
`native/test/manifest.py`, `native/test/run_tests.sh game_settings`).

- One row per forced setting: `{feature, setting name, type, forced value bits, effect}`. Types: bool, u32, i32, float. One
  feature may own several rows (they arm together). Rows today:

  | feature | setting | type | forced |
  |---|---|---|---|
  | `loading_skip_minimum_time` | `Game.IgnoreMinimumLoadingTime` | bool | 1 |
  | `loading_skip_session_intro_video` | `Game.SkipIntroVideo` | bool | 1 |
  | `loading_skip_grid_rundown` | `Game.SkipGridRundown` | bool | 1 |

- One `SWOFTY_MIXIN_WRAP` (priority 100) per getter type that has an armed row: `Ne_Get_SettingBool` 0x143a43b90 (L2, V0021
  agree), `Ne_Get_SettingU32` 0x143a43240 (L2, V0031 agree), `Ne_Map_FindOrDefaultWithErrorContext` 0x143a42f10 (i32, L2, V7824
  agree; own decomp this round: `(node, name, int *out, int def)`, strtol, status 0/2/0x21) and `Ne_Config_GetFloatOrDefault`
  0x143a44090 (float, L2, V4071 agree; own decomp: 4-byte store, default in xmm3). The wrap calls the original, then writes the
  forced value over `*out` for an armed row; the status is left as returned. With no row armed no mixin is offered.
- `Ne_Get_SettingDouble` 0x143a43ea0 (8-byte store; `TimeManager.fixedTimeStep`, `Flow.enterExitTimeLimit`,
  `Audio.speechVO_delayForRadioBeep`) has no KB row before this round; a binding record was added (`re/bindings/core/`) and it is
  not wrapped until a second contributor agrees.
- `loading_log_settings` (experimental, off): wraps all four getters and logs each distinct `name = value` the game reads once
  (and again when it changes), with the status (0 found, 2 missing = the code default). This is the way to learn what
  `system/config.xml` sets without opening `game.dat`.
- `SWOFTY_LOADING_FEATURES` (comma-separated feature names) arms rows without the launcher, for measurement runs.
- Each feature is an experimental entry of `FEATURES` in `native/src/core/runtime/stage.cpp`, off unless listed in `"features"`
  of `swofty/assets.json`. It composes with `xanpavle.re-optimization`, which wraps the bool and u32 getters too.

Adding a setting is one row in `OVERRIDES` plus one `FEATURES` entry; a new getter type is one `Getter<>` specialisation.

Why it is safe for saves, online and anti-tamper: no game memory is written except the getter's output; the settings store is
never written, so `config.xml` and the player's settings files are untouched; the hook sites are `.xtls` function starts that a
shipped addon already wraps (bool, u32) or rows with agree verifications (i32, float); the changes are timers, a video and a
presentation sequence. Test offline first (the track load state checks the online session right after its gate, and the
intro video state has `showInMultiplayer` flags).

## 4. Loading-related engine settings

From the full sweep, `re/research/engine_settings.tsv` (653 getter call sites, 602 with a literal name; L1 per row). Defaults
are code defaults; the config files in `game.dat` may override any of them.

| setting | type | default | reader | gates |
|---|---|---|---|---|
| `Game.IgnoreMinimumLoadingTime` | bool | 0 | 0x142da791c (+2 readers) | the 3 s minima (§1.2) — shipped |
| `Game.SkipIntroVideo` | bool | 0 | 0x142d6cfad | the session intro video — shipped |
| `Game.SkipGridRundown` | bool | 0 | 0x140ac6b5c | grid thumbnails and rundown (§1.5) — shipped |
| `Game.SkipPreRace` | bool | 0 | 0x142d78cc5, 0x142d817ef | a pre-race sequence; not read |
| `Game.SkipRewardSequence` | bool | 0 | 0x142c7484d | the reward sequence after an event; not read |
| `Game.ForceTrackReload` | bool | 0 | 0x142d48106 | forces a track reload (slower; debug) |
| `Game.EnableLoading` | bool | 1 | 0x14365acb3 | the loading screen itself; not read |
| `Game.Thumbnails.preloadTimeoutInSeconds` | float | 7.5 | 0x142e22edc | thumbnail wait cap |
| `Game.Thumbnails.driverPreloadTimeoutInSeconds` | float | 20 | 0x142e22efe | driver thumbnail wait cap |
| `Game.Thumbnails.enableThumbnailGeneration` | bool | 0 | 0x142e22ebd | thumbnail system on/off (the loader logs "rejected because the Thumbnail System is disabled") |
| `Game.VehicleSpawning.VehicleTimeout` | float | previous | 0x142f3a6e0 | vehicle spawn timeout |
| `ErpLoader.readBufferSize` / `readBufferCount` | u32 | 2 MiB / 4 | 0x1443db4f3 / 0x1443db50e | ERP read ring (§1.4) |
| `ErpLoader.forceSequentialReads` | bool | 0 | 0x1443db526 | loader +0x3f6; slower when on |
| `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` (+`_master`) | float | 10 | 0x1443db6c9 | main-thread resource budget (§1.4) |
| `AssetGroupLoader.rollbackOnLoadError` / `retryOnDescriptionLoadError` | bool | 0 | 0x1443dbd7f / 0x1443dbd96 | error handling, not speed |
| `JobQueueSystem.workerThreadCount` | i32 | min(cores, 32) | 0x1443c93c7 | job workers |
| `JobQueueSystem.enableSpinning`, `workerThreadSpinCount`, `wakeAllEligibleThreadsOnJobSubmission`, `enableRaiserThread` | bool/u32 | 0 | 0x1443c9d88.. | job wake latency; untested trade-offs |
| `Renderer.Gfx.maxPsoCreationThreads` | i32 | previous | 0x1443cd336 | PSO workers (§1.4) |
| `Renderer.Gfx.jitPsoCreation` | bool | 0 | 0x1443cd379 | PSO creation at draw (§1.4) |
| `Renderer.Gfx.WIN.uploadHeapSize` | u32 | 75 MiB | 0x1443cd29e | GPU upload heap |
| `Renderer.Gfx.vendorShaderPacksAllowed` | bool | 1 | 0x1436c827d | vendor shader packs |
| `Renderer.TextureStreaming.enabled`, `enabledPreloadOnly`, `forceLoadAll`, `autoPurge` | bool | 0 | 0x143b6e49a.. | texture streaming modes |
| `Renderer.TextureStreaming.streamingCacheSize` (+`_ext`) | u32 | 300 MiB | 0x143b6e8d2 | streaming cache |
| `TimeManager.cap` / `capValue` | bool / float | 1 / 0.08 | 0x1443d2036 / 0x1443d2091 | dt cap that stretches dt-accumulated timers (§1.6) |
| `Flow.enterExitTimeLimit` | double | — | 0x144420b12 | flow state enter/exit time limit; not read |
| `UI.EnableAttract` | bool | 1 | 0x1419f4f76 | attract video on the idle title |
| `UI.skipFirstTimeBoot` | bool | 0 | 0x141bb827f | first-boot slideshow and HDR screens |
| `UI.ShowLoadingTips` | bool | 1 | 0x141ca6719 | loading tips (presentation only) |
| `WatchDog.enabled` / `delay` | bool / u32 | previous | 0x1443c0a67 / 0x1443c0ac3 | hang watchdog (the core's watchdog guard owns it) |
| `OnlineServices.OfflineMode`, `OnlineServices.enabled`, `EADP.autoLoginEnabled`, `Network.Privilege.MultiplayerCheckRequired`, `RecommendedPCSpec.skipCheck` | bool | — | — | **out of scope** (online, entitlement, hardware gate) |

## 5. Next steps in this area

1. Run M0 (§6) once: it names the effective values of every setting above, the `EGO_ERP_WORKSPACE` heap size (from the heap
   guard's memory-map log) and the boot phase times, and decides items 5 to 7.
2. Get a second contributor's agree row for `Sess_DelayOnLoadingScreen_ParseParams`; then a flag that zeroes state+0xf94 after
   the parser (item 4).
3. Read the caller of `ResourceMessageHandler` slot 0x143a719c0 and the meaning of its ceiling (cfg+0x108) to L2.
4. Trace which grid ids reach the loader's +0x168 and +0x1a8 lists (§1.5, L1 part).
5. Give `FeFlow_StateIntro_OnUpdate` a signature and an agree row if the core lane builds "start at the main menu" on it, and
   solve the primary-player question (§1.3) first.
6. Agree row for `Ne_Get_SettingDouble`; then add it as a fifth getter type (8-byte value).

## 6. Measurement plan (in game, later)

Use the game's own timestamps: run with `SWOFTY_GAME_LOG=info` and
`SWOFTY_GAME_LOG_CHANNELS=SESSION_LOADER,FADE_REQUESTS,TELEMETRY,THUMBNAIL_SYSTEM,EGO_ERP_LOADER` (gamelog writes them into
`loader.log` with millisecond times). `TELEMETRY` carries `ElasticSearchReporterModule::report_LoadingTimesBegin(%s)` /
`End(%s)` when the reporter runs; `SESSION_LOADER` carries "Session load requested", "Track load requested", "Asset data has
been set up"; `FADE_REQUESTS` every fade; `THUMBNAIL_SYSTEM` "Requesting Setting Thumbnail Generation … with reason %d";
`EGO_ERP_LOADER` every `readNext` (offset per read: use it only in M0, it is verbose). Arm features with
`SWOFTY_LOADING_FEATURES=<feature>[,<feature>]`. Each A/B is five runs per arm, flag off then on, same save, same track,
offline.

**M0, `loading_log_settings` (one run, no A/B).** Boot to the main menu, start a Time Trial at Bahrain, return to the menu,
quit. Collect: every `[settings] read` line (effective values; status 2 means the code default), the heap guard's
`memory_map.xml` lines for `EGO_ERP_WORKSPACE`, and the times of process start, first `EGO_ERP_LOADER` line, the title's
`[title]`/fade lines and the main menu. Decides: `maxPsoCreationThreads` (flag if below the core count), the ERP ring (flag
only if the workspace has the head room), the resource budget.

**M1, `loading_skip_minimum_time`.**
1. Time Trial, Bahrain: from "Track load requested for" to the next fade in; then restart the session from the pause menu three
   times and time each.
2. Grand Prix weekend: Practice 1 → Qualifying through the session menu, session exit to the next fade in.
3. Story hub entry if a save is available.
Expected: restarts and session-to-session drop by up to 3 s each; the first track load from the menu changes little. The line
`[loading] Game.IgnoreMinimumLoadingTime read by the game` appears once.
Regression: a crash or hang in a loading screen, a black or half-faded screen, the weekend manager missing or empty, textures
missing for more than a moment after the fade in, the HUD missing, any change in the career save after a restart (hash the
save before and after one session, flag off and on).

**M2, `loading_skip_session_intro_video`.** Grand Prix and Time Trial start: "Session load requested" to the first fade in.
Expected: the intro video no longer shows; the time drops by about its length unless the next state waits on the same
streaming. Regression: the session starting before cars or track are visible, intro cameras missing their fade, a stuck black
screen, any change in the session's settings or weather.

**M3, `loading_skip_grid_rundown`.** Grand Prix race session with a full grid (20 cars; also a 22-car addon grid if one is
installed): from "Session load requested" for the race to the first fade in on the grid. Expected: no `THUMBNAIL_SYSTEM` line
with reason 3 during the load and no grid rundown before the formation lap; the load shortens by the thumbnail render time
(seconds; capped by the 7.5 s / 20 s timeouts). Regression: the race starting without cars on the grid, the formation lap or
start lights not running, a hang at the end of the load, missing driver portraits elsewhere in the session (pit board, timing
tower, results), any change in the grid order.

**M4, ERP ring (only after M0, with a temporary `OVERRIDES` row).** Boot to the menu and a Time Trial load with
`readBufferCount` 8 at the stock 2 MiB, then 4 × 8 MiB, each only if M0 shows the workspace heap has the extra head room.
Expected: shorter boot-to-title and track load on NVMe; no change on HDD. Regression: a boot failure (allocation), PLEASE WAIT
that never ends, corrupted textures.

**M5, `maxPsoCreationThreads` (only if M0 shows it below the core count).** Boot to the title, cold driver shader cache
(clear it) and warm, value at stock and at the core count. Expected: shorter boot with a cold cache. Regression: device removal,
missing-PSO fatal ("PSO doesn't exist"), hitches in the first session.

Pass rule for shipping any flag as a player option: every run of both arms completes, the on arm is faster by at least one
second on the median of its target transitions, and no regression above is seen in ten runs.
