# assets: eaid:// resolution, asset groups, worlds

Owner: see `subsystems.tsv`. Mostly unexplored (research/exe.md §5).

## Map
- World selection is data (world-variant table rows `worldEri`/`worldAssetGroup`), see research/exe.md §4. Code only formats `asset_groups/environment_package/tracks/%s/wep/%s_%s.assetgroup` for dev-session extra worlds (`Asset_Load_DevSessionWorlds`).
- Candidates for the resolver: `Asset_Parse_EaidA/B` (0x143b130a0/0x143b13710), `Asset_Map_EaidToGroupA/B` (0x14024fe80/0x140250ee0), `Asset_Get_AssetGroupsRoot` (0x1413b7640). Decompiles exist, not analysed.
- UI component ids are `md5(eaid path without extension)[:8]` big-endian (ui.md).
- Stock constant values of the vehicle and track shaders, and which ones were checked in game: [`gfx_material_constants.md`](gfx_material_constants.md).
- GfxMaterialRes (`.material`): F1 25 and F1 23 layouts and the F1 23 to F1 25 conversion rules are in [`gfx_material.md`](gfx_material.md). Tool: `tools/gfx_material.py`. Textures, SRVs, material bindings, meshes and models use the same byte format in both games. Materials differ: F1 25 has bindless `me_*HeapIndex` constants and no sampler records, while F1 23 has `<texture>.sampler` GfxSampleState references.

## Open
- Does the resolver support loose-file overrides? (Would remove the need to rewrite ERPs.)

## Resource sharing and per-box texture archives
- 2026-11-01 garage-art: when two loaded archives hold the same eaid name, the resource map keeps the first resource created; the second archive's bytes are skipped and its holder binds to the existing NeResource (refcounted). Evidence L2: `site_Asset_ResMap_AfterFindOrCreate` (resources.cpp `site`) reports `created` per find-or-create and the log text it prints for the two outcomes, "created by a stock ERP, NOT the overlay: the stock data wins until every holder releases it" and "the overlay found it ALREADY LOADED: the copy already in memory stays"; and players report the boot-resident shader pack overriding the track archives' garage textures (a 2020 Renault pack of 113 GfxSurfaceRes/GfxSRVResource pairs inside `vehicle_hologram_paint.nefx2.sm51.erp`). A texture that is not yet resident when its archive loads still comes from the first archive that reaches it.
- 2026-11-01 garage-art: stock garage and pit lane textures are not in a global archive. Each track's `<track>_common.erp` and its `wep/<track>_{f1,f1_26,classic,f1_myteam,f1_26_myteam,f1_previous_myteam,f1_previous_story,f2,f2_26}.erp` carry their own copy under `eaid://environment_package/core_assets/objects/textures/` (garage_liveries, garage, pit_lights, pit_perches, pitlane, race_day_r_n_d, race_day_interview_trucks, main_front_end/names/f1_team_names). Names carry the stock team slot token (`renault_a`).
- 2026-11-01 garage-art: texture resource layout, verified on all 53 textures below that root in the pack (tools/garage_textures.py --check): GfxSurfaceRes arg 9 with fragment 0 = 40 bytes `u32 2, 0x108, format, width, height, 1, mip count, 1, 0, 0` and fragment 1 = every mip level, largest first, block compressed with the minimum of one block per level (4096 square BC1, 13 levels = 11184824 bytes); GfxSRVResource arg 2 with one fragment = `u32 5, format, 0, mip count, 0, 0` followed by the surface name and a NUL (24 + name + 1 bytes). Format codes: 52/54 BC1, 55/57 BC3, 65 BC5, 69/70 BC7 (70 and 54 on the diffuse `_da` textures; the unorm/sRGB split of the other codes is read from the DXGI name, not proven). L2.
- 2026-11-01 garage-art: the car groups. A group the core registers for a car (livery or `group.register`) lists `<package>/teams/common.erp` and the car's wep archive. Whether those load before or after the track's world archives is NOT established offline (the order of the car asset group against the world group in session load is not read from the exe). The `team.garage` applier attaches the box's archive to the team's car groups and the in-game run (work/garage-test) decides it. L0 until then.
- 2026-11-01 garage-art: `runtime::group_extend` (assets.cpp) adds an ERP path to the ERP list of a group an addon declares, at the manifest parse; a group of the stock manifest is not extended. The overlay route (`resource_erps`) patches stock groups, but is read once at the early stage and the registry boot appliers run later (order read from the call sites, not traced), so the garage archive does not use it.

## ERP loading and track paths (engine-re 2026-11-17)
<a id="erp-load"></a>
Static only (Steam d873a1c8), round A of the track RE rounds. Nothing here is runtime-confirmed; open runtime questions are T200-T202.

**ERP parser.** `Game_EgoErpLoader_ProcessSection` 0x143a65bd0 (L2) reads the header and the resource index and creates the resources:
- Header: u32 `ERPK` (immediate at 0x143a671a4) and u16 version < 4 at +4, else `handleError(0x4f)`; a header shorter than 0x30 bytes gives 0x54. +6 u16, +0x10/+0x18/+0x20 u64.
- Index: u32 resource count, u32 fragment total; per entry u32 size, u16 name length (must equal strlen+1, else 0x56), name, 16-byte type name, u32 version, u8 fragment count. Fragment records are 0x14 bytes (version 1, compression code 0x10), 0x18 (version 2) or 0x21 (version 3: a signed compression byte, stored as 0 when < 1, and a u64 packed size) plus a 0x10 trailer. This matches F1_TrackLab `core/erp_core.py`.
- Errors go to `Game_ERPInfo_EError` 0x143a691a0 (handleError): it stores the code in ERPInfo+0xc, frees the buffers and closes the file.

**Type dispatch.** There is no switch on resource types in the loader. Each index entry goes to `Ne_ResourceMap_FindOrCreateProxy` 0x143995160 (L2):
- It looks the name up first. An existing name hash returns the existing proxy, so **the first loaded archive wins** (except a proxy in state 5, which is rebuilt) for a duplicate eaid name and the later bytes are skipped (logged as "already loaded ... wasteful").
- A new name gets a proxy whose descriptor is the 16-byte type name looked up in the type map (resMap+0x58) through `Ne_HashMap16_FindSlot` 0x1439970c0. **A miss is not checked** (HYPOTHESIS-static, T200): an unregistered type name gives a bogus descriptor, so the expected result is a crash at the first factory call, not a skip.
- Fragments are created through the descriptor (vt+0x40 accept, vt+0x28 create); fragments typed `tmp0`/`tmp1`/`temp` are skipped. A refusing factory releases the resource.
- The type names (TrackSpace, AISplineData, TrackInfoBinary, World, ...) are registered by their own static inits; for the render ones see `Render_StaticInit_ResourceTypeDescriptors` 0x143f13130 (render.md).

**Required vs optional.** The ERP layer itself requires nothing per type: any set of known-type resources parses. Requirements come from the consumers. Known consumer behaviour: a missing World *layer* is logged and skipped (`NeWorldLayerModule::registerLayer`, session.md#tracks-open); missing TrackSpace data logs "TrackSpacePropertyManagerNotFound - No Trackspace Data Available on the current track." (string 0x1457fa008); a missing TSQuadTree is rebuilt (tracks-runtime round). A failed asset group sets `NeAssetGroupSubTask` state 0 unless its +0x74 flag is set (then state 2 and "Continuing anyway... things may go wrong."), see `Ne_AssetGroupSubTask_OnComplete` 0x144429640. Settings `AssetGroupLoader.rollbackOnLoadError` / `retryOnDescriptionLoadError` default 0 (`Ne_Init_ResourceMessageHandlerConfig` 0x1443db170).

**Why V11H crashed with added resources / World layers: OPEN.** Statically, added resources with known types and new names parse and are created; duplicate names are silently ignored (the stock copy wins); an unknown type name is the only parser-level crash candidate found. A crash after `finishedLoadingERP` would be in a consumer (World/layer instantiation). T201 asks for the loader log of that build.

**Track paths.** No track path is built from a slot name in code except two:
- `Sess_Loader_BuildTrackRegionList` 0x142d49050 expands the track's `tracks.track_resource` row templates (row +0x60, +0x18, +0x10, +0x08) with `Game_TrackRecord_ExpandPathTokens` 0x142e4be70 (body 0x14f369fa0): tokens `FILENAME` = tracks.Track fileName (record+0x78), `FOLDER` = record+0x180, `BASETRACK` = baseTrack (record+0x128). It queues the results, the record's extra worlds and `region_on_track`.
- Track layer worlds: `Game_Read_TrackLayerRow` 0x142e587e0 reads `worldAssetGroup` / `worldEri`; `Game_TrackLayer_BuildExtraWorldDesc` 0x142e57e90 expands them with `FILENAME` and names the world `extraWorld_track_layer_<statId>`.
- Probes: `Game_TrackRecord_BuildProbeAssetGroupPath` 0x142e4c210 = expanded root + `probes_<set>.assetgroup`.
- Fixed paths: the DRS layer `eaid://environment_package/tracks/<fileName>/wep/<fileName>-drs.layer` (or `-sm.layer`) (`Game_TrackDrsLayer_Setup` 0x14299b480), podium lights `.../tracks/%s/wep/%s_podium_lights.layer` (0x145648f91, 3 refs) and the dev-session worlds (`Asset_Load_DevSessionWorlds`). These use the fileName, so a new code needs those files under `environment_package/tracks/<code>/wep/` (or the layer is simply not found).
- eaid:// names are resource names inside the ERPs of the loaded asset groups (resource map keyed by name); the asset manifest is `fileList.manifest` (`Ne_AssetManifest_Load` 0x143a5a550).

**Track slots.** The slot list is the `tracks.Track` table: `Game_Load_TrackDatabaseFull` 0x142e4c640 reads every row (statId, fileName, instanceName, baseTrack, track_resource, ...) and stores it with `Trk_TrackData_AddRecord` at items[statId] (statId is a byte: at most 256 ids). Nothing ties a row to a donor folder except its own fileName / track_resource templates, so a new code with its own folder needs no donor slot (consistent with the launcher's own-code tracks). Hard-coded names and limits found:
- `madrid` skipped in the options track bitset, std::bitset<64> (tracks.md Inbox 2026-11-13) - the only code-referenced track name found;
- `Game_TrackData_GetTrack` asserts on a mode or season id >= 32 (tracks.md#query);
- statId u8 (record+1);
- a 25-name legacy track table (`melbourne` ... `paul_ric`, 0x145805ec8) and a ghost/time trial file table (`madrid_dry.ghost`, `silverstone_reverse_wet.ghost`, ... around 0x145bd27ad) have no code reference found (rip-relative and abs64 scans), so they look like dead data;
- leads not read: `monaco` 0x1456115ff (4 refs, 0x14020a460, 0x140345f60, 0x140356a20, 0x15091ee40) and `_reverse` 0x145624725 (0x140363850, 0x140ac15d0).

<a id="track-names"></a>
## Hard-coded track names, FOLDER and limits for new track codes (round A2)
Static only (Steam d873a1c8), engine-re 2026-11-17. Method: every string in the exe that contains a stock track code (all `tracks.Track` file names plus `_reverse`), cross-referenced with rip-relative refs confirmed by decoding the instruction, abs64 refs, and a scan for the codes as immediates (movabs/dword compares) and as djb2 / FNV-1a hashes. Only the functions below test a track name in code; every other stock code (abu_dhabi ... zandvoort) has no code reference (its strings are lng keys, scenario_dynamic_bahrain_* weather scenario names, a sorted ghost file name pool and a shader path).

**Track name -> function -> effect**

| name | function | effect |
|---|---|---|
| monaco | `Game_CameraHelicopter_Reset` 0x14020a460 | helicopter camera height offset +100 |
| monaco | `Game_GridCutSceneScript_SetupDriverTalkToPress` 0x140345f60 | four extra prop cameras in the grid talk-to-press scene |
| monaco | `Game_Podium2019CutSceneScript_ChooseCutScenesToPlay` 0x140356a20, `..._SetMonacoBarrierVisibility` 0x140361f70 | podium hides world object `Env- Barriers$mon_armco_parcferme_a`; flag goes to SpawnTVScreens |
| monza | 0x140356a20 | podium flag +0x8f9 (SpawnTVScreens variant, one podium branch) |
| monaco | `Sess_ScCond_DynamicObstruction_Init_Body` 0x15091ee40 | safety car: areas 6 and 7 use obstruction_threshold_seconds_high_congestion_areas (street-track SC rule, Monaco only) |
| monaco (location) | `Game_TrackExtraMonacoBoat_Initialise` 0x14026a3c0 | boat vignette at locator `monacoboat` (data, not a name test) |
| las_vegas | `Game_Podium2019CutSceneScript_Starting` 0x1403622c0, `Game_PodiumTunnelScript_OnScriptEvent` 0x140368990, `Game_StoryPressInterview_Initialise` 0x141056fb0 | at night (time-of-day value > 1.658) render-state bool 0x23 stays 0 in the podium / press scenes |
| _reverse | `Game_Pod2019FirstScript_Initialise` 0x140363850 | podium scene `crowd_wide_cam_<name>`; a `_reverse` suffix is cut, so reverse layouts reuse the forward scene |
| any name | 0x140356a20 | the podium track name taken from the record is cut at its first digit (bahrain24 -> bahrain), so `monaco2` would get the Monaco podium rules (an overrideTrackName is not cut) |
| baseTrack set | `Game_TalkingPoints_BuildTrack` 0x140ac15d0 | key `talking_points.patterns.<fileName><session>` plus `_reverse` when baseTrack is non-empty; missing key -> `talking_points.patterns.generic` with the same suffixes (_practice/_qualifying/_race, _outro) |
| madrid | options factory 0x142ae7da0 | skipped in the 64-bit track option mask (tracks.md Inbox 2026-11-13) |
| montreal | `Game_TrackDataRecord_Ctor` 0x142e49f50 | default record name when no name is passed |
| bahrain / random | `Game_UiTrackPicker_SelectByName` 0x1416ed7e0 | UI picker default with no list / an empty list |
| hungaroring | `g_Sess_BenchmarkDefaultTrackName` 0x145f96d60 | -benchmark default track |

Not found: no name test for night races (DB `allow_night_start`, `timeofday`), for reverse layouts other than the two `_reverse` users above (`is_reverse_track` and `reverse_track_trophy_name` are DB fields), or for street tracks other than the Monaco SC areas. `use_monaco_tyre_rule_layout` is a results-table VM bool written false, with no name test.

**FOLDER token (static part of T202).** `record+0x180` is the data pointer of the record's 0x80-char FOLDER string (+0x170). `Game_Load_TrackDatabaseFull` initialises it empty and never writes it, so `[FOLDER]` expands to an empty string for every `tracks.Track` row; stock `track_resource` templates use only `[FILENAME]`. The only writer is `Game_TrackDataSystem_AddTrackFromPath` 0x142e60850 (dev sessions: `DevSessionDataContract.Track.worldAssetGroup`, path outside `/tracks/`).

**Queue order (static).** `tracks.track_resource` row: +0x08 worldAssetGroup, +0x10 worldEri, +0x18 crowdEri, +0x20 aoBake, +0x28 radianceBake, +0x30 lightList, +0x38 ddgi, +0x40 iblAssetGroupsRoot, +0x48 iblRoot, +0x50 iblKey, +0x58 iblSuffix, +0x60 trackMapAssetGroup, +0x70 iblProbes (`Sess_Load_TrackDatabase` 0x142e58f20). `Sess_Loader_BuildTrackRegionList` queues: trackMapAssetGroup (prio 10), the record+0xaf8 entries (10), the record+0xac0 regions (10), the world request from worldAssetGroup (10; worldEri and crowdEri become resource ids), region_on_track, then extra entries (prio 4). Bake and probe groups are expanded elsewhere. Runtime order: T205.

**Limits for a new track code (static).**
- statId is a u8; records sit at items[statId] (holes allowed).
- The options track mask is a std::bitset<64> over the tracks with TrackData (madrid skipped); more entries throw at boot.
- `Game_TrackData_GetTrack` asserts on a mode or season id >= 32.
- Career calendar: track_event_id is a byte and the save bag capacity is u16 (career.md Inbox 2026-11-12).
- Per-code files the code expects by name: `environment_package/tracks/<fileName>/wep/<fileName>-drs.layer` / `-sm.layer` and `%s_podium_lights.layer`; the Time Trial ghost `ghostlaps/<x>/<fileName>_<dry|wet>.ghost` (`Game_TimeTrial_LoadGhostLap` 0x142940120, missing file -> failure branch, T204); the podium scene `crowd_wide_cam_<name>` (T203).
- Monaco, Monza and Las Vegas special cases (table above) do not apply to a new code.
- Other track-related bitset users checked (all callers of `Ne_Std_ThrowInvalidBitsetPosition`): only the madrid option mask is indexed by track. No other fixed-size array indexed by track statId was found; UI track lists are built from DB/VM nodes (tracks.md#list). The invitational 8-track limit stays open (session.md#tracks-open).

<a id="track-flag"></a>
## Track intro flag: what selects the country (engine-re 2026-11-19)
Static on the Steam d873a1c8 exe plus offline reads of the stock archives (F1_TrackLab erpkern parsers, read-only). Nothing here is runtime-confirmed; runtime questions are T290-T293.

**Two flag objects.**
- **World object (the flag behind the grid).** Every stock `tracks/<t>/wep/<t>_common.erp` has an `AnimModel` instance `...$intro_flag_b` in its animations layer (`<t>-animations.layer` or `<t>-animations-objects.layer`), with model `intro_flag_b_anim.idf?context=render`, anim `intro_flag_b_animation01` (autoStart) and a literal `mat binding` string `eaid://environment_package/core_assets/objects/idf/intro/intro_flag_b_anim.emb?context=<ctx>`. The same archive ships that GfxMatBindRes (slots 01 = `intro_flag_b_<suffix>.material`, 02/03 = `intro_flag_a_pole.material`), the material and its diffuse texture `intro_flag_<suffix>_02_d.tif` (normal/surface maps `intro_flag_aus_02_n/_s`, pole and mesh come from `tracks/common_all_tracks.erp`, which also holds `?context=default` -> `intro_flag_b_aus`).
- **Cutscenes** `intro_flag` (`animation_package/scenes/cinematics/track_flag/track_flag.erp`, mesh `intro_flag_a_anim`) and `intro_flag_alt` (`track_flag_alt.erp`, mesh `intro_flag_b_anim`), spawned by `Game_IntroFlag_Initialise` 0x140351fe0 / `Game_IntroFlagAlt_Initialise` 0x1403526e0 at the placement `intro_flag`. Both archives carry MatBinds for 23 track contexts (the older tracks, no zandvoort, jeddah, imola, miami, las_vegas, losail, portimao) plus `default`, `invite` and `invite_night`.

**Context per stock track (world layer binding -> material).** abu_dhabi uae, austria aust, austria_reverse uses `austria`, bahrain bah, baku bak, brazil braz, catalunya spa, hungaroring hun, imola ita, jeddah jed, las_vegas usa, losail uses `qatar` -> qat, madrid uses `catalunya` -> spa, melbourne aus, mexico mex, miami usa, monaco mon, montreal can, monza ita, portimao por (plus a leftover `imola` instance), shanghai chin, silverstone eng, silverstone_reverse uses `silverstone`, singapore sin, spa_francorchamps bel, suzuka jap, texas usa, zandvoort zan, zandvoort_reverse uses `zandvoort`. The reverse layouts, losail and madrid show that the world binding is a free string: it is not the instanceName.

**What the code does.**
- No code builds a country suffix: the only exe strings with `intro_flag` are the scene names `intro_flag` / `intro_flag_alt`; `nationality` and `country_name` of tracks.Track are not read on this path. The suffix (bel) is data inside the MatBind resource.
- Cutscene context = the primary world name. `Sess_Loader_BuildTrackRegionList` 0x142d49050 copies the record's instanceName (record+0xc0 string, char * +0xd0) into the region request (+0xb0, char * +0xa8); the delegate `Sess_Loader_StartTrackWorldTask` 0x142d4d890 makes a `NeWorldStartSubTask` with that name (+0x90/+0xa0); `Ne_WorldLoadTask_Update` 0x14442a090 passes it to `Ne_WorldModule_StartPrimaryWorld` 0x14442a4f0, which stores it in the primary NeWorldInstance (+0x90/+0xa0, module+0xe8). The intro flag scripts read `[[[g_Game_SceneContext 0x14601f5c8]+0x30]+0xe8]+0xa0` (else `default`), spawnScene turns it into `.emb?context=<instanceName>` (`Game_SceneManager_FormatMaterialContexts_Body` 0x14741deb0), and `Game_Actor_LoadModelResources` 0x140187560 falls back to `.emb?context=default` (Australian flag) with ACTOR_WARN when that binding is missing.
- `Game_TrackModelComponent_Initialise` 0x142aa0330 rewrites the context of a track model to `default` (car/team case: SEASON/TRACK template), and the debug settings `Game.Context.overrideTrackName` / `Game.Context.TrackName` replace the context after `=` for every track model. A global switch, not per track.

**Why a same-named texture in the track archive does not win.** Resource names are global: `Ne_ResourceMap_FindOrCreateProxy` 0x143995160 keeps the first resource created for a name and skips the later bytes (section above, L2). There is no priority between global and track asset groups. A track archive that carries `intro_flag_bel_02_d.tif.image` with other pixels therefore loses whenever `track_flag_alt.erp` (or the real Spa common) was loaded first (load order: T291), and its MatBind `?context=spa_francorchamps` is the same name as the stock one.

**Own flag for a launcher track (data only, no code change, Spa untouched).** In the track archive:
1. Set the `mat binding` of the `$intro_flag_b` instance to `.../intro_flag_b_anim.emb?context=<track_code>`.
2. Add `GfxMatBindRes .../intro_flag_b_anim.emb?context=<track_code>` -> `intro_flag_b_<own>.material` + `intro_flag_a_pole.material`, the material (copy of `intro_flag_b_bel` with the diffuse path changed) and `GfxSRVResource` + `GfxSurfaceRes` `intro_flag_<own>_02_d.tif(.image)` with the flag pixels. `<own>` must not be a stock suffix (list above plus aust, fra, invite, invite_night).
3. Optional for the `intro_flag` cutscene: `intro_flag_a_anim.emb?context=<track_code>` -> own `intro_flag_a_<own>.material` / `intro_flag_<own>_01_d`. With launcher activation the instanceName is the track code, so the cutscenes find these names; without them they show the `default` (Australian) flag.
4. Leave out (or keep byte-identical) the copies of `intro_flag_b_bel`, `intro_flag_bel_02_d` and `?context=spa_francorchamps`.
This is the stock pattern (Zandvoort, Jeddah, Qatar, Portimao ship their own suffix only in their track archive). A launcher change is not needed; a launcher check could warn when a track archive carries stock flag names with other bytes.

## Inbox
- 2026-10-25 14:24 cs-heads: ERP fragment compression byte (decomp:re/decomp/143a6c700.c, decomp:re/decomp/143a6d4b0.c): the loader reads it as a signed byte and stores a negative one (0x90, 0x91) as 0, a plain copy; the decompressor thread treats codes 3 to 0x3f as zstd (0x20 to 0x2f with dictionary code - 0x20). Its zstd loop (0x143a6d4b0) calls ZSTD_decompressStream (0x1447b0a60) in 0x20000-byte chunks until the input position reaches the packed size and never checks the result. A fragment under a zstd code that holds no zstd frame returns 'Unknown frame descriptor' without consuming input (libzstd 1.5.7 run with the same loop), so the job spins on its NeErpRsDecompressor thread, the resource is never created and its ERP never finishes loading: the first rebuilt Classic Seasons heads of PR #323 stored 20 materials uncompressed under More Driver Models' code 0x10 and a Grand Prix load stayed on PLEASE WAIT. tools/erpw.py and packages/formats compress only code 0x11 on write.
- 2026-11-19 00:28 engine-re: 2026-11-19 engine-re track-flag round (static + offline ERP reads): the flag behind the grid is the world AnimModel $intro_flag_b of each track's animations layer with a literal binding intro_flag_b_anim.emb?context=<ctx> shipped in the same <t>_common.erp; the intro cutscenes use context = tracks.Track instanceName (primary world name) with fallback ?context=default (Australia); names are global, first loaded wins. Own flag for a launcher track = own MatBind/material/texture names in the track archive. Write-up: assets.md#track-flag. Tests T290-T293.
