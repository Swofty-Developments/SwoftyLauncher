# F1 25 loading pipeline and what a hook can shorten (build d873a1c8)

Lane `load-game` (v1.10.0), 2026-11-17, rounds 1 to 3. **Static reading only**: no game run, no debugger, no trace made for
this note. Every address is a VA on the KB build (`F1_25.exe` sha256 `d873a1c8…`, image base `0x140000000`). Decompiles were
made with `re/tools/decomp` from the `f1-modding` Ghidra pool; the KB rows cited carry their own `decomp:` paths, and "own
decomp" means a decompile made for this note. Levels follow `re/PROTOCOL.md` §1. Nothing here is L3: no claim about seconds
saved is measured, and §6 lists the measurements that would make it so.

**Two builds.** The KB build is `d873a1c8…` (PE timestamp 1789118616, 408,063,872 bytes). The game updated on the night of
16 to 17 November to `46009bd3…` (timestamp 1794312636, 391,613,312 bytes), which the core lists as verified but not as the
KB build: `runtime::same_build()` (`native/src/core/runtime/stage.cpp:206`) is false on it, and the core's title helper and
direct start, both gated on it, do not run there (§6). Each timing below names its build.

Round 3 adds three kinds of evidence that need no game run:

- **Retail config.** `system/config.xml`, `config_platform.xml` and `config_sku.xml` read out of the installed `game.dat` (NeFS,
  header inside `F1_25.exe`) with `packages/formats` (`nefs::Archive::open_exe`, `read`), as plain files. The values are the
  installed build's (`46009bd3…`). They are in `engine_settings.tsv` (columns `retail` and `retail_file`): 363 of the 602
  named call sites (329 of 525 names) have a value there; the rest run on the code default.
- **Flow data.** `system/state_machine.xml` and the seven `game/flow_*.xml` from the same archive.
- **Existing logs.** `loader.log` of earlier sessions on this machine (`~/.local/share/swofty/sessions/`) and of player bundles
  (`/mnt/work/F1/f1logs`). The core logs every flow state it knows by name and every other state once per process, at its first
  entry, as `#<djb2 of the node name>`; the node names from the flow files map those back. Times below marked "log" come from
  these, not from a run made for this note.

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
| engine config | step 5 `Boot_Load_SystemConfig` 0x1443c24b0: `system/config.xml`, `config_platform.xml`, `config_sku.xml`, `memory_map.xml`, `modules.xml` into `g_Boot_Config` | real work (small) | L1; retail values §4 |
| game database | step 10 `Boot_Load_GameDatabase_Step` 0x1443c45e0 → `Gdb_Load_GameDatabase`, main thread | real work | L2 |
| hardware settings | step 15 `Boot_Init_HardwareSettings` 0x1443c4fa0: presets, `-benchmark`, `Testing.OverrideGraphicsPreset` | real work | L1 |
| job system | step 17 `Boot_Init_JobQueues` 0x1443c8d70: worker count = min(logical processors, 32) unless `JobQueueSystem.workerThreadCount` (a value below 1 is added to the processor count; retail 0, so every logical processor); retail stack 2 MiB, spinning off, spin count 16 / 10 µs, raiser thread on, `assignAllJobsToCurrentThread` on and `wakeAllEligibleThreadsOnJobSubmission` on (both from `config_platform.xml`) | real work | L1 (own decomp); the last two settings' meaning was not read (L0) |
| modules | step 19 `Boot_Create_Modules` 0x1443cb030: Update/Render/Single thread contexts from `modules.xml` | real work | L2 |
| window + D3D12 device | step 22 `Boot_Create_Window` 0x1443cc250 reads every `Renderer.Gfx.*` hint, `maxPsoCreationThreads` (i32 at 0x1443cd336), `jitPsoCreation` (bool at 0x1443cd379; retail false), then `Render_Gfx2Device_Initialise` 0x1439bc2e0 (adapter, descriptor heaps, PSO worker threads `CreatePSOs_%d`) | real work | L2 for the device (KB), L1 for the two PSO values (§1.4) |
| ERP reader + asset groups | step 24 `Boot_AppModule_Connect` 0x1443db170: `ErpLoader.*`, `AssetGroupLoader.*`, `ResourceMessageHandler.*` (§1.4), `Asset_Create_GroupLoader` parses `fileList.manifest` | real work | L2 (KB) |
| boot flow | `StateApp` → `StateEgoBoot` → `StatePersistent` → `StateFrontendFrameLock`/`StateFrontendWorld` → `StateIntro` (`state_machine.xml`); boot-resident groups (shader packages, front-end UI) load through the ERP reader and their PSOs are created on the `CreatePSOs_%d` workers | real work, asynchronous | L2 for the order (flow data), L1 for what each waits on |
| splash, first-time boot | `Game_SplashTransitionViewModel_OnScreenActivated` 0x141e9b9f0 (`UI.skipFirstTimeBoot`, HDR calibration), `Game_FrontendTransitionVM_OnScreenActivated` 0x141bb7bf0 (first-boot slideshow) | first boot only | L1 |
| boot video, legal screens | screens `video_1`, `legal`, `legal_2`, `seizure_warning`, `splash` open in that order, about 2 s apart (log, §1.7); `video_1` and `legal` open during `StateFrontendWorld`, before `StateIntro` | fixed screens and the video | L1 (log) |
| boot sting | `Game_TitleScreenVM_OnScreenActivated` 0x141e9d630 binds `boot_video = cm_f1_sting.bk2` (loose file in `videos/`) and `attract_video = attract.bk2` (`UI.EnableAttract`, default 1; 52.7 s); played through `Ne_Play_FullscreenVideo` 0x14416de50. Since #1268 the core addon puts its own 6.0 s intro there (§1.7) | the video | L2 (KB rows) |
| title screen | `FeFlow_StateIntro_OnUpdate` 0x1436da4d0, §1.3 | the player's press | L1 (binding record, §1.3) |
| profile, front end | `StateOnlineConnect` (no wait: `FeFlow_StateOnlineConnect_OnEnter` 0x1436e42e0 returns at once, KB L1) → `StateFTUE` (`FeFlow_StateFTUE_OnUpdate` 0x1436d80e0 stays while the `enter_boot` route record that `StateIntro` stored is pending: the front end's boot route, KB L1) → `StateFrontend` | UI route | L1 |
| sign-in, EA login, entitlements | the title's press starts the platform sign-in (player manager +0x1f0); `Game_EALoginTask_*` 0x143372760.., `Game_PlatformAuthorisationTask_Update` 0x14334e760; retail `EADP.autoLoginEnabled` true, `OnlineServices.OfflineMode` false | network | **out of scope**: cost noted, not touched |

**Measured boot (log).** A menu-started Time Trial on this machine under Wine (session `20261117-045733`, build `46009bd3`;
the title was passed 54.8 s after it came up, by a press that was not the test agent's), seconds from the loader's attach:

| milestone | s | step |
|---|---|---|
| game `main` (CreateMutexW) | 0.36 | |
| window stage (step 22) | 4.05 | 3.69 engine boot (steps 1–21: game database, job system, modules) |
| `StateApp` / first `Present` | 5.92 / 6.02 | 1.97 device and the first frame |
| `StatePersistent` | 8.03 | |
| `StateFrontendWorld` | 9.23 | |
| `StateIntro` (title state) | 11.39 | 5.36 from the first frame: boot flow, boot-resident groups, PSOs |
| press (`StateOnlineConnect`) | 66.19 | the press |
| `StateFrontend` | 72.92 | 6.73 after the press (`StateFTUE`: the front end's boot route) |
| home screen open | 85.14 | 12.22 after `StateFrontend`, including the online-services error popup at 78.1 (below) |

Sixteen test-agent runs on 2026-11-16 (build `d873a1c8`, stock sting, `20261116-022626` to `-095053`) show the same shape:
`StateIntro` 10.1–13.6 s after attach, `video_1` to `splash` 4.7–9.0 s, `splash` to the agent's press 5.8–7.0 s,
`StateIntro` to the press 8.1–13.2 s. The agent presses about every 2 s from 3 s into the title until it passes, so its
press time is when the title takes input, to within about 2 s.

**Offline start (task: does a start without network wait on a timeout before the title?).** No such wait was found:

- The flow before `StateIntro` has no online state (`state_machine.xml`; L2 by the data). `StateOnlineConnect` comes after the
  press and does not wait (KB L1).
- The NTP client (`ntp::Manager`) is polled, never waited on (own decomps of 0x143292290, 0x1432928b0, 0x143291870, located by
  the xrefs of its log strings): name resolution is started (0x143aeb7a0), flagged pending at +6 and polled on later updates
  (0x143aebc90); the socket is made non-blocking (`ioctlsocket(FIONBIO)`) and read on its own thread (0x143291870); the request
  step sends at a QPC-timed interval and returns. L2 for "does not block its caller".
- The F1OS web requests (`f1sredirect.codemasters.com/...`) are request ids with completion callbacks ("Request %d is
  complete"; L1 from strings) and the EADP boot report is submitted asynchronously (log).
- Log: the sixteen runs above reach `StateIntro` 10.1–13.6 s after attach, and `20261117-045733` at 11.4 s, with no gap of
  a timeout's size between boot stages. None of them was a start with the network down, so this shows that normal starts do
  not wait, not the no-network case (L1). One run with the network unplugged and the report's M0 boot phases settles it.

The only network-shaped cost on the way to the menu comes after the title: every session on 16 and 17 November that reached
`StateOnlineFrontend` (31) shows the game message `online_services_generic_exception_online_with_code`, a median 6.0 s
after that state (5.2–8.5 s), before the home screen. That is the online services' own error handling: noted, not traced,
out of scope.

### 1.2 A session load (Grand Prix, Time Trial, career hub)

| stage | driver | what ends it | level |
|---|---|---|---|
| request | `Sess_Loader_RequestSessionLoad` 0x142d470c0 ("Session load requested for %s"); flow state `StateLoadSession` | — | L2 |
| front-end assets, cars, drivers | `Sess_Loader_StartFrontendAssetLoad` 0x142d47210 → `Ne_AssetLoader_RequestGroupHandles` 0x1409a7510, `Sess_Loader_CollectVehicleAssetGroups` 0x140612a80, `Game_VehicleAssetManager_ChangeResidentAssets` 0x140818e30 | real work, asynchronous | L2 |
| loader state machine | `Sess_Loader_Update` 0x142d4a480 (per frame), `Game_StreamingAssetLoadTask_Update` 0x142d4b6a0 | real work, polled once per frame | L2 |
| track | `Sess_Loader_SetupTrackAssets` 0x142d48b70, `Sess_Loader_BuildTrackRegionList` 0x142d49050 | real work | L2 |
| track load state | `Sess_Loader_RequestTrackLoad` 0x142da7870: holds until **3.0 s** after its QPC stamp (state+0xea0) unless `Game.IgnoreMinimumLoadingTime` | **timer** and the weekend manager | L2 |
| grid thumbnails | §1.5 (retail: the thumbnail system is on) | thumbnails ready, or their timeout | L2 for the wait, L1 for which ids |
| vehicle spawning | retail `Game.VehicleSpawning.VehicleTimeout` 45 s (`config.xml` also sets `MaxConcurrentSpawns` 6, which no getter call reads by name) | spawns, 45 s cap | L1 |
| cutscene prep | `Game_CutsceneManager_IsPreparingScenes` 0x140211e40, `Game_CutsceneManager_WaitTextureStreaming` 0x140212210 (gives up after 30.0 s) | streaming, 30 s cap | L2 |
| session transition | `Sess_SessionTransition_OnStateUpdate` 0x1436f9890: **pure timer**, 3.0 s since state+0xd98 | **timer** | L2 (V9348) |
| intro video | flow node `IntroVideo` (`SessionStateIntroVideo`, `videoType="FullSwipe"`); `Sess_IntroVideo_OnStateEnter` 0x142d6cfc0 / `Sess_IntroVideo_CanFinish` 0x142d6d3b0; `Game.SkipIntroVideo` (state+0xdc6 from `Sess_IntroVideo_ParseParams` 0x142d6cc30) | the video | L2 |
| talking points | flow node `TalkingPoints` (`SessionStateTalkingPoints`) after `IntroCameras` in race and practice flows, not in Time Trial; links `next`, `snapshot`, `skipped`, `loop` | the broadcast sequence, or the player's skip | L2 for the node (flow data), L1 for the rest (§1.8) |
| grid rundown | flow nodes `GridRundownType`, `GridRunDown`, then `F1Swipe` (a short sting) and `PreRace` | the sequence | L2 (flow data), §1.5 |
| story hub | `Sess_StoryHub_OnStateEnter` 0x1436fb5e0: same 3.0 s minimum from state+0xe20 | **timer** and the scene | L2 (V9363) |
| end of race | `PTS` (`SessionStatePurgeTextureStreaming`), `Game_SessionState_PurgeTexturesAndWait` 0x142dab950 | real work | L1 |

Every loader step advances from a flow-state update, run once per frame, so polled waits notice completion up to one frame
late (about 16 ms per step at 60 fps). The QPC timers are wall-clock.

**Measured loads (log).**

| load | `StateLoadSession` | to `StateSession` running | intro video | talking points | to gameplay |
|---|---|---|---|---|---|
| Time Trial, Bahrain, from the menu (`20261117-045733`, `46009bd3`) | 8.78 s | 9.44 s | 1.59 s | — | 11.08 s (`InGame`, the garage) |
| practice 1 by direct start (`20261116-033718`, `d873a1c8`; a slow run, its boot stalled) | 12.24 s | 12.97 s | 1.76 s | 14.33 s | 29.29 s |
| benchmark race (`20261103-091514`, `d873a1c8`; its flow has no intro video or rundown) | 6.91 s | 8.43 s | — | — | 15.96 s (`InGame`) |
| player bundles, first sighting of each state (about 6,500 logs, duplicates included) | — | — | median 1.89 s | median 4.65 s | — |

### 1.3 The title screen and what `title_skip.cpp` does

`FeFlow_StateIntro_OnUpdate` 0x1436da4d0 (slot 9 of `vt_Sess_Intro` 0x14585bff0) is the title's flow state. Round 3 moved its
row, signature (`fe_flow.state_intro_on_update`, the same pattern `title_skip.cpp` uses) and verify row (V1486, ts-v1, agree)
out of the shared tables into `re/bindings/fe_flow/FeFlow_StateIntro_OnUpdate.json` with `binding.py unfold`, and added a
second proof on the KB exe (load-game: one scan match at 0x1436da4d0, sigcheck passed). It stays L1 and not exported until its
subsystem owner raises it with L2 evidence (`binding.py status`). From the record and `native/src/core/testing/title_skip.cpp`:

1. It stays (`g_Game_FlowLinkStay`) until the title's sync point (state+0xcd0, `Vm_SyncPoint_IsComplete` 0x1442626c0) has
   both bool nodes true: the request node +0x80 and the UI's node +0xa0. The UI sets +0xa0 on the button press.
2. Besides the press, the title waits for its own view to exist (`[[state+0xc88]+0x140]+0x198`): in game the view comes up
   only after a press, so writing the sync point alone never passes the title (the comment at `title_skip.cpp:46`).
3. The press makes the primary player in the player manager (`NePlayerManager`, +0x198; sign-in state +0x1f0: 1 or 4 starts
   the platform sign-in, 3 completes it) and the lead local player (+0x1b0). This is the sign-in path: noted, not touched.
4. The state sends its own request (kind 0x2e, `FeFlow_Send_SceneRequest_Thunk` 0x14388bbe0, flag state+0xef3) that loads the
   profile and settings save; its callback `FeFlow_StateIntro_OnEngaged` 0x1436daa90 sets state+0xef4.
5. The next update returns the front-end link (0x146cb2288); `StateOnlineConnect` and `StateFTUE` follow (§1.1), then the
   main menu.

#### 1.3.1 Design: a player-facing "start at the main menu"

What there is to save, from the logs in §1.1: the boot video and legal screens (about 6–8 s from `video_1` to `splash`), the
time from `splash` until the title takes a press, and the player's own reaction. Nothing after the press gets shorter: the
front end's boot route (`StateFTUE`, about 6.7 s) and the menu build run either way.

Constraints:

- **The press is the player's identity.** The device that presses on the title becomes the primary player and the lead local
  player (item 3), and the platform sign-in starts from it. A press that the core makes (the virtual pad that `title_skip.cpp`
  uses for tests) makes the virtual device the primary player, so the player's own controller or wheel may not drive the
  menus, and the sign-in runs for the wrong device. A player feature must not make that press.
- **Pressing as the last-used device** would need the core to inject input under another device's identity, and to know
  which device that is before the game has enumerated it. Both are input synthesis, the device can change between sessions
  (a wheel unplugged, a pad added), and a wrong guess locks the player out of the menu with no way back but a restart. Not
  safe; not designed further.
- **Legal and photosensitivity screens stay.** `legal`, `legal_2` and `seizure_warning` are fixed notices of about 2 s each.
  A player feature keeps them.

The safe version, in three parts:

1. **The sting through content, not code.** Replace `videos/cm_f1_sting.bk2` with a short clip through the Asset API (§1.7).
   The core already does this with its 6.0 s intro (`startup_intro`, on by default); a "quick start" variant is the same
   operation with a clip of about 0.5 s. No hook.
2. **Let the player's first real press count as soon as the game can take it.** The title takes a press only once its view
   exists (item 2). Presses during the boot video and the legal screens are lost today, so a player who presses early
   presses again. The game's own input decides the primary player; the core only watches. The design: from `StateIntro`
   entry, keep the title's view request satisfied early (the safe sync-point writes that `title_skip.cpp` already makes through
   `Vm_NodeSet_Bool` 0x140973e20) so the view comes up while the legal screens run, and leave the press entirely to the
   player. Whether the view can come up before `splash` without breaking the screen order is not known (L0); that is what
   the experimental flag would measure.
3. **Never touch sign-in.** No write to the player manager (+0x1f0, +0x198, +0x1b0); `SWOFTY_DIRECT_SIGNIN` stays test-only.

What is implemented this round: nothing new in code. Part 2 needs a hook on `FeFlow_StateIntro_OnUpdate`, and the record is
L1 until its owner raises it. The core's existing hook there (`title_setup` in `title_skip.cpp`) installs only on the KB
build (`runtime::same_build()`, because the state offsets +0xcd0, +0xef3, +0xef4 are that build's) and acts only when the
test agent asks; on `46009bd3` it does not install and the agent falls back to the virtual pad. Once the record is L2 with its agree row, part 2 is one experimental feature (off by default) in the core lane: arm the
sync-point writes from `StateIntro` entry, no pad, no sign-in write, and log `[title] view up` and `[title] pressed by the
player` with times so the report (§6) can show the saving. Part 1 is content and needs no code.

### 1.4 The ERP reader and pipeline states

**ERP reader** (KB L2 rows, `kb:subsystems/assets.md#erp-loader`, `#erp-read`; own decomps of `Asset_Erp_ReadNext` and the ring
constructor 0x143a61020):

- `Boot_AppModule_Connect` reads `ErpLoader.readBufferSize` (u32, code default 0x200000), `readBufferCount` (u32, 4),
  `forceSequentialReads` (bool, 0), `useApr` and `Storage.UserMarkers.decompressErp`, and builds `Asset_Erp_Create_Loader`
  0x143a60bc0 with them. **Retail on PC** (`config_platform.xml`): 4 MiB × 4 buffers, `forceSequentialReads` true;
  `config.xml` alone would give 2 MiB × 4, false. The loader has its own `NeERPLoader` thread and 8 handlers on
  `EGO_ERP_LOADER_QUEUE`.
- The read buffers are **one allocation of size × count bytes** (0x143a61020: allocator vt+0x18) from the heap
  `EGO_ERP_WORKSPACE`, a fixed-size heap. **Retail** (`memory_map.xml`): 25 MiB, overridden to **90 MiB on Windows** (and
  PS5, Xbox), with the comment "A larger read buffer is required when using preloading on the erp loader, this grew from 2meg
  to 64meg". `EGO_ERP_TEMP` is 250 MiB on Windows.
- `Asset_Erp_PopNextRead` 0x143a68490 takes one ERP file at a time (FIFO). `Asset_Erp_ReadNext` 0x143a69720 aligns the wanted
  offset down to the buffer size, advances the ring index (`(+0x1c + 1) % count`), **spins until that buffer's refcount drops
  to one** (sections still decompressing hold references), then issues one asynchronous read of the buffer on the same file
  handle (0x143f38900). Reads within a file are sequential, one buffer at a time; the ring lets reading run ahead of
  decompression by up to count − 1 buffers. Across files, reads are sequential.
- With `forceSequentialReads` on (loader +0x3f6), `Asset_Erp_ProcessSection` 0x143a65bd0 takes the in-line sequential path for
  fragment types 2 and 3 instead of the branch at 0x143a65c80 (L1, own decomp).
- Decompression runs on `min(cores − 1, 32)` `NeErpRsDecompressor` worker threads (`Asset_Erp_Create_Decompressor`
  0x143a639b0, `Asset_Erp_Decompress_Worker` 0x143a6c700, zstd in 0x20000-byte chunks in 0x143a6d4b0) with the shared
  `combined.dict`. The game's stream decoder (0x1447b0a60) sizes its buffers from each frame's window and content size inside
  the same `EGO_ERP_WORKSPACE` heap (`native/src/core/assets/erp_zstd.hpp`). Under the retail config every one of those
  buffers and the read ring share that 90 MiB. Resource creation is sent back with
  `Asset_Erp_Send_CreateResource` 0x143a6b400.
- The consumer of the resource-creation messages on the main thread is throttled by
  `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` (float, default 10.0; not set in retail) and `_master`:
  `Boot_AppModule_Connect` stores them as doubles in a 0x58-byte object (vtable 0x1458a3440), and its slot 0x143a719c0 adapts a
  per-frame budget in milliseconds (+0x28) between a floor (+0x40, the setting) and a ceiling (+0x38, cfg+0x108) from the
  measured frame time. L1: the caller and the meaning of the ceiling were not read.

Conclusion with the retail values: the ring is 16 MiB of a 90 MiB heap, but the heap also holds the decoders' buffers of up to
32 decompression workers, so the head room depends on the core count and the archive's frames; the static read still cannot
say how much is free on a 16- or 32-thread PC. `forceSequentialReads` is on for PC only, which is the one loader value that
does not change an allocation: an M4 arm can force it off with a temporary `OVERRIDES` row (bool getter). **No flag shipped.**

**Pipeline states** (KB rows `Render_Gfx2Device_Initialise` L2, `Render_Gfx_ResolveDrawPipelineState` 0x143a01670 L1,
`Ne_Heap_OnCorruptionNotification` 0x1406cd740 L2):

- PSOs are built ahead of use on `CreatePSOs_%d` worker threads that `Render_Gfx2Device_Initialise` creates (string
  references at 0x1439be7e0 and 0x1439be9b3); `maxPsoCreationThreads` sets their number. **No config file sets it**, and its
  code default is read from the stack slot [rsp+0x14c] (0x1443cd320) with no store to that slot anywhere in
  `Boot_Create_Window` (0x1443cc250–0x1443ce7f8), so the effective value is whatever the slot holds (L1). M0's
  `loading_log_settings` line `[settings] read i32 Renderer.Gfx.maxPsoCreationThreads = … (status 2)` shows it.
- With `jitPsoCreation` off (retail false), a draw whose PSO does not exist is fatal ("DX12: PSO doesn't exist ->
  shadergroup:%s", `Ne_FatalAbort`), so the shipped path is "every PSO before its first draw". L1.
- No game-side PSO cache was found (no pipeline-cache file name, no pipeline-library use). PSOs are compiled every boot and
  reused across boots only through the driver's shader cache, or `vkd3d-proton.cache` in the game folder under Wine. L1.
- Verdict unchanged: `maxPsoCreationThreads` is worth a flag only if M0 shows the effective value below the core count;
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
- Which ids reach +0x168 and +0x1a8 from +0x188 was not traced (L1). **Retail**: `Game.Thumbnails.enableThumbnailGeneration`
  is true (code default false), `preloadTimeoutInSeconds` 7.5 and `driverPreloadTimeoutInSeconds` 20, so the thumbnail
  system runs and a stuck render costs at most those.

`Game.SkipGridRundown` therefore removes the rundown thumbnails from the race load and the rundown itself. Shipped as
`loading_skip_grid_rundown` (§3).

### 1.6 Loading screen delay

`SessionStateDelayOnLoadingScreen` (vtable 0x1457de6c0; binding record `Sess_DelayOnLoadingScreen_ParseParams`, L2 own decomp,
no agree row yet; its `delayLength` string is anchor `s_delayLength` 0x1457de760, read with `lea` at 0x142d58629):

- ParseParams (slot 7, 0x142d58600) reads the flow node parameter `delayLength`, parses it as a float (0x145343ff4) and stores
  the seconds at state+0xf94; onStateEnter (slot 8) zeroes the accumulator +0xf90; the update (slot 9, 0x142d586c0) adds the
  frame time to +0xf90 until it reaches +0xf94, then sets `ui_menu_data.loading_complete`.

**Retail: no flow file uses the state.** Neither `state_machine.xml` nor any of the seven `game/flow_*.xml` in `game.dat`
names `SessionStateDelayOnLoadingScreen` or `delayLength`, so the retail game never enters it (L2 by absence in the data the
game loads; `Flow.stateMachineFilename` is `system/state_machine.xml`). The candidate is dropped. The only fake loading screen
in the data is `StoryHubPostCinematicFakeLoad` (`SessionStateStoryModeFakeLoadingScreen`), story mode only, not read.

### 1.7 The boot sting as content

The file is `videos/cm_f1_sting.bk2` in the game folder (loose, not in an archive), with `videos/cm_f1_sting.srt` beside it
(one cue, 2.7–5.7 s, text `lng_subtitle_ea_sports_intro`). From the Bink headers (`KB2` + version, u32 frames at +8, width
+20, height +24, fps num/den +28/+32, audio tracks +40):

| file | header | frames | size | fps | audio tracks | length |
|---|---|---|---|---|---|---|
| stock EA sting, in the game folder between modded sessions (17,841,524 bytes, as `docs/make/startup-video.md` gives it) | `KB2j` | 355 | 1920×1080 | 30 | 8 | 11.83 s |
| core intro, `addons/swoftymod/startup_intro/cm_f1_sting.bk2` (9,338,608 bytes) | `KB2n` | 180 | 1920×1080 | 30 | 1 | 6.00 s |
| `attract.bk2` (idle title) | | 1580 | 1920×1080 | 30 | | 52.67 s |

**What the core already does.** Since #1268 (merged 2026-11-17) the core addon (`swofty.core`, setting `startup_intro`, on by
default) puts its intro in place of the sting for a modded session with a `file.put` op and sets
`lng_subtitle_ea_sports_intro` to "SWOFTY" (`addons/swoftymod/startup_intro/ops.json`); between sessions the folder holds
the stock file again (read at 19:00 on 2026-11-17). The core intro is 5.83 s **shorter** than the stock sting, so it cannot
make boot longer when the game waits for the video to end ("the game moves on when it ends", `docs/make/startup-video.md`).

The logs cannot show the saving. The sixteen runs of 16 November (stock sting, build `d873a1c8`) against the six test runs of
17 November 18:00–18:55 (build `46009bd3`, other addons): `video_1` to `splash` 4.7–9.0 s against 7.0–8.3 s, `StateIntro`
to the press 8.1–13.2 s against 8.3–13.1 s, and `video_1` 7.9–11.2 s after attach against 15.3–26.3 s. The game build, the
addon set and the machine's load all changed between the two sets, and the session folders do not record whether the core
intro was in place. Whether the sting holds the title is open (L1); the `boot_with_the_stock_intro` test in §6 answers it
on one build in one sitting.

**What an addon ships to shorten it further.** The same operation as the core's intro, documented in
`docs/make/startup-video.md`:

- `files/videos/cm_f1_sting.bk2` with `{ "source": "files/videos/cm_f1_sting.bk2", "target": "videos/cm_f1_sting.bk2" }` in
  `operations.files` of `swofty.json` (kind `files`). It wins over the core's intro without a conflict line; two addons that
  replace it both show in the build's conflict list, and the later one in load order wins.
- Format: Bink 2 (`radvideo64 binkc in.mp4 out.bk2 /O /#` from an H.264 and AAC MP4), 1920×1080, 30 fps, like both files
  above. A near-empty clip of about 15 frames (0.5 s) of black with one silent audio track is the shortest form that keeps
  the core intro's shape. Shorter clips, other resolutions and a clip with no audio track are untested (L0). The `.srt` cue
  starts at 2.7 s, so a clip shorter than that never shows the subtitle.
- Optional: `lng_subtitle_ea_sports_intro` through `lang.set` in an `assets` file (kind `text`).

No hook is needed, and none is proposed: a hook that blanks `boot_video` in `Game_TitleScreenVM_OnScreenActivated` would be
L1 and changes nothing the content route cannot.

### 1.8 Talking points

`TalkingPoints` (`SessionStateTalkingPoints`) runs after `IntroCameras` in the race and practice flows. In
`flow_practice.xml` (with `releaseCutscenesOnExit="true"`) its `skipped` link goes straight to `PostLoadReadyMonitor`; in
`flow_race.xml` both `next` and `skipped` go to `GridRundownType`, so in a race the skip saves only the talking points
themselves. In the slow direct-start practice log it took 14.3 s; across player bundles the median from its first entry to the
next state is 4.65 s (the player can skip it, so this is a lower bound for an unskipped sequence). The string
`Game.SkipTalkingPoints` sits in the exe beside the state's own strings but nothing references it: no rip-relative load and no
absolute pointer. The same holds for `Game.SkipIntroCameras` and its log line "Skipped SessionStateIntroCameras as
Game.SkipIntroCameras is true" (the one xref hit, 0x14434280f, falls inside other instructions). The skip code looks
compiled out of the retail build, so no setting gates these states (L1).
A flag would need a hook that takes the state's `skipped` link: a behaviour hook on a state nobody has recorded yet. Ranked
below; not started.

## 2. Ranked candidates

Ranked by likely seconds saved per load, then by safety. "Saved" is an upper bound from the static read and the logs.

| # | candidate | saves (inferred) | safety | evidence | status |
|---|---|---|---|---|---|
| 1 | `Game.IgnoreMinimumLoadingTime` | up to 3.0 s per session transition (always), per track load and story hub entry when the real load is shorter | high: the game's developer switch, three readers, not set in retail config | L2 ×3 + getter agree V0021 | **shipped**, `loading_skip_minimum_time` |
| 2 | `Game.SkipGridRundown` | the grid render of up to 32 driver thumbnails (the thumbnail system is on in retail; capped at 7.5 s / 20 s) and the rundown sequence | medium: removes a presentation sequence the player may want | L2 predicate and waits; L1 which ids; retail false | **shipped**, `loading_skip_grid_rundown` |
| 3 | `Game.SkipIntroVideo` | the intro video before a session: 1.6–1.9 s (log) | medium: the video may cover streaming | L2; retail false | **shipped**, `loading_skip_session_intro_video` |
| 4 | talking points (`skipped` link) | about 4.7 s median, up to 14 s, per race or practice session | medium: removes a broadcast sequence; needs a new hook | L2 node (flow data), L1 state; no setting reader | record the state's update, then a hook (§1.8) |
| 5 | the boot sting | done by the core's 6.0 s intro (stock 11.8 s); a 0.5 s clip saves up to 5.5 s more if the sting holds the title | high as content | header read; L1 whether it holds the title | content (§1.7); measure with `boot_with_the_stock_intro` |
| 6 | `Renderer.Gfx.maxPsoCreationThreads` | boot time spent creating PSOs, if the effective value is low | medium | L1; retail unset, code default an uninitialised stack slot | M0 first (log the effective value) |
| 7 | `ErpLoader.forceSequentialReads` false | unknown; parallel section reads on fast drives | medium: PC config sets it on deliberately; no allocation change | L1 | M4 arm with a temporary `OVERRIDES` row |
| 8 | ERP read buffers | unknown; fewer reader stalls | low: the 90 MiB workspace also holds up to 32 decoders' buffers; a bad value fails the boot allocation | L2 mechanism; retail 4 MiB × 4 | M0 first; then an M4 arm |
| 9 | `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` | main-thread resource creation per frame during loads | medium: longer frames during loading screens | L1 (consumer caller not read); retail unset | M0 first; read the caller of slot 0x143a719c0 |
| 10 | `Game_CutsceneManager_WaitTextureStreaming` 30 s cap | up to 30 s where streaming never reports ready | medium | L2 | later, once a log shows the cap hit |
| — | loading screen delay (`delayLength`) | nothing: no retail flow uses the state | — | L2 by absence (§1.6) | **dropped** |
| — | "start at the main menu" with a synthesised press | the player's reaction time | **unsafe**: the press picks the primary player | §1.3.1 | **not built**; the safe design is §1.3.1 |
| — | `jitPsoCreation` | boot PSO time, paid back as in-session hitches | low: off the tested path, missing-PSO is fatal otherwise | L1 | **not worth a flag** |
| — | sign-in, EA login, entitlement, online handshakes, the online-services error popup | network time, the popup | — | — | out of scope by rule |

## 3. The game setting override table

Code: `native/src/core/loading/game_settings.cpp` and `game_settings_logic.hpp`, registered as the core set `loading` in
`native/src/core/guards/registry.cpp`; host test `native/test/game_settings_test.cpp` (Unit `game_settings` in
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
  agree) and `Ne_Config_GetFloatOrDefault` 0x143a44090 (float, L2, V4071 agree). The wrap calls the original, then writes the
  forced value over `*out` for an armed row; the status is left as returned. With no row armed no mixin is offered.
- `Ne_Get_SettingDouble` 0x143a43ea0 (8-byte store; `TimeManager.fixedTimeStep`, `Flow.enterExitTimeLimit`) has a binding
  record (`re/bindings/core/`) and is not wrapped until a second contributor agrees.
- `loading_log_settings` (experimental, off): wraps all four getters and logs each distinct `name = value` the game reads once
  (and again when it changes), with the status (0 found, 2 missing = the code default). `native/tools/loading_report.py` lists
  these lines.
- `SWOFTY_LOADING_FEATURES` (comma-separated feature names) arms rows without the launcher, for measurement runs.
- Each feature is an experimental entry of `FEATURES` in `native/src/core/runtime/stage.cpp`, off unless listed in `"features"`
  of `swofty/assets.json`. It composes with `xanpavle.re-optimization`, which wraps the bool and u32 getters too.

Adding a setting is one row in `OVERRIDES` plus one `FEATURES` entry; a new getter type is one `Getter<>` specialisation.

Why it is safe for saves, online and anti-tamper: no game memory is written except the getter's output; the settings store is
never written, so `config.xml` and the player's settings files are untouched; the hook sites are `.xtls` function starts that a
shipped addon already wraps (bool, u32) or rows with agree verifications (i32, float); the changes are timers, a video and a
presentation sequence. Test offline first (the track load state checks the online session right after its gate, and the
intro video state has flags for online sessions).

## 4. Loading-related engine settings

From the full sweep, `re/research/engine_settings.tsv` (653 getter call sites, 602 with a literal name; L1 per row). "Code" is
the default at the call; "retail" is the value the installed build's config files give (the later of `config.xml`,
`config_platform.xml`, `config_sku.xml`); "—" means no file sets it and the code default applies.

| setting | type | code | retail | reader | gates |
|---|---|---|---|---|---|
| `Game.IgnoreMinimumLoadingTime` | bool | 0 | — | 0x142da791c (+2 readers) | the 3 s minima (§1.2) — shipped |
| `Game.SkipIntroVideo` | bool | 0 | false | 0x142d6cfad | the session intro video — shipped |
| `Game.SkipGridRundown` | bool | 0 | false | 0x140ac6b5c | grid thumbnails and rundown (§1.5) — shipped |
| `Game.SkipPreRace` | bool | 0 | false | 0x142d78cc5, 0x142d817ef | a pre-race sequence; not read |
| `Game.SkipRewardSequence` | bool | 0 | false | 0x142c7484d | the reward sequence after an event; not read |
| `Game.SkipTalkingPoints`, `Game.SkipIntroCameras` | — | — | — | none | strings with no reader (§1.8) |
| `Game.ForceTrackReload` | bool | 0 | — | 0x142d48106 | forces a track reload (slower; debug) |
| `Game.EnableLoading` | bool | 1 | true | 0x14365acb3 | the loading screen itself; not read |
| `Game.Thumbnails.enableThumbnailGeneration` | bool | 0 | **true** | 0x142e22ebd | thumbnail system on |
| `Game.Thumbnails.preloadTimeoutInSeconds` / `driverPreloadTimeoutInSeconds` | float | 7.5 / 20 | 7.5 / 20 | 0x142e22edc / 0x142e22efe | thumbnail wait caps |
| `Game.VehicleSpawning.VehicleTimeout` | float | previous | 45 | 0x142f3a6e0 | vehicle spawn timeout |
| `ErpLoader.readBufferSize` / `readBufferCount` | u32 | 2 MiB / 4 | **4 MiB / 4** | 0x1443db4f3 / 0x1443db50e | ERP read ring (§1.4) |
| `ErpLoader.forceSequentialReads` | bool | 0 | **true** | 0x1443db526 | loader +0x3f6 (§1.4) |
| `EGO_ERP_WORKSPACE` heap (`memory_map.xml`) | — | — | 25 MiB, **90 MiB on Windows** | heap guard | ring and decoder buffers |
| `ResourceMessageHandler.synchronousResourceProcessingTimeBudget` (+`_master`) | float | 10 | — | 0x1443db6c9 | main-thread resource budget (§1.4) |
| `JobQueueSystem.workerThreadCount` | i32 | min(cores, 32) | 0 (every logical processor) | 0x1443c93c7 | job workers |
| `JobQueueSystem.enableSpinning` / `workerThreadSpinCount` / `SpinIntervalUs` | bool / u32 | 0 / 0 / 0 | false / 16 / 10 | 0x1443c9d88.. | job wake latency |
| `JobQueueSystem.assignAllJobsToCurrentThread`, `enableRaiserThread`, `wakeAllEligibleThreadsOnJobSubmission` | bool | 0 | true (PC) | 0x1443c9d88.. | not read (L0) |
| `Renderer.Gfx.maxPsoCreationThreads` | i32 | uninitialised stack slot | — | 0x1443cd336 | PSO workers (§1.4) |
| `Renderer.Gfx.jitPsoCreation` | bool | 0 | false | 0x1443cd379 | PSO creation at draw (§1.4) |
| `Renderer.Gfx.WIN.uploadHeapSize` | u32 | 75 MiB | — | 0x1443cd29e | GPU upload heap |
| `Renderer.TextureStreaming.enabled` / `streamingCacheSize` | bool / u32 | 0 / 300 MiB | true / 350 MiB | 0x143b6e49a.. | texture streaming |
| `TimeManager.cap` / `capValue` / `mode` | bool / float / string | 1 / 0.08 | true / 0.0833 / `anti_spiking` | 0x1443d2036 / 0x1443d2091 | dt cap for dt-accumulated timers |
| `Flow.enterExitTimeLimit` | double | — | 300 | 0x144420b12 | flow state enter/exit limit; not read |
| `UI.EnableAttract` / `UI.skipFirstTimeBoot` / `UI.ShowLoadingTips` | bool | 1 / 0 / 1 | — / — / true | 0x1419f4f76 / 0x141bb827f / 0x141ca6719 | attract video, first-boot screens, tips |
| `WatchDog.enabled` | bool | previous | true | 0x1443c0a67 | hang watchdog (the core's watchdog guard owns it) |
| `OnlineServices.OfflineMode`, `EADP.autoLoginEnabled` | bool | — | false / true | 0x143369edd / 0x1406d7051 (+2) | **out of scope** (online, sign-in) |

## 5. Next steps in this area

1. Run M0 (§6) once: it names the effective values of every setting the game reads (including `maxPsoCreationThreads`) and the
   boot phase times, and decides items 6 to 9 of §2.
2. Run the A/B of §6 for the three shipped flags; ship as player options only on the pass rule.
3. Record `SessionStateTalkingPoints`' update (its `onStateUpdate` name string is loaded in 0x142d95b90, the likely update) and
   decide whether its `skipped` link can be taken without breaking the cutscene release (`releaseCutscenesOnExit`).
4. `FeFlow_StateIntro_OnUpdate`: L2 from its subsystem owner, then the experimental part 2 of §1.3.1.
5. Get a second contributor's agree row for `Ne_Get_SettingDouble`; then add it as a fifth getter type.
6. Read the caller of `ResourceMessageHandler` slot 0x143a719c0 (item 9) and what `assignAllJobsToCurrentThread` does on PC.

## 6. Measurement plan (in game, later)

**The report.** `native/tools/loading_report.py` (test: `native/test/loading_report_test.py`, Script `loading_report` in
`native/test/manifest.py`, `native/test/run_tests.sh loading_report`) reads one `loader.log` per run (a session folder, a test
run's `batches/<n>/` folder or the file; `LABEL=PATH` names the arm) and prints, per run and as medians per arm:

- **M0 boot:** attach → game `main` → window (step 22) → first `Present` → `StateApp`, `StateEgoBoot`, `StatePersistent`,
  `StateFrontendWorld`, `StateIntro` → title passed → `StateOnlineConnect` → `StateFTUE` → `StateFrontend` → home screen, and
  the phases between them.
- **Each session load:** `StateLoadSession` entry to exit (`load_state_s`), to `StateSession` running (`to_session_s`), to the
  first fade in (`to_fade_in_s`, with the game channels), track request to fade in, the intro video, talking points and grid
  rundown lengths (from the first entry of each node to the next one), and the load to `InGame` or, in a race, to `PreRace`
  (`to_gameplay_s`); the reason-3 thumbnail requests.
- The game's own `report_LoadingTimesBegin/End` pairs (channel `TELEMETRY`) when the reporter runs.
- The `[settings] read` lines of `loading_log_settings`: name, type, value, and whether a config file set it or the code
  default applied.
- **Regressions:** a crash (`[crash]`, or `end.json` `"end": "crash"`), a load-stall report, `PSO doesn't exist`, the override
  set not installing or a getter not resolving, a texture preload timeout, the thumbnail system disabled, an armed flag the game
  never read (or a flag read in the off arm), a load that never reached `StateSession`, grid thumbnails or a rundown longer
  than 1 s with `loading_skip_grid_rundown` armed, and a run that reached neither the menu nor a load. The exit code is 1 when
  any run has one.
- With an `off` arm, each other arm's median difference per phase and per load field.

Unnamed flow states are logged once per process, so the presentation lengths are for the first load of a run; each run below
is one game start with one load.

**The suite.** `native/test/ingame/loading.test.json` (`swofty test plan` lists 3 tests in 3 game starts), each test its own
game start (`fresh`) with no addons beyond the core:

| tag | test | what it runs | builds |
|---|---|---|---|
| `loading` | `boot_and_time_trial_load` | boot to the menu, route F1 World → Play, then the virtual pad: right, a ×4 (Time Trial → first season, team and track); waits for `StateLoadSession` and the garage's `InGame` (`#ba16bc16`) | any verified build; on `46009bd3` the agent passes the title with the virtual pad |
| `benchmark` | `benchmark_race_load` | `-benchmark` race, Bahrain, 1 lap, 20 cars, straight from launch; waits for `StateLoadSession` and `InGame`, then for the race to end | any verified build; its flow has no intro video and no rundown |
| `intro` | `boot_with_the_stock_intro` | boot to the menu with `swofty.core` `startup_intro` off (the stock sting) | any verified build |

A Grand Prix race (intro video, talking points, grid rundown) needs a direct start, `swofty test session grand_prix`, which
works only on the KB build: `direct_start.cpp` looks up its hooks only when `runtime::same_build()` holds, and on another build
the core logs `[direct] … start failed: …; the menu opens as usual`. On the installed build `46009bd3` the race arm cannot run
until the core's direct start supports that build (core lane), so M3 waits for it.

**Setup (once, from the checkout to measure).**

```sh
W=/mnt/work/F1/wt/v110-load-game
(cd "$W" && taskset -c 16-31 nice -n 10 native/build.sh)
(cd "$W" && CARGO_TARGET_DIR=/mnt/work/F1/scratch/v110-load-game-target taskset -c 16-31 nice -n 10 cargo build --offline -p swofty-cli)
SW=/mnt/work/F1/scratch/v110-load-game-target/debug/swofty
SUITE=$W/native/test/ingame/loading.test.json
REPORT="python3 $W/native/tools/loading_report.py"
FLAGS=loading_skip_minimum_time,loading_skip_session_intro_video,loading_skip_grid_rundown
OUT=/mnt/work/F1/scratch/loading-ab; mkdir -p "$OUT"; cd "$OUT"
export SWOFTY_LOADER_DIR=$W/native/dist
export SWOFTY_GAME_LOG=info
export SWOFTY_GAME_LOG_CHANNELS=default,SESSION_LOADER:info,FADE_REQUESTS:info,TELEMETRY:info,THUMBNAIL_SYSTEM:info
```

The CLI built from the worktree already falls back to its own `native/dist`; `SWOFTY_LOADER_DIR` makes that explicit. The
environment reaches the game (the launcher passes its own environment on). Working from `$OUT` keeps the `swofty-test/`
folders that `swofty test session` writes out of the worktree. The game is not started under `taskset` (it would inherit
the affinity). The CLI takes the game lock itself (`/mnt/work/F1/f1-game.lock.d`); `--wait 2h` queues behind a running
game instead of failing.

**One command per arm.** Five Time Trial starts and five benchmark starts per arm, on any verified build:

```sh
arm() {   # arm <label> <SWOFTY_LOADING_FEATURES value, empty for off>
  for i in 1 2 3 4 5; do
    SWOFTY_LOADING_FEATURES="$2" "$SW" test run "$SUITE" -t loading --wait 2h --out "$OUT/$1-tt-$i"
    SWOFTY_LOADING_FEATURES="$2" "$SW" test run "$SUITE" -t benchmark --wait 2h --out "$OUT/$1-bench-$i"
  done
}
arm off ""
arm on "$FLAGS"
```

On the KB build only, the Grand Prix race arm: a direct start into the race that waits for `StateSession` and the grid's
`PreRace` (`#2330fd27`), then quits. `swofty test session` copies no logs, so each run's session folder is linked by hand:

```sh
race() {  # race <label> <SWOFTY_LOADING_FEATURES value>
  for i in 1 2 3 4 5; do
    printf 'wait flow:StateSession 900\nwait flow:#2330fd27 300\nquit\n' |
      SWOFTY_LOADING_FEATURES="$2" "$SW" test session grand_prix track=bahrain session=race laps=1 --wait 2h
    ln -sfn "$(ls -td ~/.local/share/swofty/sessions/2*/ | head -1)" "$OUT/$1-race-$i"
  done
}
race off ""
race on "$FLAGS"
```

`track=` needs its name: a lone word is read as the box.

**The tables.** One report per kind of load, so each "load 1" median compares like with like:

```sh
$REPORT "$OUT"/off-tt-*/batches/1 "$OUT"/on-tt-*/batches/1 --json "$OUT/tt.json"
$REPORT "$OUT"/off-bench-*/batches/1 "$OUT"/on-bench-*/batches/1 --json "$OUT/bench.json"
$REPORT "$OUT"/off-race-* "$OUT"/on-race-* --json "$OUT/race.json"
```

Without labels the arm is read from the run (the race sessions' `env.txt`, or the core's `[loading] game setting overrides
wanted:` line in a test batch), so a flag that failed to arm shows up as its own arm.

**M0, `loading_log_settings` (one run, no A/B).**

```sh
SWOFTY_LOADING_FEATURES=loading_log_settings "$SW" test run "$SUITE" -t loading --wait 2h --out "$OUT/m0"
$REPORT "$OUT/m0/batches/1"
```

Decides `maxPsoCreationThreads` (a flag if its value is below the core count), whether the M4 arms are worth running, and the
resource budget.

**Boot sting.** Five runs of the stock sting against the five `off` Time Trial runs (core intro on), in the same sitting:

```sh
for i in 1 2 3 4 5; do "$SW" test run "$SUITE" -t intro --wait 2h --out "$OUT/stock-intro-$i"; done
lbl() { for p in "${@:2}"; do echo "$1=$p"; done; }
$REPORT $(lbl stock "$OUT"/stock-intro-*/batches/1) $(lbl core "$OUT"/off-tt-*/batches/1)
```

Compare `first_frame_to_title_state` and `title_state_to_press`. The agent presses a for 250 ms every 2 s until the title
passes (from 3 s in on the KB build, at once on `46009bd3`), so the press time is when the title takes input, to within
2 s. If the stock arm is about 5.8 s longer, the sting holds the title and a 0.5 s clip is worth shipping as content.

**What each flag should show.**

- **M1, `loading_skip_minimum_time`** (Time Trial and benchmark arms). Expected: `load_state_s` and `to_session_s` drop when
  the real load is shorter than the 3 s minima; session-to-session transitions drop by up to 3 s each. The line `[loading]
  Game.IgnoreMinimumLoadingTime read by the game` appears. Regression: a hang in a loading screen, a black or half-faded screen, the weekend manager missing,
  textures missing after the fade in, the HUD missing, any change in a career save after a restart.
- **M2, `loading_skip_session_intro_video`** (Time Trial arm; the race arm too on the KB build). Expected: `intro_video_s` near zero (from 1.6–1.9 s) and `to_gameplay_s`
  shorter by about that. Regression: the session starting before cars or track are visible, intro cameras missing their fade,
  a stuck black screen, any change in the session's settings or weather.
- **M3, `loading_skip_grid_rundown`** (race arm only, so the KB build only for now). Expected: no reason-3 thumbnail request and no `GridRunDown` time in the race; the race
  load shortens by the thumbnail render time (capped at 7.5 s / 20 s). The report flags any rundown or thumbnail left.
  Regression: the race starting without cars on the grid, the formation lap or start lights not running, missing driver
  portraits elsewhere in the session, any change in the grid order.
- **M4, ERP reader (only after M0, with a temporary `OVERRIDES` row each).** Arms: `ErpLoader.forceSequentialReads` false;
  then `readBufferCount` 8 at 4 MiB, only once the workspace's free space is known (nothing logs it yet). Expected: shorter
  `first_frame_to_title_state` and `load_state_s` on NVMe; no change on HDD. Regression: a boot failure (allocation), a load
  that never ends, corrupted textures.
- **M5, `maxPsoCreationThreads` (only if M0 shows it below the core count).** Boot with a cold shader cache (move
  `vkd3d-proton.cache` aside) and a warm one, value at M0's and at the core count. Expected: shorter
  `first_frame_to_title_state` with a cold cache. Regression: device removal, `PSO doesn't exist`, hitches in the first
  session.

Pass rule for shipping any flag as a player option: every run of both arms completes, the report shows no regression, and the
on arm is faster by at least one second on the median of its target field.
