# F1 25 loading pipeline and what a hook can shorten (build d873a1c8)

Lane `load-game` (v1.10.0), 2026-11-17. **Static reading only**: no game run, no debugger, no trace. Every address is a VA on
the KB build (`F1_25.exe` sha256 `d873a1c8…`, image base `0x140000000`). Decompiles were made with `re/tools/decomp` from the
`f1-modding` Ghidra pool; the KB rows cited carry their own `decomp:` paths. Levels follow `re/PROTOCOL.md` §1. Nothing here
is L3: no claim about seconds saved is measured, and §5 lists the measurements that would make it so.

## 1. The pipeline

### 1.1 Process start to the title screen

| stage | driver | thread | what ends it | level |
|---|---|---|---|---|
| engine config | `Ne_Load_SystemConfigXml` 0x1443c24b0 parses `system/config.xml` + `config_sku.xml` into the settings store (`g_Boot_Config`, read later through `*(0x146ca5e28)+0x18`) | main | real work (small) | L1 (research/hidden_modes_features.md §1) |
| job system | `Boot_Init_JobQueues` 0x1443c8d70 (boot step 17): `GetLogicalProcessorInformationEx`, worker count = min(logical processors, 32); `JobQueueSystem.workerThreadCount` (signed: a value < 1 is added to the processor count), `minWorkerThreadCount` (default 2), `workerThreadStackSize` (default 0x100000), spin and wake options | main | real work | L1 (decomp read, this note) |
| ERP reader | `Boot_AppModule_Connect` 0x1443db170: `ErpLoader.readBufferSize` default 0x200000, `ErpLoader.readBufferCount` default 4, `ErpLoader.forceSequentialReads` default 0, `ErpLoader.useApr` | main | real work | L1 (decomp read, this note) |
| window + D3D12 device | `Boot_Create_Window` 0x1443cc250: reads `Renderer.Gfx.enableVRS`, `enableAsyncCompute`, `maxPsoCreationThreads` (signed getter 0x143a42f10 at 0x1443cd332), `jitPsoCreation` (bool at 0x1443cd372, default 0) | main | real work | L1 (xref + decomp, this note) |
| front-end flow, splash | `Game_SplashTransitionViewModel_OnScreenActivated` 0x141e9b9f0 (`UI.skipFirstTimeBoot`, HDR calibration flow, `settings.current_settings.saved_at_least_once`); `Game_FrontendTransitionVM_OnScreenActivated` 0x141bb7bf0 (first-boot slideshow) | UI | first boot only: the slideshow and HDR screens; later boots pass straight through | L1 |
| title screen | `Game_TitleScreenVM_OnScreenActivated` 0x141e9d630 binds the legal messages (on the title itself, no separate timed legal screen), `boot_video = cm_f1_sting.bk2`, `attract_video = attract.bk2`, `show_attract` (`UI.EnableAttract`) | UI | the player's press: `FeFlow_StateIntro_OnUpdate` 0x1436da4d0 waits for sync point `state+0xcd0` (both bool nodes) and then the kind-0x2e request that loads the profile (`FeFlow_StateIntro_OnEngaged`) | L1/L2 (KB rows; `core/testing/title_skip.cpp` drives this path) |
| sign-in, EA login, entitlements | `Game_EALoginTask_*` 0x143372760.., `Game_PlatformAuthorisationTask_Update` 0x14334e760 | online tasks | network | **out of scope** (online integrity and entitlement): cost noted, not touched |

The boot sting `videos/cm_f1_sting.bk2` (17.8 MB) and the title wait are presentation the player sees; the time before them is
engine init that the static read shows as real work, not timers. No `Sleep` or minimum-duration loop was found in the boot
readers above. Where the shaders and pipeline states are created was not traced this round (§4, item 6).

### 1.2 A session load (Grand Prix, Time Trial, career hub)

| stage | driver | thread | what ends it | level |
|---|---|---|---|---|
| request | `Sess_Loader_RequestSessionLoad` 0x142d470c0 ("Session load requested for %s") | flow (main) | — | L2 |
| front-end assets, cars, drivers | `Sess_Loader_StartFrontendAssetLoad` 0x142d47210 → `Ne_AssetLoader_RequestGroupHandles` 0x1409a7510 (batched asset groups), `Sess_Loader_CollectVehicleAssetGroups` 0x140612a80, `Game_VehicleAssetManager_ChangeResidentAssets` 0x140818e30 | flow + asset loader | real work, asynchronous | L2 |
| loader state machine | `Sess_Loader_Update` 0x142d4a480 (per frame), `Game_StreamingAssetLoadTask_Update` 0x142d4b6a0 (waits until every resource id reports loaded, then unpauses texture streaming) | flow, one step per frame | real work, polled once per frame | L2 |
| track | `Sess_Loader_SetupTrackAssets` 0x142d48b70, `Sess_Loader_BuildTrackRegionList` 0x142d49050 | flow | real work | L2 |
| track load state | `Sess_Loader_RequestTrackLoad` 0x142da7870 ("Track load requested for %s"): stamps QPC at state+0xea0, then holds until **elapsed ≥ 3.0 s** (`Game.IgnoreMinimumLoadingTime` off) before it opens the weekend manager and fades in | flow | **timer** (3.0 s minimum) and the weekend manager state | L2 (decomp read again this round) |
| grid thumbnails | `Sess_Loader_RequestLoadThumbnails` 0x142d4d040 → `Game_GridRundown_ShouldPlay` 0x140ac6b30 (`Game.SkipGridRundown`), `Sess_Loader_PrepareGridThumbnails` 0x142d4c140 | flow + renderer | real work; `Game.Thumbnails.preloadTimeoutInSeconds` caps it | L2 |
| cutscene prep | `Game_CutsceneManager_IsPreparingScenes` 0x140211e40, `Game_CutsceneManager_WaitTextureStreaming` 0x140212210 (texture streamer states; gives up after 30.0 s) | flow | streaming, with a 30 s cap | L2 |
| session transition | `Sess_SessionTransition_OnStateUpdate` 0x1436f9890: **pure timer**, stays (`g_Game_FlowLinkStay` 0x146cb2280) until 3.0 s since state+0xd98, then sets `ui_menu_data.loading_complete` and leaves | flow | **timer** (3.0 s minimum) | L2 (verify V9348 agrees with the L1 read; decomp read again this round) |
| delay on loading screen | `Sess_DelayOnLoadingScreen_Update` 0x142d586c0: accumulates frame dt into state+0xf90 until state+0xf94, then `loading_complete`, then waits for the UI sync point | flow | **timer** (dt-accumulated; the length is a flow parameter, not read yet) | L1 for the timer (this note) |
| intro video | `Sess_IntroVideo_OnStateEnter` 0x142d6cfc0 / `Sess_IntroVideo_CanFinish` 0x142d6d3b0: the video before a session; `Game.SkipIntroVideo` (state+0xdc6, from `Sess_IntroVideo_ParseParams` 0x142d6cc30) lets it end at once | flow + video | the video | L2 |
| story hub | `Sess_StoryHub_OnStateEnter` 0x1436fb5e0: same 3.0 s minimum from state+0xe20 before the fade in | flow | **timer** (3.0 s) and the scene | L2 (verify V9363) |
| end of race | `Game_SessionState_PurgeTexturesAndWait` 0x142dab950: one full texture purge, waits on the texture manager | flow | real work | L1 |

Every loader step above advances from a flow-state update, which the game runs once per frame. The waits that poll
asynchronous work (`Game_StreamingAssetLoadTask_Update`, `IsPreparingScenes`) therefore notice completion up to one frame
late; at 60 fps that is about 16 ms per step, small next to the timers. The timers themselves are wall-clock (QPC) or
dt-accumulated, so a frame cap does not stretch them.

## 2. Ranked candidates

Ranked by likely seconds saved per load, then by safety. "Saved" is an upper bound from the static read.

| # | candidate | saves (inferred) | safety | evidence | status |
|---|---|---|---|---|---|
| 1 | `Game.IgnoreMinimumLoadingTime` on | up to 3.0 s per session transition (always: the state only waits), up to 3.0 s per track load and story hub entry when the real load is shorter (session restarts, practice to qualifying at the same track, quick TT reloads) | high: the game's own developer switch, read on every update by the three states, no data changes | L2 ×3 readers + L2 getter with agree V0021 | **implemented**, flag `loading_skip_minimum_time` |
| 2 | `Game.SkipIntroVideo` on | the length of the intro video before a session (several seconds; not measured) | medium: the video may cover streaming, so the session could fade in with textures still streaming | L2 (`Sess_IntroVideo_ParseParams`, `Sess_IntroVideo_CanFinish`) | **implemented**, flag `loading_skip_session_intro_video` |
| 3 | `Game.SkipGridRundown` on | the grid rundown and its 20-driver thumbnail renders during a race load | medium: removes a presentation sequence; whether the load waits on those thumbnails is inferred | L2 `Game_GridRundown_ShouldPlay` (agree V10470) | not implemented: needs the thumbnail wait confirmed (`Game.Thumbnails.preloadTimeoutInSeconds`, reader `Game_Read_GameConfig_4` 0x142e22c60 L1) |
| 4 | `SessionStateDelayOnLoadingScreen` length (state+0xf94) | that delay, per use | medium | L1 | read the state's ParseParams (vtable 0x1457de6c0 area, slot 7) and the flow XML value first |
| 5 | ERP read buffers (`ErpLoader.readBufferSize` 2 MiB, `readBufferCount` 4) | unknown; larger or more buffers may help fast NVMe drives | medium: memory, loader assumptions about buffer sizes | L1 (reader read, consumers not) | needs the consumer (the ERP reader object built at 0x1443db170) read to L2, then an A/B measurement |
| 6 | PSO creation (`Renderer.Gfx.maxPsoCreationThreads`, `jitPsoCreation`) | unknown; possibly boot and first-load hitches | medium-high risk: renderer internals | L1 (string xrefs into `Boot_Create_Window`) | trace where the two values go before anything else |
| 7 | the boot sting `cm_f1_sting.bk2` | the sting's length on every boot | high as content: it is a loose file in `videos/` | — | not a hook: an addon can replace the file through the Asset API; out of this lane |
| 8 | `Game_CutsceneManager_WaitTextureStreaming` 30 s cap | up to 30 s on low-VRAM systems where streaming never reports ready | medium: cutscenes may start with low-resolution textures | L2 | candidate for a later flag (cap at a few seconds) once a trace shows it hitting the cap |
| — | EA login, platform sign-in, entitlement and online handshakes | network time, offline timeouts | — | — | out of scope by rule |

Not found: no `Sleep` loop, fixed frame count or vsync-tied counter in the loader states read; the job system already sizes
itself from the processor count (up to 32 workers).

## 3. How the two implemented flags work

Code: `native/src/core/loading/setting_overrides.cpp` and `setting_overrides_logic.hpp`; host test
`native/test/loading_settings_test.cpp` (`run_loading_settings_test.sh`).

- One `SWOFTY_MIXIN_WRAP` on `Ne_Get_SettingBool` (binding `core.setting_get_bool`, 0x143a43b90, L2, verify V0021 agree; EA
  overlay hint `0x3a30bb0`). The wrap calls the original, then, when the name is an armed row of `OVERRIDES`, writes the forced
  value to `*out`. The return status is left as the original's (the three readers and the intro video reader ignore it).
- Each row has its own feature: `loading_skip_minimum_time` → `Game.IgnoreMinimumLoadingTime = 1`,
  `loading_skip_session_intro_video` → `Game.SkipIntroVideo = 1`. Both are experimental entries of `FEATURES` in
  `native/src/core/runtime/stage.cpp`, off unless listed in `"features"` of `swofty/assets.json`. With neither listed no mixin
  is offered and nothing is installed.
- A name outside `Game.` costs one five-byte compare. The getter has 350 call sites; the three minimum-time readers call it on
  every update of their state (once per frame while a loading screen is up), so the wrap stays a compare and a store.
- It composes with the third-party `xanpavle.re-optimization` addon, which wraps the same getter: wraps chain by priority.

Why it is safe for saves, online and anti-tamper: no game memory is written except the getter's output byte; nothing is
saved (the settings store is never written, so `config.xml` and the player's settings files are untouched); the hook site is
a `.xtls` function start already wrapped by a shipped addon; the change is a timer and a video, not data any peer or server
sees. Both flags should still be tested offline first (the track load state checks the online session right after the
gate, `Game_Online_GetLocalMachineEntry` 0x143067c10, and the intro video state has `showInMultiplayer` flags).

What could go wrong: the minimum may hide a race in the game (a fade-in before a dependent system is ready) that the
developers never saw because the 3 s always covered it; with the intro video skipped the session may appear before its
textures stream in. Either shows as pop-in or a black frame, not as a corrupted save.

## 4. Next steps in this area

1. Measure items 1 and 2 (§5) and raise the three readers' gate claims to L3 with a trace of the setting read.
2. Read `SessionStateDelayOnLoadingScreen`'s ParseParams and the flow XML to learn the delay length (item 4).
3. Confirm whether the race load waits on the grid rundown thumbnails (item 3); then add `Game.SkipGridRundown` as a third row.
4. Read the ERP reader that `Boot_AppModule_Connect` builds (item 5) to L2; then an A/B of 8 MiB × 8 buffers.
5. Trace `maxPsoCreationThreads` / `jitPsoCreation` into the renderer (item 6).
6. Find where the shader packages under `shader_package_2025` are turned into pipeline states at boot (`SHADERS`,
   `SHADERS_MERGE` log channels) and whether a cache exists across boots.

## 5. Measurement plan (in game, later)

Use the game's own timestamps: run with `SWOFTY_GAME_LOG=info` and
`SWOFTY_GAME_LOG_CHANNELS=SESSION_LOADER,FADE_REQUESTS,TELEMETRY` (gamelog writes them into `loader.log` with millisecond
times). `TELEMETRY` carries the game's own `ElasticSearchReporterModule::report_LoadingTimesBegin(%s)` / `End(%s)` lines when
the reporter runs; `SESSION_LOADER` carries "Session load requested", "Track load requested", "Asset data has been set up";
`FADE_REQUESTS` carries every fade in and out. Each measurement is five runs per arm, flag off then flag on, same save, same
track, offline.

**M1, `loading_skip_minimum_time`.**
1. Time Trial, Bahrain: from "Track load requested for" to the next `"…" requested a fade in`. Then restart the session from
   the pause menu three times and time each.
2. Grand Prix weekend: Practice 1 → Qualifying via the session menu: from the session exit to the fade in of the next
   session.
3. Braking Point / story hub entry if a save is available: from the hub request to its fade in.
Expected: with the flag on, the restart and session-to-session times drop by up to 3 s each; the first track load from the
menu changes little (its real load is longer than 3 s). The loader.log line `[loading] Game.IgnoreMinimumLoadingTime read by
the game` must appear once.
Regression: a crash or hang in a loading screen, a black or half-faded screen after the load, the weekend manager screen
missing or empty, missing textures for more than a moment after the fade in, the HUD missing on track, any change in the
career save after a restart (compare the save file hash before and after one session with the flag off and on).

**M2, `loading_skip_session_intro_video`.**
1. Grand Prix and Time Trial start: from "Session load requested" to the first fade in on track or in the garage.
Expected: the intro video no longer shows; the time drops by about its length, unless the next state waits on the same
streaming.
Regression: the session starting before the cars or track are visible, the broadcast intro cameras missing their fade,
a stuck black screen, or any difference in the session's settings or weather (compare the session's HUD weather and laps).

Pass rule for shipping either flag as a player option: every run of both arms completes, the on arm is faster by at least
one second on the median of its target transitions, and no regression above is seen in ten runs.
