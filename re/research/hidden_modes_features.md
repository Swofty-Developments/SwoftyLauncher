# Hidden game modes and features (F1 25, build d873a1c8)

Survey of game modes, editions and features that ship in `F1_25.exe` and `gamedatabases.2025.bin` but are not reachable from the normal menus. Agent `vmknolle-hidden`, 2026-11-13.

**Evidence level: offline only.** Everything below comes from the game DB (read with `tools/gamedb.py`, keys = fnv1a64 of the row path) and from static string cross-references in the exe (`re/tools/pe_index.py`). Nothing here was tested in game. "Hidden" means: defined in the data or code, but not listed by the edition or switched off. Whether a mode or feature still works when switched on is open in every case.

## 1. How the game decides what is available

Three layers, outermost first:

1. **Engine settings** (`system/config.xml`, `system/config_sku.xml`, loaded by `Ne_Load_SystemConfigXml` 0x1443c24b0). About 960 dotted names, read through `Ne_Get_SettingBool` 0x143a43b90, `Ne_Get_SettingU32` 0x143a43240, `Ne_Config_GetString` 0x143a44700 and related getters. One of them, `ContentSet.currentContentSet` (read by `Sess_Read_CurrentContentSetSetting` 0x1413e1cd0, default empty), names the edition (content set) to use.
2. **Content set** (`content_sets.content_set_N`). Lists the game modes the edition offers (`available_game_modes_0.game_modes`, `.game_mode_championships`, `.game_mode_time_trials`) and holds a feature-bit row (`feature_list_0`) that `Game_Features_LoadContentSetMask` 0x1413ed470 turns into a 128-bit mask. A set inherits from its `base_content_set`.
3. **Game mode rows** (`game_modes.ModeChampionship_N`, `game_modes.GameMode_N`). Each row has `available` (False in the DB; set at startup), `disabled`, `statId`, `game_setup_modes` and the per-mode options.

## 2. Game modes in the DB

29 `ModeChampionship` rows and 1 `GameMode` row (`benchmark`). *In sets* lists the content sets whose `game_mode_championships` names the mode. Bold rows are not offered by the retail-looking sets (2025, 2025_final_sub, 2026) or only by special editions.

| # | mode | statId | in sets | note |
|---|---|---|---|---|
| 0 | online_custom | 7 | all with modes except f1p, f1a | |
| 1 | event_mode | 0 | 2024, 2025, 2025_final_sub, 2026 | |
| 2 | career_invitationals | 11 | 2024, 2025, 2025_final_sub, 2026 | feature bit `invitationals` is False in every set |
| 3 | champ_invitationals | 12 | 2024, 2025, 2025_final_sub, 2026 | same |
| **4** | **dev** | 72 | every set with modes except f1p | developer championship; setup modes f1, f1_26, classic, f2, f1_generic; no front-end entry found |
| 5 | championships | 13 | 2024, 2025, 2025_final_sub, 2026 | type championship_event |
| 6 | online_weekly_events | 15 | 2024, 2025, 2025_final_sub, 2026 | |
| **7** | **online_championship** | 14 | **none** | defined, never offered |
| 8 | grand_prix_splitscreen | 6 | 2024, 2025, 2025_final_sub, 2026 | |
| 9 | story_mode | 17 | 2025, 2025_final_sub, 2026 | Braking Point |
| 10 | grand_prix_23 | 4 | all with modes except 2024_beta and f1a | the Grand Prix mode |
| **11** | **career_23** | 21 | **none** | F1 23 career row kept in the DB |
| **12** | **career_23_online** | 22 | **none** | same |
| 13-16 | driver_career_24, career_24_online, my_team_career_24, challenge_career_24 | 23-26 | 2024 only | previous-year careers |
| 17-20 | my_team_career_25, driver_career_25, career_25_online, challenge_career_25 | 27-30 | 2025, 2025_final_sub, 2026 | |
| **21** | **pro_dev** | 73 | **f1p** only | developer mode of the "f1p" edition |
| **22** | **pro_online_custom** | 74 | **f1p** only | online lobby of the "f1p" edition (setup modes include f1_elimination) |
| **23** | **story_mode_zenith** | 75 | 2025, 2026 (not 2025_final_sub) | Braking Point variant; feature bits `zenith_chapter_select`, `zenith_ti`, `zenith_only_preview` |
| 24 | rewind_event_mode | 31 | 2024, 2025, 2025_final_sub, 2026 | |
| **25** | **F1CG_solo_battles** | 77 | **none** | setup mode `solo_battle`; feature bit `f1cg` is False in every set |
| 26-27 | driver_career_26, my_team_career_26 | 78-79 | 2026 | season-2026 careers |
| **28** | **f1a_mp** | 80 | **f1a** only | F1 Academy multiplayer (setup mode `f1a_mp`) |
| - | benchmark (`GameMode_0`) | 127 | every set with modes except f1a | `-benchmark` command line (`Sess_Benchmark_InitFromCmdline` 0x1407a2250) |

Time-trial modes: `time_trial` in the normal sets, `solo_testing` only in `f1p`.

## 3. Content sets (editions)

feature_list columns: f1cg / iconic_drivers / invitationals / ir / story_mode (T = True, F = False).

| # | set | base | game modes offered | feature_list |
|---|---|---|---|---|
| 0 | 2024 | - | F1 24 list + dev | F T F F F |
| 1 | 2024_beta | 2024 | online_custom, dev | F T F F F |
| 2 | 2025_with_2024_vehicle_package_override | 2025 | (inherits) | (inherits) |
| 3 | 2025 | - | 2025 list incl. story_mode_zenith + dev | F T F F T |
| 4 | 2025_final_sub | 2025 | 2025 list without story_mode_zenith | F T F F T |
| 5 | 2025_launch | 2025 | (inherits) | F T F F T |
| **6** | **2025_movie** | 2025 | (inherits) | (inherits) |
| **7** | **2025_one_track** | 2025 | (inherits) | (inherits) |
| **8** | **f1p** | 2025 | benchmark; grand_prix_23, pro_dev, pro_online_custom; TT solo_testing | F F F F F |
| **9** | **2024_esports** | 2024 | (inherits) | (inherits) |
| 10 | 2024_beta_extra | 2024_beta | (inherits) | (inherits) |
| 11 | 2026 | - | 2025 list + driver_career_26, my_team_career_26 + dev | F T F F T |
| 12 | 2025_beta | 2025 | benchmark, ModeTimeTrial; online_custom, grand_prix_23, dev | F T F F F |
| 13 | 2025_beta_extra | 2025_beta | (inherits) | (inherits) |
| **14** | **vte** | 2025 | benchmark; online_custom, grand_prix_23, dev | F F F F F |
| **15** | **f1a** | 2026 | f1a_mp, dev | F T F F F |

Which set the retail game runs on was not traced. `ContentSet.currentContentSet` defaults to an empty string (Ne_Config_GetString call at 0x1413e1d42); the fallback path after that call was not followed.

## 4. Feature bits

`g_Game_FeatureNames` 0x1456e7300 names 65 bits (index = bit). The DB `feature_list` rows carry only five columns (`f1cg`, `iconic_drivers`, `invitationals`, `ir`, `story_mode`). No other bit has a column in any content set, and no set has `add_features` / `remove_features` rows, so `Game_Features_LoadContentSetMask` leaves the other 60 bits at their default.

| bit | name | bit | name | bit | name |
|---|---|---|---|---|---|
| 0 | story_mode | 22 | special_liveries | 44 | pds |
| 1 | story_team_select | 23 | ce | 45 | rsa |
| 2 | invitationals | 24 | mtc | 46 | f1w_compendium |
| 3 | f1_l | 25 | multiplayer_dotd | 47 | f1cg |
| 4 | f1_w | 26 | driver_icons_mk2 | 48 | DAISlider |
| 5 | sc_unlock_system | 27 | ti | 49 | fhr |
| 6 | new_ranked | 28 | subt_mk2 | 50 | ab |
| 7 | iconic_drivers | 29 | upd_rw_rp | 51 | session_strategy |
| 8 | support_exporting_session_results | 30 | vde | 52 | ydp |
| 9 | rewards_screen | 31 | vde_mirror_link | 53 | main_menu |
| 10 | post_race_licence_update | 32 | vde_mirror_link_same_projection_area | 54 | show_new_features_screen |
| 11 | fanzone | 33 | driver_number | 55 | f1p_pr |
| 12 | podium_pass_tier_purchases | 34 | manual_pit_stops | 56 | ir |
| 13 | multiple_loadouts | 35 | pro_pit_markers | 57 | f1p_autoadvance |
| 14 | rvr | 36 | eatrax_on | 58 | ftue_stay_in_touch |
| 15 | stepped_tutorials | 37 | tt_leaderboards_on | 59 | race_garage_start |
| 16 | silly_season_cutscene | 38 | show_rivals_tt | 60 | garage_spectating |
| 17 | show_customisable_item_icon | 39 | show_objectives_tt | 61 | zenith_only_preview |
| 18 | show_non_customisable_item_icon | 40 | pro_safety_rating | 62 | tt_ai_level_check |
| 19 | CC | 41 | tss | 63 | vte_sessions |
| 20 | driver_hq | 42 | zenith_chapter_select | 64 | challenge_events |
| 21 | allow_skip_first_season_first_practice | 43 | zenith_ti | | |

Bits that look like unreleased or edition-only features: `invitationals` (False everywhere, yet both invitational modes are listed), `f1cg` (goes with `F1CG_solo_battles`), `ir`, `manual_pit_stops`, `pro_pit_markers`, `pro_safety_rating`, `session_strategy`, `race_garage_start`, `garage_spectating`, `challenge_events`, `vte_sessions`, `f1p_pr`, `f1p_autoadvance` and the `zenith_*` bits. Which code tests each bit is not mapped yet (next step: xref the mask readers).

## 5. Developer settings

80 functions that read a gameplay-relevant setting and were missing from the KB are now L1 rows: `Game_Read_*Setting` when the function is small (< 0x180 bytes) and references one name, `Game_Unk_<addr>` otherwise; the notes list the settings. Plus `Ne_Load_SystemConfigXml` and `Sess_Read_CurrentContentSetSetting`. Highlights:

- **Grid:** `Grid.Override`, `Grid.CarsInRow`, `Grid.RowOffset`, `Grid.StaggerOffset`, `Grid.PoleLR` (`Game_Read_GridOverrideConfig` 0x142f61640, already in the KB).
- **Session shortcuts:** `Qualifying.full` / `.short` / `.one_shot`, `Game.SkipPreRace`, `Game.SkipIntroVideo`, `Game.SkipIntroCameras`, `Game.SkipGridRundown`, `Game.SkipWinnerSequence`, `Game.SkipRewardSequence`, `Game.IgnoreMinimumLoadingTime`.
- **Dev session** (`DevSessionDataContract.*`): track, layer, laps, race length, session length, entrant list, equal performance, staggered start, championship component.
- **Forced cutscenes:** `Game.ChampionshipWon.ForceOn`, `Game.DonutSequence.ForceOn` / `.Endless` / `.Vehicle`, `Game.DriverHQIntro.*.ForceOn`.
- **Photo mode / replay:** `PhotoMode.devEnabled`, `.canPositionCars`, `.allowAllWeatherChanges`, `.allowPhotoModeInPitlane`, `.cameraCollisionAvoidance`, `Replay.FullRaceReplay.Enabled`, `Replay.force_advanced_replay_ui`.
- **Debug camera / capture:** `DebugCamera.*` (speed, fov, shake, wobble), `VideoCapture.*`, `VideoExport.Enabled`, `EnvArtMode.enabled`.
- **Front end:** `UI.EnableAttract`, `UI.DemoEvent`, `UI.PreviewEvent`, `UI.MovieBuild`, `UI.skipDLCUpsell`, `UI.skipFirstTimeBoot`, `UI.overrideMainMenuState`, `UI.EnableCredits`, `UI.DisplayWatermark`.
- **Career:** `Game.NegotiationsAfterEveryWeekend`, `Game.driverSelectOverride`, `Game.teamSelectOverride`, `Game.vehicleSelectOverride`, `Game.teamPositionOverride`.
- **Online / LAN:** `Network.LAN.active`, `Network.Season.CollisionsFirstLapOnly`, `Network.Season.WeatherType`, `Network.Dev.*`.
- **Other:** `SafetyCar.noGhost`, `Testing.EnableSmokeTest`, `TimeManager.fixedTimeStep`, `Game.DRL.IgnoreCarSpeed`, `HudCustomisation.imguiEnabled`, `VTE.imgui`.

## 6. Open questions

1. Is `system/config.xml` read from a loose file, and can a Swofty overlay (`api_overlay_file`) replace it? (Untested.)
2. Which content set does the retail build select, and does setting `ContentSet.currentContentSet` to `vte`, `f1p` or `f1a` switch the edition?
3. Do `dev`, `online_championship`, `career_23` and `F1CG_solo_battles` still have working front-end flows, or only DB rows?
4. Which code reads each feature bit?

## Appendix: settings by group

Every name below is referenced from code (string xref). Function names come from the KB; `?` means no KB row.

### `Game`

| setting | read in |
|---|---|
| `Game.AlwaysVirtualiseSessionsIndividually` | Game_Unk_140bd3b30 `0x140bd3b30`, Game_Championship_VirtualiseSession `0x140bd4bc0` |
| `Game.AutoProfile.enabled` | Game_Read_GameConfig `0x1407f7320` |
| `Game.AutoProfile.framesToCapture` | Game_Read_GameConfig `0x1407f7320` |
| `Game.AutoProfile.framesToWait` | Game_Read_GameConfig `0x1407f7320` |
| `Game.Career.Online.EnableSaving` | Game_TOPLevelUI_TriggerProfileSave `0x140ff7cf0` |
| `Game.CareerSync.DumpComponents` | Game_CareerOnlineDocumentComp_InternalSerialiseAndCalculateHash `0x140fd0990` |
| `Game.CareerSync.DumpComponentsVerbose` | Game_CareerOnlineDocumentComp_InternalSerialiseAndCalculateHash `0x140fd0990` |
| `Game.ChampionshipWon.ForceOn` | Game_Unk_142d54f20 `0x142d54f20` |
| `Game.ClientHello` | no code reference found |
| `Game.Context.TeamLivery` | Game_CutSceneScript_RunCommand `0x14023c600`, Game_Unk_142a9d730 `0x142a9d730`, Game_PlayerTeamModelComponent_Initialise `0x142a9f390` |
| `Game.Context.TrackName` | Game_Unk_14020a460 `0x14020a460`, Game_GridCutSceneScript_SetupDriverTalkToPress `0x140345f60`, Game_Podium2019CutSceneScript_ChooseCutScenesToPlay `0x140356a20` +1 |
| `Game.Context.overrideEmote` | Game_Podium2019CutSceneScript_ChooseCutScenesToPlay `0x140356a20` |
| `Game.Context.overrideTeamLivery` | Game_CutSceneScript_RunCommand `0x14023c600`, Game_Unk_142a9d730 `0x142a9d730`, Game_PlayerTeamModelComponent_Initialise `0x142a9f390` |
| `Game.Context.overrideTrackName` | Game_Unk_14020a460 `0x14020a460`, Game_GridCutSceneScript_SetupDriverTalkToPress `0x140345f60`, Game_Podium2019CutSceneScript_ChooseCutScenesToPlay `0x140356a20` +1 |
| `Game.DRL.IgnoreCarSpeed` | Game_Unk_14243a840 `0x14243a840` |
| `Game.DecalGenerationService.MandatoryMipsDefault` | Game_DecalGeneration_CreateService `0x141428060` |
| `Game.DecalGenerationService.MandatoryMipsForLocalHumanPlayer` | Game_DecalGeneration_CreateService `0x141428060` |
| `Game.DonutSequence.Endless` | Sess_DoughnutSequence_Update `0x142d5a150` |
| `Game.DonutSequence.ForceOn` | Sess_DoughnutSequence_Enter `0x142d59920` |
| `Game.DonutSequence.Vehicle` | Sess_DoughnutSequence_Enter `0x142d59920` |
| `Game.DriverHQIntro.DriverHQIntro.ForceOn` | Game_Read_GameConfig_7 `0x1436c5ef0` |
| `Game.DriverHQIntro.DriverRatingsMilestone.ForceOn` | Game_Read_GameConfig_7 `0x1436c5ef0` |
| `Game.DriverHQIntro.DriverWonWorldChampionship.ForceOn` | Game_Read_GameConfig_7 `0x1436c5ef0` |
| `Game.DriverHQIntro.NewSeasonEstablishingCutscene.ForceOn` | Game_Read_GameConfig_7 `0x1436c5ef0` |
| `Game.EnableLoading` | Game_Read_GameConfig_6 `0x14365a8b0` |
| `Game.EnableMarshalLightShaders` | no code reference found |
| `Game.EnablePitExitLightShdaers` | no code reference found |
| `Game.EnablePitExitLights` | no code reference found |
| `Game.EnableRaceUI` | Sess_State_OnStateEnter `0x1436f6740` |
| `Game.EnableSaveGame` | Game_Read_GameConfig `0x1407f7320`, Game_Read_GameConfig_6 `0x14365a8b0` |
| `Game.EnableSaveSystemInSkipFE` | Game_Read_GameConfig_6 `0x14365a8b0` |
| `Game.EnableSaving` | Game_Read_GameConfig_6 `0x14365a8b0` |
| `Game.F1World.ForceFanzoneChoices` | Game_Unk_14170c8c0 `0x14170c8c0`, Game_Read_GameConfig_2 `0x1418cfc90`, Game_Read_GameConfig_3 `0x141b19e80` |
| `Game.F1World.Ranked.AutoUpdateCadence` | Game_Unk_142747e00 `0x142747e00` |
| `Game.F1World.SkipFanzoneChoices` | Game_Read_GameConfig_2 `0x1418cfc90`, Game_Read_GameConfig_3 `0x141b19e80` |
| `Game.F1World.SkipOnboarding` | Game_Read_GameF1WorldSkipOnboardingSetting `0x1427447d0` |
| `Game.ForceTrackReload` | Sess_Loader_UnloadSession `0x142d48040`, Game_Unk_142d4b4c0 `0x142d4b4c0`, Game_StateSession_OnStateExit `0x1436f8340` |
| `Game.GameSessionActivity` | no code reference found |
| `Game.IgnoreMinimumLoadingTime` | Sess_Loader_RequestTrackLoad `0x142da7870`, Game_Read_GameIgnoreMinimumLoadingTimeSetting `0x1436f9890`, Game_Unk_1436fb5e0 `0x1436fb5e0` |
| `Game.LoadForever` | no code reference found |
| `Game.NegotiationsAfterEveryWeekend` | Game_Unk_14126a1a0 `0x14126a1a0` |
| `Game.ReturnToGarageTimeLimit` | Game_Unk_1424ac650 `0x1424ac650` |
| `Game.SkipCamSequence` | no code reference found |
| `Game.SkipGridRundown` | Game_GridRundown_ShouldPlay `0x140ac6b30` |
| `Game.SkipIntroCameras` | no code reference found |
| `Game.SkipIntroVideo` | Game_Unk_142d6cc30 `0x142d6cc30` |
| `Game.SkipPreRace` | Game_Unk_142d78c80 `0x142d78c80`, Sess_PreRaceGridState_OnEnter `0x142d81270` |
| `Game.SkipRewardSequence` | Game_CVar_Ctor_SkipRewardSequence `0x142c747f0` |
| `Game.SkipTalkingPoints` | no code reference found |
| `Game.SkipWinnerSequence` | no code reference found |
| `Game.SummonToGameSessionActivity` | no code reference found |
| `Game.Thumbnails.driverPreloadTimeoutInSeconds` | Game_Read_GameConfig_4 `0x142e22c60` |
| `Game.Thumbnails.enableThumbnailGeneration` | Game_Read_GameConfig_4 `0x142e22c60` |
| `Game.Thumbnails.preloadTimeoutInSeconds` | Game_Read_GameConfig_4 `0x142e22c60` |
| `Game.Tutorials.SkipVideoTutorials` | no code reference found |
| `Game.Tyres.WarmUpSetsPerCompound` | Game_TyreAllocation_ValidateWarmUpSets `0x140bce940` |
| `Game.UseSMSSteer` | Car_Create_F1AssistsSystem `0x1447546c0` |
| `Game.VehicleDamage.tyres` | Game_Read_VehicleConfig `0x14463a800` |
| `Game.VehicleSpawning.EnableMemorySnapshots` | Game_Read_GameConfig_5 `0x142f3a5d0` |
| `Game.VehicleSpawning.MaxConcurrentSpawns` | Game_Read_GameConfig_5 `0x142f3a5d0` |
| `Game.VehicleSpawning.VehicleTimeout` | Game_Read_GameConfig_5 `0x142f3a5d0` |
| `Game.WeekendSessionManagerInFirstSession` | Sess_WeekendManager_PrepareTrackLoad `0x142409db0` |
| `Game.driverSelectOverride` | Game_Unk_14245a0d0 `0x14245a0d0` |
| `Game.ecaEnabled` | Game_Unk_14244fe70 `0x14244fe70` |
| `Game.equalPerformance` | Game_Read_GameEqualPerformanceSetting `0x1407e1d90` |
| `Game.forceIgnoreCareerDriverInTT` | Game_Unk_142933c70 `0x142933c70` |
| `Game.setup_mode` | Game_Unk_1407dfb60 `0x1407dfb60` |
| `Game.suppressWarnings` | Game_Read_GameConfig `0x1407f7320` |
| `Game.teamPositionOverride` | Game_Unk_142933c70 `0x142933c70` |
| `Game.teamSelectOverride` | Sess_Entrant_BuildCandidate `0x1424a15d0` |
| `Game.team_name` | Game_Unk_1407dfda0 `0x1407dfda0` |
| `Game.time_of_day` | Game_Unk_1407e1040 `0x1407e1040` |
| `Game.vehicleSelectOverride` | Sess_Collect_FilteredEntrants `0x1424a1c20` |
| `Game.weather` | Game_Unk_1407e1470 `0x1407e1470` |

### `UI`

| setting | read in |
|---|---|
| `UI.ChallengeCareer.shouldValidateTracks` | Game_Unk_141fc0740 `0x141fc0740` |
| `UI.DemoEvent` | no code reference found |
| `UI.DisableMessages` | Game_Unk_1442a3070 `0x1442a3070` |
| `UI.DisplayWatermark` | no code reference found |
| `UI.EnableAttract` | Game_Unk_1419f4b20 `0x1419f4b20`, Game_Unk_141e9d630 `0x141e9d630` |
| `UI.EnableCredits` | no code reference found |
| `UI.EnableMouseSupport` | Game_Read_UIConfig `0x1444a1b30` |
| `UI.ForceFanZoneSeasonResults` | Game_FanZoneSeasonResults_Exit `0x14238da40`, Game_Unk_145127550 `0x145127550`, Game_Unk_14536f23c `0x14536f23c` |
| `UI.IsTextProcessorEnabled` | Game_Read_UIConfig `0x1444a1b30` |
| `UI.L` | no code reference found |
| `UI.MovieBuild` | Game_Read_UIMovieBuildSetting `0x143034b20` |
| `UI.PreviewEvent` | Game_Read_AudioConfig `0x140a249f0`, Game_Unk_141758910 `0x141758910`, Game_Unk_141a10d50 `0x141a10d50` +5 |
| `UI.RoundSwipeAnimation` | Game_Unk_1437f47d0 `0x1437f47d0` |
| `UI.ShowInProgressRoute` | Game_Read_UIConfig `0x1444a1b30` |
| `UI.ShowLoadingTips` | Game_LoadingScreenVM_OnScreenActivated `0x141ca43e0`, Game_Unk_142bb74b0 `0x142bb74b0` |
| `UI.SkipFanZoneSeasonResults` | Game_FanZoneSeasonResults_Exit `0x14238da40` |
| `UI.TextEngine` | Game_Read_UIConfig `0x1444a1b30` |
| `UI.TextProcessorAffectsRTLLanguagesOnly` | Game_Unk_142e106f0 `0x142e106f0`, Game_Read_UIConfig `0x1444a1b30` |
| `UI.UnicodePlurals` | no code reference found |
| `UI.UseKeyboardOnlyIconFont` | Game_Read_GameVersionNumberConfig `0x1437d4130` |
| `UI.defaultTrack` | Game_Read_UIDefaultTrackSetting `0x1438520f0` |
| `UI.overrideMainMenuState` | Game_FEHomeViewModel_OnScreenActivated `0x141bb17f0` |
| `UI.skipDLCUpsell` | Game_Unk_141a25dd0 `0x141a25dd0`, Game_Unk_141a2d460 `0x141a2d460`, Game_Unk_141a5df70 `0x141a5df70` +2 |
| `UI.skipFEFullPlatformLoginFlowEnabled` | Game_GhostlapTrace_OnLeaderboardEntryRead `0x142fa53c0` |
| `UI.skipFirstTimeBoot` | Game_Unk_141bb7bf0 `0x141bb7bf0`, Game_SplashTransitionViewModel_OnScreenActivated `0x141e9b9f0`, FeFlow_Recalculate_TransitionState `0x1436d1500` |

### `Replay`

| setting | read in |
|---|---|
| `Replay.FullRaceReplay.Enabled` | Game_Unk_142c45070 `0x142c45070` |
| `Replay.disable_onboarding_tutorials` | Game_Unk_140f39ce0 `0x140f39ce0`, Game_Read_ReplayConfig `0x142c04a30` |
| `Replay.force_advanced_replay_ui` | Game_Read_ReplayConfig `0x142c04a30` |
| `Replay.reset_onboarding_flags` | Game_Read_ReplayConfig `0x142c04a30` |

### `PhotoMode`

| setting | read in |
|---|---|
| `PhotoMode.allow360VideosWithAnyReplayDuration` | Game_Read_PhotoModeConfig `0x141cea5d0` |
| `PhotoMode.allowAllWeatherChanges` | Game_Read_PhotoModeConfig `0x141cea5d0` |
| `PhotoMode.allowPhotoModeInPitlane` | Game_NetworkTransport_KickConnection `0x14406fa70` |
| `PhotoMode.cameraCollisionAvoidance` | no code reference found |
| `PhotoMode.cameraLockedToBoundingBoxAroundCar` | no code reference found |
| `PhotoMode.cameraLockedToHeightAboveTrack` | no code reference found |
| `PhotoMode.canPositionCars` | no code reference found |
| `PhotoMode.debugNum360VideoFramesToStopAfter` | Game_Unk_142bcd750 `0x142bcd750` |
| `PhotoMode.devEnabled` | Game_Read_PhotoModeConfig `0x141cea5d0` |
| `PhotoMode.showCaptureTypeMenuOptions` | Game_Read_PhotoModeConfig `0x141cea5d0` |
| `PhotoMode.showLogoPositionOffOption` | Game_Read_PhotoModeConfig `0x141cea5d0` |

### `DevSessionDataContract`

| setting | read in |
|---|---|
| `DevSessionDataContract.ChampionshipComponentEnabled` | Sess_DevSession_ReadChampionshipComponentEnabled `0x1409995c0`, Game_Unk_140affe60 `0x140affe60`, Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Entrants.entrant_list` | ? `0x1407a38e0`, Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.ForceStaggeredStart` | Sess_Build_AiSystem `0x1405ff750` |
| `DevSessionDataContract.Game.DefaultEntrantGarageOverrides` | Game_Read_DevSessionDataContractEntrantsConfig `0x141127eb0` |
| `DevSessionDataContract.Game.entrant_setup_mode` | Sess_PitStopUpdateJob_Update `0x1403c06a0`, Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Game.equalPerformance` | Game_Unk_140ab7720 `0x140ab7720`, Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Game.laps` | Asset_Load_DevSessionWorlds `0x141124b30` |
| `DevSessionDataContract.Game.race_length` | Asset_Load_DevSessionWorlds `0x141124b30` |
| `DevSessionDataContract.Game.sessionLengthInSeconds` | Asset_Load_DevSessionWorlds `0x141124b30` |
| `DevSessionDataContract.Game.session_id` | Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Game.setup_mode` | Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Track.clear_extra_worlds` | Asset_Load_DevSessionWorlds `0x141124b30` |
| `DevSessionDataContract.Track.layer` | Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Track.list` | Sess_DevSession_Apply `0x141125e80` |
| `DevSessionDataContract.Track.name` | Sess_DevSession_Apply `0x141125e80`, Game_Read_DevSessionDataContractTrackNameSetting `0x143853ef0` |
| `DevSessionDataContract.Track.worldAssetGroup` | Sess_DevSession_Apply `0x141125e80` |

### `ContentSet`

| setting | read in |
|---|---|
| `ContentSet.currentContentSet` | Sess_Read_CurrentContentSetSetting `0x1413e1cd0` |

### `Grid`

| setting | read in |
|---|---|
| `Grid.CarsInRow` | Game_Read_GridConfig `0x142f62120` |
| `Grid.Override` | Game_Read_GridOverrideConfig `0x142f61640` |
| `Grid.PoleLR` | Game_Read_GridConfig `0x142f62120` |
| `Grid.RowOffset` | Game_Read_GridConfig `0x142f62120` |
| `Grid.StaggerOffset` | Game_Read_GridConfig `0x142f62120` |

### `Qualifying`

| setting | read in |
|---|---|
| `Qualifying.full` | Game_Read_QualifyingConfig `0x142e44b50` |
| `Qualifying.one_shot` | Game_Read_QualifyingConfig `0x142e44b50` |
| `Qualifying.short` | Game_Read_QualifyingConfig `0x142e44b50` |

### `VTE`

| setting | read in |
|---|---|
| `VTE.Sessions.override` | Game_Read_VTEConfig `0x142f7f7b0` |
| `VTE.enabled` | Game_Read_VteEnabled `0x1407972c0`, Game_Read_GameConfig `0x1407f7320`, Game_Read_GridOverrideConfig `0x142f61640` +2 |
| `VTE.imgui` | Game_Read_VTEConfig `0x142f7f7b0` |

### `Testing`

| setting | read in |
|---|---|
| `Testing.EnableSmokeTest` | Render_Gfx_BuildConfigObject `0x140104f70`, Game_ModeChampionship_Build `0x1407d2820`, Game_Unk_1407fd8d0 `0x1407fd8d0` +7 |
| `Testing.OverrideGraphicsPreset` | Game_Application_InitialiseHardwareSettings `0x1443c4fa0` |

### `Story`

| setting | read in |
|---|---|
| `Story.disableCinematicCinemascope` | no code reference found |
| `Story.forcedTitleUpdateValue` | Game_Read_StoryForcedTitleUpdateValueSetting `0x141869690` |

### `SafetyCar`

| setting | read in |
|---|---|
| `SafetyCar.noGhost` | Game_Unk_142f1b700 `0x142f1b700` |

### `DebugCamera`

| setting | read in |
|---|---|
| `DebugCamera.enableDpadCommands` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `DebugCamera.fov` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `DebugCamera.invertVerticalLook` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `DebugCamera.quickSpeedMultiplier` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `DebugCamera.shake.isActive` | no code reference found |
| `DebugCamera.shake.positionLimitsInMetres.x` | no code reference found |
| `DebugCamera.shake.positionLimitsInMetres.y` | no code reference found |
| `DebugCamera.shake.positionLimitsInMetres.z` | no code reference found |
| `DebugCamera.speedMultiplier` | Game_Read_VideoCaptureConfig `0x140ad22a0`, Game_Unk_1446690f0 `0x1446690f0` |
| `DebugCamera.wobble.isActive` | no code reference found |
| `DebugCamera.wobble.overallSpeedMultipler` | Render_Gfx_CreateTrackConstantsAndPuddleBuffers `0x1447889a0` |
| `DebugCamera.wobble.rotationLimitsInDegrees.x` | ? `0x144e81953` |
| `DebugCamera.wobble.rotationLimitsInDegrees.y` | no code reference found |
| `DebugCamera.wobble.rotationLimitsInDegrees.z` | no code reference found |
| `DebugCamera.wobble.rotationSpeedMultipliers.x` | no code reference found |
| `DebugCamera.wobble.rotationSpeedMultipliers.y` | no code reference found |
| `DebugCamera.wobble.rotationSpeedMultipliers.z` | no code reference found |

### `DebugOSD`

| setting | read in |
|---|---|
| `DebugOSD.DisplayFPSonSpeedo` | Game_Unk_141d5d910 `0x141d5d910` |
| `DebugOSD.DisplayResOnERS` | no code reference found |
| `DebugOSD.GameInputModule.showInputProviders` | Game_Read_LeadPlayerConfig_2 `0x142a733f0` |

### `VideoCapture`

| setting | read in |
|---|---|
| `VideoCapture.enabled` | Game_Read_VideoCaptureConfig `0x140ad22a0`, Ne_InputSystem_Init `0x14442f0c0`, Game_Unk_1445d54e0 `0x1445d54e0` |
| `VideoCapture.farMax` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.farMin` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.fovMax` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.fovMin` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.nearMax` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.nearMin` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.rotationSpeedMax` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.rotationSpeedMin` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.translationSpeedMax` | Game_Read_VideoCaptureConfig `0x140ad22a0` |
| `VideoCapture.translationSpeedMin` | Game_Read_VideoCaptureConfig `0x140ad22a0` |

### `VideoExport`

| setting | read in |
|---|---|
| `VideoExport.Enabled` | Game_Unk_1444b1130 `0x1444b1130` |

### `EnvArtMode`

| setting | read in |
|---|---|
| `EnvArtMode.enabled` | Game_Unk_142d80950 `0x142d80950`, Game_Unk_1436e5850 `0x1436e5850` |

### `HudCustomisation`

| setting | read in |
|---|---|
| `HudCustomisation.disableLegacyScript` | Game_Read_GameVersionNumberConfig `0x1437d4130`, Game_Unk_143808570 `0x143808570` |
| `HudCustomisation.imguiEnabled` | Game_Unk_1438072f0 `0x1438072f0` |

### `Career`

| setting | read in |
|---|---|
| `Career.Online.KickFailingSynchronisation` | Game_Unk_140fcd6b0 `0x140fcd6b0` |

### `TimeManager`

| setting | read in |
|---|---|
| `TimeManager.cap` | Game_Read_TimeManagerConfig `0x1443d1bc0` |
| `TimeManager.capValue` | Game_Read_TimeManagerConfig `0x1443d1bc0` |
| `TimeManager.fixedTimeStep` | Game_Read_TimeManagerConfig `0x1443d1bc0`, Game_Read_TimeManagerFixedTimeStepSetting `0x1443da4d0` |
| `TimeManager.mode` | Game_Read_TimeManagerConfig `0x1443d1bc0` |
| `TimeManager.smoothing` | Game_Read_TimeManagerConfig `0x1443d1bc0` |

### `Network.LAN`

| setting | read in |
|---|---|
| `Network.LAN.active` | Game_Unk_14172c5a0 `0x14172c5a0`, Game_Unk_14172c840 `0x14172c840` |

### `Network.Season`

| setting | read in |
|---|---|
| `Network.Season.AIDifficulty` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.AIDrivers` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.AISkillRatingMax` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.AISkillRatingMin` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.AbsAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.BrakingAssistsAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.CanReturnToGarage` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.CarPerformanceType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.CarSetupOptions` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.CollisionsFirstLapOnly` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_1433002e0 `0x1433002e0` |
| `Network.Season.CollisionsOn` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.ContentDomain` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.Enabled` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.FormationLap` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.FormationLapType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.GearBoxAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.Ghosting` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.ParcFermeRulesOn` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.PitAssistAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.PracticeType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.QualifyingType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.RaceLength` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.RaceStarts` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.RaceStartsAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.RaceType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.RacingLineAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.SafetyCarFrequency` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.SafetyCarType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.Scenario` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.StartingGridType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.TimeOfDay` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.TimeOfDayType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.Track` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.TractionControlAllowed` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.TyreCarcass` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.VehicleDamageType` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.Versus` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Season.WeatherType` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_144f3cd2b `0x144f3cd2b` |
| `Network.Season.ersModeAllowed` | Game_Read_NetworkConfig `0x143040da0` |

### `Network.Dev`

| setting | read in |
|---|---|
| `Network.Dev.AllowMultiplayerRestart` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.DepartedTimeOut` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.FailSteamAuthentication` | Game_Unk_143369930 `0x143369930` |
| `Network.Dev.ForceCrashOnGameModeDeselectionDuringMultiplayerSession` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ForceDirectP2P` | no code reference found |
| `Network.Dev.ForceLOD` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ForceSpectate` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.LeaveMultiplayerOnGameModeDeselection` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.PacketCapture.AutoPacketCapture` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.PacketCapture.SendToElasticSearch` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.PacketCapture.SendToLogFileServer` | Game_Read_NetworkConfig `0x143040da0`, Game_Read_NetworkDevPacketCaptureSendToLogFileServerSetting `0x1449fcee0` |
| `Network.Dev.PitEveryLaps` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.RandomisePlayerCars` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.Robodriver` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowAuthorityManager` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowBandwidthGraph` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowBandwidthStats` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowConnectionMap` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowConnectionQuality` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowContentDomainVerification` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowFontHeight` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowHopperMatching` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowPlayerSet` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressAnalysis` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressBandwidth` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressCalculator` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressClock` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressHistory` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressInfo` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressLogger` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressObservers` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowProgressTrackspace` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowReadyMonitor` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSafetyRating` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSavedChampionshipState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionAttribs` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionDrivers` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionFormationLap` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionMigration` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_144dfbfe0 `0x144dfbfe0` |
| `Network.Dev.ShowSessionNextTrackVoting` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionPlayers` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionRaceInfo` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionRaceStatus` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionSeason` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionStartLights` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionStats` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionTeams` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSessionVoting` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowSkillRatingSimulator` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehicleControllers` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehicleControlsState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehicleGhostingState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehicleLaptimes` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_14532c7e8 `0x14532c7e8` |
| `Network.Dev.ShowVehicleMechanicalState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehiclePenaltyState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehiclePitState` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowVehicleProgressState` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_143cfc0d0 `0x143cfc0d0` |
| `Network.Dev.ShowXpos` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.ShowYpos` | Game_Read_NetworkConfig `0x143040da0` |
| `Network.Dev.TestVehicleDelta` | Game_Read_NetworkConfig `0x143040da0`, Game_Unk_14437a6e0 `0x14437a6e0` |
| `Network.Dev.TraceLogging` | Game_Read_NetworkConfig `0x143040da0` |
