# tracks: track select screens, track lists and TRACK_DATA records

Owner: trk-t5 (2026-10-30). Static work for the tracks system release (`work/std16/TRACKS-PLAN.md` R1-R8). The `Game_TrackData_*` rows stay in the `game` lane; this lane holds the front-end track select view models, the record store and the set query.

## select

The track select screens are four view models, each found by its type record (slot 0 returns it; the VM type table inits 0x14217f380 and 0x141e93610 write the names) and each confirming through `onCommandRaised` (slot 10, djb2 command hash at `**cmd`):

| screen | vtable | slot 10 | confirm command |
|---|---|---|---|
| Grand Prix single race and championship (`home_solo.ui` solo_track_select, rounds list) | `vt_Trk_GrandPrixTrackSelectVm` 0x14573b950 (HomeSoloTrackSelectViewModel) | `Trk_GrandPrixTrackSelectVm_OnCommandRaised` 0x141c06dc0 | advance_pressed 0xfc8f026c |
| Time Trial (`track_select.ui`, carousel) | `vt_Trk_TimeTrialTrackSelectVm` 0x14575ce70 (TrackSelectViewModel) | `Trk_TimeTrialTrackSelectVm_OnCommandRaised` 0x141f28410 | select_pressed 0x2b0357a |
| championship settings track tab | `vt_Trk_SettingsSelectTrackTabVm` 0x14574f430 | 0x141de93b0 | back_pressed saves the championship |
| career calendar wizard | `vt_Trk_CareerWizardTrackSelectVm` 0x1457280f0 (L1) | 0x141a325e0 | advance_pressed |

Grand Prix confirm (0x141c06dc0, advance_pressed): returns at once unless the can-advance byte vm+0x93b is set; sets vm+0x93a; with the game mode entity `[[vm+0x648]+0x88]+0x260` it finds component desc 0x14605ad40 in the entity's +0x1030 registry and refreshes its +0x298; commits the rounds with 0x141c068f0(vm, 0x141c09900); writes `[vm+0x678]+0x8b`, calls 0x1424b3640(`[vm+0x678]`), fires route `open` to `[vm+0x678]+0xf8` through the router vm+0x620 (vt+0x80), then the entity's vt+0xe8. Nothing before the vm+0x93b test has side effects, so a wrapper that returns without calling the original for this hash refuses the confirm cleanly (the screen stays as it was).

Time Trial confirm (0x141f28410, select_pressed): returns unless `[vm+0x2c0]+0x130` and `+0x134` are 0 or 2 and vm+0x301 is set; then sets the route `t_next_time_trial` in vm+0x308, posts bus message 0x145753b20, sets `[vm+0x278]+0x4d0`, requests a fade and fires `fade_off_screen`; fade_off_complete 0x2dea0627 then fires the stored route. Refusing select_pressed in a wrapper leaves no state behind. selected_track_changed 0xdcd54bcb only sets vm+0x300.

Where the selected track lives: both screens bind VM nodes, not a stat id field. `Game_BaseGameModeSetupDataContract_UpdateSelectedTrackContextual` 0x143852b40 reads the selected track's name from the contract's VM node (+0x30, child 0x145735e19), finds the `TrackDataRecord` whose name (+0xd0) equals it in TrackDataSystem +0x28..+0x30 (null items skipped), and calls `Game_TrackData_GetTrack` with the contract's own 6-byte query at contract+0x70 (mode, used, season, used, format, used; it logs `UI_TRACK: %s: Track %s, GameSetUpMode: %d, SeasonStatId: %d, WeekendFormat: %s`). Its callers are the game mode setup steps 0x1407b5330 and 0x1407b7b90, each right after 0x143852230. The hook reads the name, so the guard maps name -> stat id through the record list.

## list

The lists every screen shows are VM nodes. `Game_UITrack_TrackFoundGameModeTracklistWill` 0x143852230 resolves a track against the game mode's track list (`<mode node>.track_list`, from `track_info.gm_track_list`), falls back to `seasons.current_season.track_list`, then to `track_data.track_list`. `Trk_TrackData_LoadTrackListNodes` 0x142e58f20 (run by Sess_Apply_ContentAvailability at 0x1413e4e15) builds `track_data.track_list` from tracks.Track rows, `track_data.restricted_tracks` and `schedule.all_track_events`. `Trk_CustomTrackListComponent_Create` 0x140d12e50 makes a mode's `track_info.gm_track_list` and points `track_info.track_list` at it. The TT view model's slot 13 0x141f276b0 and the GP view model's slot 10 (`#track_data.track_list`, `tracks_deserialised`) read `track_data.track_list`; the settings tab's slot 19 0x141de57c0 builds `rounds`, `track_list`, `track_contextual_data_list` and `filter_list`.

R2 status: the per-screen list node is built from these VM nodes, so route (a) (narrow the data the list node is built from) is a VM-node edit, not a content set vector edit; route (b) needs the element builder, not yet found (candidates: GP slot 19 0x141c03e90, TT slot 13 0x141f276b0). Both stay L1 until a trace counts the list per build (T-request in re/tests).

## query

`Trk_ChampionshipEntity_TrackDataQuery_Body` (.data1 0x1485962f0, thunk 0x14079f290, championship entity vt+0xc8) builds the session's TRACK_DATA query: mode = `[entity+0x2b0]+0x80` (the game setup mode stat id) when +0x2b0 is set; season = the u16 at `[entity+0xe18]+0xb0`, the selected entrant list's season with its own used flag (the era picker's setup +0xe18 is the same selected entrant list); format from entity vt+0xd0. `Game_TrackData_GetTrack` takes the first set in database row order whose masks hold the bits and asserts (0x1411c04f0) on a mode or season id >= 32. So an entrant list whose +0xb0 used byte is 0 matches on the mode bit alone, and the first set listing that mode wins: for `swofty_f1` (stat 20) that is the first set whose game_setup_modes include it, which is why a box's session can read a set no Classic Seasons row names. `site_Trk_TrackData_SetMatched` 0x142e60eb1 is the trace site that settles it (R3).

## stat-ids

`Trk_TrackData_AddRecord` 0x142e57740 stores each tracks.Track record at items[statId] of TrackDataSystem +0x20: it pads with zeroed items up to the stat id, appends at size == statId, fills an empty in-range item, and refuses an occupied one. So the record vector tolerates holes, and the two readers read here skip null items: `Game_TrackData_GetTrack` (bounds-checks statId against each set's item count and falls back to the set's first non-null track, so a stat id beyond a set's items reads another track's data without crashing) and 0x143852b40's name search. Not yet checked: every other walk of TrackDataSystem +0x28 and of each set's items (R8 stays partial until those are listed).

## fields

TrackData reader 0x142e44b50, writes into the 0x290 `TrackDataTrack` (call return address in brackets, the store follows it): number_of_laps +0xe0 stores the `Gdb_Get_FieldInt` value slot pointer, not the value (0x142e46e8c: `mov [rsi], rax`), so laps are read from the database value at use and a database value write changes them; fuel_burn_per_lap +0xf0 (known); weekend_month +0xf4 (0x142e47008); weekend_day +0xf8 (0x142e47035); use_split_qualifying +0xfc (0x142e470ea); number_of_intermediate_tyres +0xd0 (0x142e46d30); number_of_wet_tyres +0xd4 (0x142e46d5d); enforce_pitstops_in_wet_weather +0xd8 (0x142e46d87); chance_of_using_softer_preset +0x60, harder +0x64 (0x142e4633d, 0x142e46369); tyre presets' stat ids +0x68..+0x80; practice/qualifying/race crowd percentage +0x210/+0x214/+0x218; session start times p1..race3 +0x1d0..+0x208; pitSpeedLimit +0x21c (0x142e47817); pitStopTime_Dry +0x220 (0x142e4784e); pitStopTime_Wet +0x224 (0x142e4787a). L1 until a reader of each is listed (R4); laps needs the reader of +0xe0.

## ai

Leads only (R6 open): `Sess_Load_VehicleAiConfig` 0x1425027e0 reads `asset_filename` (0x142502f31); `Game_TrackDataSystem_Init` 0x142e5e010 (0x142e5e6fa) and `Game_Read_VehiclePackageRow` 0x142504a90 read `ai_package_folder`. The string `environment_package/tracks/<track_name>/ai/<vehicle_package>/<team_name>/` (0x1456349da) is only copied as text by 0x140610940.

## guard

The guard (R1, behaviour hook after T-tests and an agree row): wrap the two slot-10 handlers; for advance_pressed (GP) and select_pressed (TT) look up the selected track, ask the core's decision function, and on a refusal show the popup and return 0 without calling the original. Decision function proposed for TRACKS-CONTRACTS.md: `runtime::tracks::Decision runtime::tracks::guard(std::string_view track_code, uint8_t stat_id, std::string_view box_entrant_list, bool time_trial)` with `Decision { bool ok; std::string title, body; }`.

## Inbox
- 2026-10-30 00:52 trk-v1: trk-v1 verify (V1625-V1637): the guard must detour the handler entries (func_start sigs), not swap vtable slot 10: the GP handler is also reached through adjustor thunk 0x141c083b0 (add rcx,-0x268; jmp 0x141c06dc0) at slot 4 of the +0x268 base vtable 0x14573ba60, and the settings tab's through 0x141dead50 in 0x14574f5c0; TT has no second route. Vtable sizes are wrong in globals.tsv: GP primary is vtable[28] (+0x260 base 0x14573ba30), TT vtable[16] (+0x260 base 0x14575cef0, so 'slot 17 0x141f27210' is that base's slot 1), settings tab vtable[44] (+0x260 base 0x14574f590) and its type name is written by 0x141e93610, not 0x14217f380. Trk_TrackData_AddRecord returns the stored record pointer (0 when occupied), not -1. Refusal before both confirm tests leaves no state (agree). 0x143852b40 agree; it can go to L2.
