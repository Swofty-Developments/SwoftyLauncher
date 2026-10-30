# F1 1990 native DLL

`f1_1990.dll` signs the first 1990 Driver Career contract without the negotiation scene and sets the race grid from each driver's best lap in either 1990 qualifying session (Friday: SprintShootout group, 2; Saturday: Qualifying group, 1). Every hook acts on 1990 content only: the grid and tyre hooks on the 1990 weekend structure (shootout and qualifying groups, no `race_2`), which only the `f1_1990` setup mode has, and the contract skip on a career whose data group is `f1_career_1990`, so 2025 and 2026 careers and sessions in the same profile stay stock. 1990 careers are saved in the normal profile save, in their own slot list (data group `f1_career_1990`).

## Grid hooks

| binding | address | use |
|---|---|---|
| `Sess_Build_GroupResult` | 0x140afa220 | wrap; after a group's rows are built (last session of the group, or a save load) |
| `Sess_Record_GroupTimes` | 0x140af2880 | wrap; after a group's per-driver best laps are recorded (every qualifying-type session, driven or simulated) |
| `Sess_Find_GroupRowPosition` | 0x140af8aa0 | call; driver key to the group-1 row position |

After either wrap the group-1 row positions (`row+0x50`) are rewritten: drivers with a time are ranked by min(Friday, Saturday); drivers without one follow in Saturday order. The race grid (`grid_type` qualifying) reads those positions. A weekend is treated as 1990 when it has both a SprintShootout and a Qualifying group session and no `race_2` session; other weekends are untouched.

Log lines (swofty log): `[f1-1990] GRID best-of (<why>)` once per pass, then one `[f1-1990] GRID driver <kind>/<stat id> fri <s> sat <s> best <s>: position <old> -> <new>` per driver.

## Build

`./build.sh` builds the 17 DLLs with zig against the game headers in `native/include` and the SDK in `packages/sdk/include`. `f1_1990_parts.dll` binds `site_Sess_PuPanel_SlotHolder`, which has no `agree` verify row yet, so the core's `bindings.json` doesn't list it and the core drops that DLL; it loads with a loader whose `swofty/addons/core/bindings.json` adds the entry.

`test/run.sh` runs the ranking unit tests on the host.

## Contract skip

`src/contract_skip.cpp` signs the player's first 1990 Driver Career contract and passes the negotiation scene (native request N1). It runs only for a career whose data group is `f1_career_1990` (read like the save-slot predicates 0x1438fb390/0x1438fb5d0: each championship's CareerSetupComponent data-group index into its group list), with one human player.

| binding | address | use |
|---|---|---|
| `Career_Decide_ContractNegotiation` | 0x142d57d10 | wrap; on link 0x5e3c3c2d with `Career_Needs_DriverContract`, build the contract (current team, type 0, 1 season), rebuild it at the recommended target and sign it |
| `Career_Build_DriverContract` | 0x140d79dd0 | call |
| `Career_Sign_DriverContract` | 0x140d79ff0 | call (`replaced_key` 0x3fff0000, notify 1) |
| `Career_Needs_DriverContract` | 0x140d83090 | call |
| `Career_ContractIntro_OnEnter` / `_OnUpdate` | 0x1436c2620 / 0x1436c2dc0 | wrap; enter skipped, update returns the continue link |
| `Career_ContractUpdate_OnEnter` / `_OnUpdate` / `_OnExit` | 0x1436c1760 / 0x1436c1e00 / 0x1436c1fc0 | wrap; enter idles without the screen, update continues, exit returns 1 without closing a screen that never opened |
| `Career_ContractOutro_OnEnter` / `_OnUpdate` | 0x1436c3330 / 0x1436c3750 | wrap; enter skipped, update returns the continue link |
| `g_Career_FlowLinkContinue` | 0x146cb2284 | data (continue link, 0x7c9b1ec4 in logs) |
| `k_Career_DriverCareerContractsManagerComponentDesc` | 0x14605c0d8 | data (existing binding) |

The recommended target is the screen's: rating + the matching rating range's offset, inside the slider and at most 100 (`src/contract_target.hpp`, tested by `test/run.sh`).

The career-lane bindings were raised to L2 with `sitereg.py --force` (lane owner career-re) with the developer's approval on 2026-10-21; they still need an `agree` verify row from another agent.

`F1990_ContractSceneSkipped` (exported) returns 1 from the decision wrap's sign until the Outro passes, for `f1_1990_intro.dll`.

## Friday qualifying tyres (`f1_1990_tyres.dll`)

A separate DLL, so a binding it can't resolve drops only this hook and never the save separation. `Sess_TyreAllocation_OnSessionStart` (0x140bcc190, the tyre allocation component's session-start virtual) is wrapped: after it runs for a sprint-shootout session (ids 10..14) on a 1990 weekend, every unused, undamaged dry set (softer, medium, harder; no wear, not used) of every car gets its handed-back flag (set +0x09 bit 3) cleared and its session tag (+0x00) set to 0, and the allocation is marked dirty (+0xe8). A tag-0 set passes the game's "usable now" check (0x142e88630/0x142e88730) in every session, which otherwise has no case for shootout sessions and leaves only inters and wets. Friday and Saturday qualifying then share the unused dry sets. It logs `[f1-1990] FQ tyres: session <id>: <n> unused dry set(s) made usable on <cars> car allocation(s)`.

Option `fq_tyre_fix` (toggle, default on) turns it off. With `no_tyre_allocation` the allocation is off and the usable check passes anyway; the hook then changes nothing that matters.

### Tyre fallback

Same DLL. In a 26-car 1990 session some cars, the player's included, found no compound to fit and ran without tyres: the game logged `Unable to get Closest Available Compound for vehicle <n>.` from `Sess_Tyres_ClosestAvailableCompound` (0x14368fb40, a jmp thunk into the .data1 body 0x150dc4ad0, bound through its call at 0x142a3ba4c). The lookup takes the car's tyre component from the car list (`*(rules+0xd8)`, slot `+0x28[vehicle]`, entity `+0x10`, through `Ne_Entity_FindComponent` with `k_Sess_TyreComponentType` when that type's parent chain reaches `k_Ne_TypeRoot`), reads its allocation at `+0x9c68` and tries the five compound slots in preference order; a slot passes with a set the least-worn picker accepts (usable now, see `g_Sess_TyreRulesActive`, `g_Sess_TyreRulesMode`, `g_Sess_TyreRulesSessionType`) whose highest wheel wear is below max(1, the rules' limit). The game stores an allocation in a tyre component only while the component holds none, so a car can keep an allocation other than its driver's in the weekend list. In the failing runs every list allocation held 23 fresh sets.

- After `Sess_TyreAllocation_OnSessionStart` on a 1990 weekend: every car allocation is counted, an allocation with no sets gets a copy of the fullest one's sets (new sets: used flags cleared, wear 0, session tag and handed-back flag kept), and every session entrant's tyre component is pointed at its driver's list allocation when it holds another one.
- When the lookup returns 0xffffffff in a 1990 session, the car's tyre component is found the lookup's way and repaired step by step, the lookup running again after each step that changed something: point it at the driver's list allocation; refill it when empty; clear the wear of fresh sets the lookup rejects on wear (NaN or not below 1); drop the session tag of fresh sets. If the lookup still fails, the first slot in the car's preference order holding a fresh set is returned.

Lines: `[f1-1990] tyres: session <id>: <n> car allocation(s) for <n> car(s), <n> set(s) per car at least, <n> at most; <n> empty, <n> car(s) without one, <n> tyre component(s) not found, <n> reassigned`; `... V<nn> (driver <key>) held allocation <key|unlisted address|none>; now <key>`; `... allocation <key> (preset <n>, <why>) had no sets: <n> set(s) copied from <key> (<n> set(s))`; `... V<nn> (driver <key>) has no weekend allocation`; and once per car and session `... V<nn> (driver <key>, entrant) found no compound in allocation <held> (weekend list <key>): <sets per slot, fresh, tagged, worn>; <human|AI>, flags +0x48 <xx> +0x4b <xx>, vehicle id <n>; preference <5 slots>, wear scale <f>, limit <f>; rules on <n> mode <n> session <n>; <steps taken>; result: [forced slot ]<slot or -1>`. Keys print as the game's `%04x` driver key. Option `tyre_fallback` (toggle, default on) turns it off.

### Tyre class (`f1_1990_tyreclass.dll`)

A separate DLL, so a binding it can't resolve drops only this hook. A car's tyre component holds a compound class (`+0x9bc0`, a pointer into the class array at `*(classes+0xf0)`..`+0xf8`, 0x1f0 apart and indexed by stat id) from the car's `compound class` attribute. TyreComponent init (0x142ea08f0) gets it from `Sess_TyreClass_FromName` (0x142e97cb0, a jmp thunk into the .data1 body 0x14f44ad60, bound through its only call at 0x142ea0fe4) and builds three tyre models per entry of the class's visual compound list (`+0x170`, {visual compound, physical mask}). The garage tyre picker (0x141faa240), the HUD, MFD, pause and timing tower tyre widgets map a set to a visual compound through the class (per-car slot table `+0x9bd0`, built by 0x142e9e6c0 / 0x142e9e540 from the class's per-option physical and visual masks `+0x1a8` and `+0x1bc`). The 1990 cars, built on the 2026 asset groups, take `f1_26`.

`Sess_TyreClass_FromName` is wrapped: while a 1990 career is loaded, a result whose five per-option physical compound masks (`+0x1a8`) equal those of `f1_1990` (its stat id read from `tyres.compound_classes`) becomes `f1_1990`. Session, garage and showroom cars of the 1990 career all name `f1` (class 3). The component builds three models per entry of the class's visual compound list (`+0x170`, at most 11 fit before its slot count at `+0x98d0`) and finds a visual compound's slot through the class's map (`+0x1d8`, filled by 0x142e9a7d0 from that list, 0xff elsewhere). A showroom car shows the class's frontend compound (`+0x10` dry, `+0x18` wet): 0x14d42ede0 passes it to 0x14f4935c0, which takes the slot at `+0x30 + map[visual] * 0xde0`. With `f1_26`'s frontend compound `Hard_2018`, which `f1_1990` doesn't list, the slot was 0xff, and `setWingVisibilityOnCar` (0x14f3c6a80) entered a critical section past the models (team select, 2026-10-30: ACCESS_VIOLATION reading 0x40000000520 in ntdll). The swap now needs both frontend compounds of `f1_1990` in its visual list and a list of at most 11; `db/rules.json` sets them to `Soft_f1_1990` and `Wet_f1_26`. The physical compound of every option stays the same, so the tyre physics of the car's `TyrePhysicsGroup` do not change; the visual compounds, and with them the badges and the tyre models' visual compounds, are the 1990 ones. Lines: `[f1-1990] tyre class: 1990 career cars on compound class <n> take f1_1990 (class <n>)` (or `... keep it: other physical compounds than f1_1990`), once per class and career; `[f1-1990] tyre class: f1_1990 (class <n>) kept off every car: <why>` once when the class fails the check. Option `tyre_class_fix` (toggle, default on) turns it off.

At the start of a 1990 race (session ids 15 to 17), `f1_1990_tyres.dll` hands back every qualifying set (class slot 1, the ultrasoft) that is neither used nor handed back (`tyres::hand_back_usable_sets`), so FQ tyres' freed qualifiers stay out of the race. Line: `[f1-1990] tyres: session <id>: <n> unused qualifying set(s) handed back on <n> car allocation(s)`. Option `qualifiers_quali_only` (toggle, default on) turns it off.

At every 1990 session start, after the FQ tyres change, `f1_1990_tyres.dll` hands back each set past the 9th of a compound that is neither used nor handed back (`tyres::cap_usable_sets`). The session-enter set pick (0x14368fde0) collects those sets through 0x14f3d1720 into a vector on a 256-byte stack arena: it grows 1, 2, 3, 4, 6, 9, 13, the arena never reclaims, and the 10th set gets a null block (ACCESS_VIOLATION writing 0 in memcpy 0x1453784c0, Friday qualifying, 2026-10-30). Line: `[f1-1990] tyres: session <id>: <n> set(s) on <n> car(s) handed back: the game picks from at most 9 usable sets of one compound`.

`f1_1990_tyres.dll` logs at every 1990 session start which class the cars hold against the class setPresetChoice (0x140bcb9a0) gives the allocations (the first class whose setup-mode, season and weekend-format masks `+0x158`, `+0x15c`, `+0x160` hold the championship's bits, career vtable `+200`): `[f1-1990] tyres: session <id>: compound class <n> on <n> car(s), <n> on another class (<n>), <n> tyre component(s) not found`. Class 10 is `f1_26`, 12 is `f1_1990`.

## Replay state holders (`f1_1990_replay.dll`)

A separate DLL, so a binding it can't resolve drops only this hook and never the save separation. The replay/flashback recorder (`Ne_Replay_WriteFrames`, 0x1445d0e70) keeps a state holder per recordable: a heap block with a 0x80-byte header, a current and a previous state buffer. Its class (one of 20, capacity 0x20 to 0xffff) is fixed at registration. Each frame the recordable serializes into a static scratch buffer and the recorder copies the state into the holder with no capacity check. At 26 cars, at the race start, a per-car recordable outgrows its holder; the copy then overwrites the next heap block (seen: the tyre system's wheel map table, which aborts the race start with heap error 0x16 when the table is freed).

`site_Ne_ReplayAfterRecord` (0x1445d1712, after the recordable serialized, before both copies) is a site hook. When the state is larger than the holder's capacity, the holder is replaced by one of the smallest class that fits. The new holder comes from the registry's allocator; the header and both buffers are copied; the registry slot and the recorder's cached pointers move to it. The old holder stays allocated. The class table is checked against the exe's code once; a mismatch turns the hook off. It logs `[f1-1990] replay holders: <recordable> state <n> bytes > capacity <c>; holder <old> -> <new> (capacity <c>)`. A state that fits returns at the first compare, so a 22-car grid is untouched.

Option `replay_holder_fix` (toggle, default on) turns it off.

## 1990 Career tile (`f1_1990_ui.dll`)

A separate DLL, loaded at the window stage after the others; option `career_menu_ui` (default on), and only when the 1990 career content is in the database (`src/profile.hpp`). The 2025 and 2026 tiles stay stock.

| binding | address | use |
|---|---|---|
| `site_FeTile_CareerContentEnd` | 0x141741831 | site at the end of `FeTile_Build_List` case 6 (r14 = the list): `FeHome_Add_CareerTile(list, 3)` appends `button_3`, then its home item is replaced by a content item |
| `FeTile_Construct_CareerItem` | 0x141725380 | call; builds the 0x1c8 content item with the 2026 tile's route id |
| `FeTile_CareerDlcItem_Populate` | 0x141726680 | called by our populate; then title `lng_f1990_career_content`, description `lng_f1990_career_content_description`, notification 0 (no badge) |
| `FeTile_Route_CareerItem` | 0x141725620 | called by our press after it writes the selected content; wrapped to record whether the last career tile pressed was the 1990 one (exported as `F1990_CareerWizard1990` for `f1_1990_tabs.dll`) |
| `g_FeTile_CustomisationSystem` / `g_FeTile_SelectedCareerContent` | 0x146064df0 / 0x146066c42 | the press selects the car group with stat id 2 (`1990`), else the first dlc group (2026) |

The item carries data group `f1_career_1990` (+0x180), suffix `90` (+0x188), is_dlc 0 (+0x190) and the 2026 entitlement string (+0x1c0); its vtable is a copy of the 2026 tile's with populate and press replaced.

Built against an SDK with `swofty/career_content.hpp` (the core's career content library), none of the hooks in this section are built: the DLL registers the content with the core from its init (`id f1-1990`, data group `f1_career_1990`, suffix `90`, mode `driver_career_1990`, car group 2, 2026 model) and the core owns the tile, the press, the route, the driver-career gate pass and the slot predicate. `F1990_CareerWizard1990` answers from the core's last pressed content tile, and `is_1990_career` (`src/career_1990.hpp`) asks the core for the career's data group when the library is available. It logs `[f1-1990] career menu UI: 1990 Career content registered with the core: ok`; the core logs `[career] content 'f1-1990' ready: ...` once the database has loaded, or why it is off. Against an older SDK the private hooks are built as before. The router and the wizard's use of the data group are not traced yet (see `/mnt/work/.f1-1990/v2/third-tile.md`).

## Flavio in 1990 careers (`f1_1990_voice.dll`)

A separate DLL; option `agent_voice_mute` (default on). `Game_SpeechSystem_PostLine` (0x140a30b60) is wrapped: while the loaded career's data group is `f1_career_1990` (worked out on the core's `career.loaded` event, cleared on `career.unloaded`), a line whose speaker is `agent` or `agent_cutscene` gets its emitter's `vol_speech` RTPC set to 0 (emitter vtable +0x60, the setter the post uses for `random_number_generator`) before it is posted as usual. The line still plays, so its subtitles and the end-of-event callbacks that finish it (0x140ac1160/0x140ac11a0) run; a line that was not posted would never finish. 2025 and 2026 careers keep the agent's voice, and the addon no longer replaces `en_agent*.nfs`.

It logs `[f1-1990] voice: career loaded (data group f1_career_1990); agent voice muted`, `... agent line muted (emitter ...)` and, if a muted emitter also speaks for another speaker, `... that line is silent too`.

The same DLL switches the agent's strings. `data/flavio-strings.json` (the career lane's `tools/career.py`; shipped next to the DLL through the manifest entry's `files`) holds, per game language, every stock string that names Elena rewritten for Flavio. On the first 1990 career load the DLL reads it through the core's storage API, then sets each text for its own language with the core's runtime string overrides (`swofty/lang.hpp`, API v8); `career.unloaded`, or a non-1990 `career.loaded`, clears exactly those overrides, so other careers and modes read the stock tables. It does not call `refresh()`. Built against an SDK below v8 the code is compiled out, and with a core below v8 `swofty::lang::available()` is false; either way the agent keeps Elena's strings.

It logs `[f1-1990] strings: set 1858 agent string(s) in 11 language(s) (0 failed)` on a 1990 career load, `... cleared ...` on unload, and otherwise why the strings stay stock (`built against SDK API v6`, `the core has no runtime string overrides`, or a read or parse error).

## Flavio's character ref (`f1_1990_agent.dll`)

A separate DLL, loaded at the early stage. The UI keeps one VM node per `entrants.characters` row under `character_list`; `Game_EntrantsUi_BuildCharacterRef` (0x1424f1d70) fills it (`stat_id`, `filename`, the names, `job_role`, and the textures `message_avatar` / `message_avatar_race` from `entrants.ui.asset_templates.characters.textures`, `[NAME]` = the row name), and `Game_EntrantsUi_OnCharacterListLoaded` (0x1424bacc0), the `GameCharacterListLoaded` handler, clears the list and builds it from the game's character vector (message +0x10; 0x38-byte rows). The subtitle box, emails and contacts bind these nodes.

While the loaded career's data group is `f1_career_1990`, the builder gets a stand-in record for the `elena` row: the `flavio` row (stat_id 105, `lng_f1990_agent_*`, texture `message_flavio`) with Elena's stat_id. The handler wrap keeps the character vector; when `career.loaded` or `career.unloaded` changes the 1990 state, the DLL calls the handler with it again, so the `elena` node is rebuilt from the stand-in or from her own row. The `message_avatar_race` array has no `message_flavio` slice.

It logs `[f1-1990] agent: character list built from 0x... (N rows, 1990 career ...)` per build (first 8), `agent: 1990 career loaded; character list rebuilt, the elena ref shows flavio` and `agent: 1990 career left; ... shows elena`, and `no flavio row in the character list` when the data side is missing.

## Choose Driver tabs (`f1_1990_tabs.dll`)

A separate DLL. In a 1990 wizard the Choose Driver tab strip shows only the F1 tab, with no F5/F6 hints. The `driver_career_driver_select_header` view model builds its tabs with the shared tab builder 0x144251f40 from the content group `driver_career_driver_select_header` (`header_f1`, `header_selected_team_icon`, `header_f2`, `header_icon`; 0x30-byte entries, tab name at +0x08), tab i from entry i. The screen group widget (`UISG`, 0x144253930) spawns one tab header per entry of the same stock group and binds it to `headers.<name>`, so all four tabs stay in the header: a header without `header_f2` and `header_icon` leaves two unbound tab headers, drawn as empty boxes with their notification diamond. `site_Game_DriverSelectHeader_BuildTabs` (the call at 0x141a591f2) is redirected to record which tabs are F2 and Icons in a 1990 wizard, and `Game_DriverSelectHeader_SetTabState` (0x141a59fe0, `content_visible` and `enabled` of tab N) is wrapped to keep those tabs at 0 and 0, the state the view model gives a tab it doesn't offer, such as the selected team tab. The widget refreshes the hints (0x144252a40) when a tab's `system_data.enabled` changes and hides them while fewer than two tabs are enabled. The wrap also skips an index past the header's tabs. A wizard is a 1990 one when `F1990_CareerWizard1990` (from `f1_1990_ui.dll`) is 1 and no other career is loaded. Other careers keep all four tabs as the game sets them.

It logs `[f1-1990] driver select: 1990 wizard; header tabs mask 0xc off (F2 and Icons)` per build (first 4) and `driver select: tab 2 kept off (asked content 1, enabled 1)` (first 8).

## Power-unit rules (`f1_1990_pu.dll`)

A separate DLL; option `pu_rules_1990` (default on). The career's `EngineManagementComponent` keeps a 0xa60-byte record per driver with seven component slots (record +0x18, 0x10 apart; each holder: fitted components at +0x08, 0x10 each with wear at +0x04, fitted index +0x30, session wear range +0x3c/+0x40). Component configs are 0x250-byte sets per season (`engine_management.components`, read by 0x1411cb150): threshold +0x04, `first_new_grid_drop` +0x18, `subsequent_grid_drop` +0x1c, `parc_ferme_grid_drop` +0x20, `allocation` +0x24, wear ranges +0x28..+0x3c.

| binding | address | use |
|---|---|---|
| `Game_EngineMgmt_ApplySessionWear` | 0x1411d1560 | wrapped: amount 0 for a 1990 car (pending components are still fitted, the wear copied to the car unchanged) |
| `Game_EngineMgmt_AddGridPenalty` | 0x1411d0740 | wrapped: nothing filed for a 1990 car; its only callers are the new-component fit 0x1411d00e0 and the component switch 0x1411cf930 (parc fermé) |

A car is a 1990 car when the driver's career entrant row (driver key low byte = `id_lo` +0x50, high word = `id_hi` +0x52) has a vehicle (+0x5c, stat id in the high word) whose `entrants.vehicles` row has `vehicle_package` `f1_1990` or `f1_1990b`; that list is read on `career.loaded`. Other packages, and every mode without a career, keep the stock rules.

It logs `[f1-1990] power unit: N vehicle(s) of the f1_1990 and f1_1990b packages run without power-unit wear or grid penalties`, then per driver `session wear X skipped` and `N-place grid penalty (reason R) dropped` (first 8 and 16).

## Power-unit parts (`f1_1990_parts.dll`)

A separate DLL; option `pu_parts_1990` (default on); ships `data/pu-strings.json` through its entry's `files`. While a 1990 career is loaded (`career.loaded` with data group `f1_career_1990`; `career.unloaded` or another career turns it off) the power-unit list of the garage monitor and the laptop shows six parts on the stock slots:

| slot | stock part | 1990 part | allocation | wear |
|---|---|---|---|---|
| 0 | ICE | Engine (the race engine) | 4 (the 2026 set's) | its own range, every session group but 1 and 2 |
| 1 | MGU-K | Qualifying Engine | 3 (the 2026 set's) | its own range, session groups 1 and 2 only |
| 3 | ES | Clutch | 6 | 1.2 x the Engine's range, every session |
| 4 | TC | Fuel System | 5 | 0.9 x the Engine's range, every session |
| 5 | CE | Electronics | 5 | 0.8 x the Engine's range, every session |
| 6 | event gearbox | event gearbox | the 2026 set's | its own range, every session |

MGU-H (slot 2) has no row and no wear; the 2026 component set the 1990 career runs on has no MGU-H config.

| binding | address | use |
|---|---|---|
| `site_Sess_PuPanel_SlotHolder` | 0x141f50ecc | in the slot loop of `Sess_PuPanel_BuildComponents` (0x141f50910), `rax` = the slot's holder, `rbp` = the slot and `rsi` = the view model: for slot 2 `rax` becomes an empty holder (no parts) and the game adds no row for it. Rows keep their slot index (+0), which the fit-component handler 0x141f58a20 (fits the part, then refills the row) passes to the entry filler `Sess_PuPanel_FillComponent` (0x141f532b0). The site also writes the allocations (below) through the view model's EngineManagementComponent (`rsi` +0x690) |
| `Game_EngineMgmt_ApplySessionWear` | 0x1411d1560 | wrapped: writes the allocations, then for the call sets each holder's wear range (+0x3c/+0x40) as in the table (0 on a slot that does not run in the session; the Engine's range times the scale for the clutch, fuel system and electronics, read before any change) and puts the ranges back after. The group is the weekend's current session (provider +0x270 vt +0x98, sessions +0x220, index +0x238); an unknown session counts as a non-qualifying one |

Allocations: the EngineManagementComponent's component sets (*(mgmt +0x340) +0x50..+0x58, 0x250 bytes each: season byte, then one 0x54-byte config per slot from +4, allocation u32 at +0x24) are its own copy of `engine_management.components`, built by 0x1411cb150 and freed with the component; the DLL writes the clutch, fuel system and electronics allocations into every set of the 1990 career's component. `Sess_PuPanel_FillComponent` and the new-part fit 0x1411d00e0 read the allocation from there.

`data/pu-strings.json` `strings.<language>.<key>` are set through the core's runtime string overrides (API v8): the `_title` and `_description` keys of `lng_internal_combustion_engine`, `lng_motor_generator_unit_kinetic`, `lng_energy_store`, `lng_turbo_charger` and `lng_control_electronics` (`*`, English), the five titles per language. Only `Sess_PuPanel_FillComponent` reads those keys. The power-unit diagram beside the list still draws the hybrid parts and binds `components[0..6]` by row position.

With `pu_rules_1990` on no part wears at all; the wear split applies with it off. The qualifying engine's power comes from the R&D qualifying lines (`f1_1990_quali.dll`); the handling files have one rev limit per engine for every session.

It logs `[f1-1990] parts: 1990 career: engine, qualifying engine, clutch, fuel system, electronics and gearbox` / `no 1990 career: stock power-unit parts`, `set|cleared N part name string(s)`, `N allocation(s) set in N component set(s): clutch 6, fuel system 5, electronics 5` (first 8), `power-unit panel row 2 (MGU-H) hidden` (first 12) and `driver 0x… session group G: wear W on the race engine|qualifying engine, the clutch, fuel system, electronics (engine range A..B x 1.2 / 0.9 / 0.8) and the gearbox; N range(s) set` (first 16).

## Qualifying engine lines (`f1_1990_quali.dll`)

A separate DLL; option `quali_engine_1990` (default on). The rnd lane's `f1990_quali_rev_limit` and `f1990_quali_engine` are Engine Power (type 0) upgrades that stay in the Engine Power denominator.

| binding | address | use |
|---|---|---|
| `Game_Rnd_FillVehicleUpgrades` | 0x141238cb0 | wrapped: in a 1990 career, reads the current session's group (provider +0x270 vt +0x98 = the weekend, sessions +0x220, 0xa0 each, group +0x98, current index +0x238) and marks the fill when it is not 1 (Qualifying) or 2 (SprintShootout) |
| `Game_Rnd_CalcUpgradeTypeProgress` | 0x143526260 | wrapped: inside a marked fill, type 0 is recomputed (`src/rnd_progress.hpp`) with the two upgrades (node hash 0x1305fa1e / 0x4e03ba09 or name `f1990_quali*`) left out of the completed weighting only |

Every other caller of the progress function (UI, the power-unit wear range) and every other career get the game's value. It logs `[f1-1990] quali engine: car N set up for a session of group G; qualifying engine lines counted|left out` (first 8) and `Engine Power progress X -> Y outside qualifying` (first 8 changes).

## R&D card art and department names (`f1_1990_rnd.dll`)

A separate DLL; ships `data/rnd-art.json` and `data/rnd-strings.json` through its entry's `files`.

- **Card art:** the three card binders (0x14180fc60, 0x141814320, 0x1418169f0) name a frame `<upgrade_name>`, `_in_progress` or `_complete` and format it right after the six `site_Career_RnDCardArt{Image,Mask}N` sites (the rnd-tyres addon's bindings; handlers chain). At those sites a frame of an `f1990_*` upgrade becomes its donor's frame for the same state (`rnd-art.json` `frames`), named the way the list in pkg names it: `om_xm_<part>`, `_progress`, `_complete`, mask `om_xm_<part>_m` in `rnd_om_xm`; `<part>`, `_in_progress`, `_complete`, mask `<part>` in the plain lists (`src/rnd_art.hpp`). Only 1990 trees carry `f1990_` names, so there is no career check. `r8` is the career's R&D UI package (`upgrades.<path>.general.ui_data_0.texture_array_dictionary_path`, read by the upgrades loader 0x143540790); when it is not an R&D package (a build without `upgrades.1990.general` leaves it empty), an `f1990_*` card uses `#overrides.career.rnd_om_xm` and its `om_xm` frames.
- **Department names:** `rnd-strings.json` `strings.<language>.<key>` are set through the core's runtime string overrides (API v8) on a 1990 `career.loaded` and cleared on `career.unloaded`; compiled out below SDK v8.

It logs `[f1-1990] rnd: N card art donor(s)`, once `card art: the career's R&D package is "#overrides.career.rnd_om_xm"` (or why it falls back), `card art site S image "f1990_x_in_progress" -> "om_xm_y_progress"` once per card and site, `no donor for "..."` and `set|cleared N department string(s)`.

## R&D look (`f1_1990_rndlook.dll`)

A separate DLL; option `rnd_look_1990` (default on); ships `data/rnd-look.json` (the uidb package and the colours) through its entry's `files`. On a 1990 `career.loaded` (and `career.hub`) it turns the 1990 look on; `career.unloaded`, or a non-1990 `career.loaded`, turns it off. The colours are indexed by the department byte (0 aero, 1 chassis, 2 durability, 3 engine, 4 the rnd-tyres Tyres department, which keeps rnd-tyres' orange).

| binding | use |
|---|---|
| `k_Career_RnDDeptColourTable`, `k_Career_RnDCardColourTable` | entries 0..4 written with the 1990 colours while the look is on (department tabs, purchase panel, upgrade cards, the performance pie), the previous values put back when it turns off |
| `site_Career_RnDListRowColourStore`, `…LightColourStore`, `site_Career_RnDPopupColourStore`, `site_Career_RnDInProgressColourStore` | the stored colour of departments 0..4 becomes the 1990 colour (light tint for the light store) |
| `site_Career_RnDPieSliceIconArgs` | R&D > Performance tiles: the slot colour `[rsp+0x64]`, stored by the populate switch and read at 0x141f48e86, becomes the 1990 colour unless the slot is `shutdown` or `regulation`; the wheel-icon package is swapped as below |
| `site_Career_RnDUpgradeListDeptIcon`, `site_Career_RnDListRowDeptIcon`, `site_Career_UpgradeCardDeptIcon`, `site_Career_RnDPerksTitleDeptIcon`, `site_Career_RnDCardDeptIcon0..2`, `site_Career_RnDInProgressStrip` | `r8`, the career's R&D package (`#overrides.career.rnd_om_xm`, read from the UpgradesComponent +0x660), becomes `#overrides.career.f1990_rnd` for the department atlases |
| `Career_Construct_RnDDeptCard` | wrapped: the department tab's icon path `<package>.rnd_dept_icons_small.<frame>` (built before the call; rnd-tyres' Tyres tab included) gets the 1990 package |

`#overrides.career.f1990_rnd` is the uidb package `erp/rnd_look.json` adds (`tools/rnd_look.py`); part images, masks and category icons keep the career's package. Not covered: the Team HQ research overview pie (0x141f41d60) and Team HQ department tiles (0x1422fe430), both My Team only; the 148 px department card icon (`#overrides.career.dept_card_icons`, an absolute path in 0x14180e910), whose 1990 list ships unused until a site at 0x14180ecef is registered; the performance comparison chart's bar colours, which the shared `laptop_r_and_d.ui` scenes animate.

It logs `[f1-1990] rnd look: package …, colours …`, `<table> … -> 1990 colours` and `restored to …`, `1990 career: 1990 department colours and icons on` / `no 1990 career: stock department colours and icons`, and per site (first 4) `package #overrides.career.rnd_om_xm -> #overrides.career.f1990_rnd`, `department N <was> -> <now>` or `code N (aero) <was> -> <now>`.

## 1990 career setup (`f1_1990_wizard.dll`)

A separate DLL; option `career_wizard_1990` (default on). A new 1990 Driver Career runs Recommended / Custom settings,
the confirm step, then Choose Driver with the official 1990 drivers. The career wizard keeps its selections (options,
wizard state +0x100) and steps (+0xe0) in a state object whose +0x128 is the data group of the Career tile that opened
it (`f1_career_2025`, `f1_career_2026`, `f1_career_1990`); every hook acts only when that is `f1_career_1990`, so the
2025 and 2026 wizards keep every step.

| binding | address | use |
|---|---|---|
| `Game_CareerWizard_StartsInF1` | 0x14078ddd0 | wrap, yes: the router skips the Skip to F1 / Start in F2 step, the step machine drops the team-icons step (My Team / Konnersport) after Recommended settings, the driver select confirm takes the F1 path (as option 6) |
| `FeTile_Route_CareerItem` | 0x141725620 | wrap: the 1990 tile's `career_wizard_driver_career_to_f1` (season length step) becomes `career_wizard_driver_career_1990`, which `erp/wizard.json` adds to the content select's route grouping (`tools/wizard.py`) and which opens the settings step: no Full Season / Custom Season choice, the career keeps its full 1990 calendar |
| `Game_CareerWizard_PushNextStep` | 0x14386d890 | wrap: a classic drivers step (14, Add Driver Icons To Market) pushed after the settings becomes the confirm step (6) |
| `site_Game_CareerWizardConfirm_NextRoute` | 0x141a23c91 | site in the confirm step's continue (rdx = route name, rsi = view model, +0x270 the wizard): `driver_career_driver_type_select` (Official Driver / Custom Driver) becomes `driver_career_driver_select`, the route the game takes for a team-icon career |
| `Game_SeasonLength_ApplyPreset`, `k_Game_TrackListComponentDesc` | 0x140bc9fc0, 0x14605b368 | called from the `Game_CareerWizard_PushNextStep` wrap when the settings choice (option 0xb or 0xc) is pushed and the wizard has no season length option: preset 0 on the career's season length component, what the skipped step's Full Season does, so the career gets the 1990 calendar (all three 1990 presets hold its 16 rounds) |
| `site_Game_DriverSelect_BackRoute` | 0x141a5e8eb | site in Choose Driver's back (rdx = route name, rsi = view model, +0x438 the wizard): `screen_group_back` becomes `screen_group_back_1990`, and the confirm step and the settings choice come off the wizard state |
| `site_Game_CareerWizard_BackRoute`, `vt_Game_CareerWizardSettingsSelectViewModel` | 0x14bc7ab4d (.data1), 0x145727970 | site in the wizard view models' base handler after its back pop (rdx = route name, rsi = view model, +0x270 the wizard): for the settings view model `back` becomes `back_1990` |


`erp/wizard.json` (`tools/wizard.py`) adds the routes: `screen_group_back_1990` runs the header's `back_1990`, which
closes the driver select screen group and opens the settings step again; the settings step's `back_1990` returns to the
Career tiles (`fe_career`) with the steps of the season length step's `back_4`. Back on Choose Driver goes to the
settings step, back on the settings step to the Career tiles.

It logs `[f1-1990] wizard: f1_career_1990 starts in F1, no F2 choice and no team-icons step (game said yes|no)`,
`wizard: 1990 tile opens the settings step (career_wizard_driver_career_to_f1 -> career_wizard_driver_career_1990)`,
`wizard: classic drivers step (14) replaced by the confirm step (6)` and
`wizard: confirm goes to driver_career_driver_select, no driver type step`,
`wizard: season length preset 0 (Full Season) applied to the 1990 career`,
`wizard: Choose Driver back goes to the settings step (screen_group_back_1990; confirm step dropped, settings choice dropped)`
and `wizard: settings back goes to the Career tiles (back_1990)` (first few of each).

### 1990 calendar

A career runs its rounds from the season length component's calendar (+0x2a8 vector, 0x28-byte entries: event stat
id +8, minute offset u32 +0x18, date time_t +0x20). Each entry's date is its event's own `schedule.all_track_events`
`date_time` (the Thursday of the stock weekend: 2026 for the `_26` events, 2025 for Imola). GameModeChampionship::begin
(0x1407b5c40) calls every career component's vt+0xd0 and then season start (0x140ae35a0), which builds the weekends. On
the season length component vt+0xd0 is slot 26 (0x140bc9be0, a protected body). Slot 77 (0x140bc9360) sorts the calendar
by date, or keeps the events of a default list that isn't `sort_by_date`, but GameModeChampionship::begin doesn't call
it. Without the fix a 1990 career ran the calendar it had at begin:
- the 16 1990 events in stock 2026 order, Australia first, when the wizard applied the F1_1990 preset;
- the stock 2025 list, when it didn't (the v8 core's wizard path).

For a 1990 career (data group `f1_career_1990`, or setup mode `f1_1990`, stat id 16) whose season has no started
weekend, the DLL fixes the calendar at four points: each 1990 wizard step, career load, before slot 26, and again after
slot 26 when slot 26 changed it. Slot 77 is hooked too. At each point it:
- refills the calendar from the `F1_1990` list of the dynamic event data (career +0xf58, entries +0xb8..+0xc0,
  `{name, allocator, begin, end, capacity}`) with `Game_SeasonLength_PushCalendarEntry` (0x140bc9d80), unless it holds
  exactly those events in that order;
- moves each round's date by whole days from its stock day to its 1990 day in the career year
  (`data/calendar-1990.json` from `tools/career.py`: the Thursday before the Sunday on or before the 1990 race date),
  only while the date still falls on the stock day, so a second pass changes nothing;
- sets each entry's minute offset from the season start (`season_start`, the event list's start_date, 2026-02-11), as
  the game's date sort does.

The rounds then run Phoenix first: 134 Phoenix 5 March, 133 Interlagos, 27 Imola, 117 Monaco, 116 Montreal,
132 Mexico City, 119 Paul Ricard, 121 Silverstone, 111 Hockenheim, 124 Hungaroring, 123 Spa, 127 Monza (stock day),
128 Estoril, 118 Jerez, 112 Suzuka, 110 Adelaide 29 October. A career whose season already has a started weekend is
left alone. `src/calendar_1990.hpp` (list lookup and order, date shifts and offsets, the day arithmetic) is tested by
`test/calendar_test.cpp`, which also checks the shipped file.

It logs:
- once, `[f1-1990] calendar: 16 1990 round date(s) from data/calendar-1990.json, season start 2026-02-11`;
- `calendar: 1990 career at a wizard step|the settings step|career load|career begin|after career begin|season begin
  (setup mode M, default track list "D", W weekend(s)); calendar N event(s) [ids]; F1_1990 has 16 event(s); calendar
  kept|calendar refilled from F1_1990; K date(s) moved to the 1990 days, O offset(s) set [id@YYYY-MM-DD,...]`;
- `calendar: career begin left the calendar as F1_1990|changed: N event(s) [ids]`;
- on load, `calendar: 1990 career loaded; season has N weekend(s) (S started) [names]; calendar N event(s) [ids]`;
- `calendar: 1990 career at ... has S of W weekend(s) started; calendar left as it is`.

## Intro video (`f1_1990_intro.dll`)

A separate DLL; option `intro_video_1990` (default on). A new 1990 Driver Career opens with `videos/f1_1990_intro.bik`
in the slot of the agent's intro cutscene (the DriverCareerContractNegotiations Intro state, which the contract skip
passes in a 1990 career). Then the Driver HQ intro cutscene and the hub follow as before. The addon installs it from
`carpack/video/f1_1990_intro.bik` (git-ignored, encoded by `carpack/tools/intro_video.sh`). The game plays a `.bik` as Bink 1 and a `.bk2` as Bink 2; the free RAD Video Tools encode
Bink 1.

| binding | address | use |
|---|---|---|
| `Career_ContractIntro_OnEnter` / `_OnUpdate` | 0x1436c2620 / 0x1436c2dc0 | wrap at priority 1, outside the contract skip's wraps: armed on enter when `F1990_ContractSceneSkipped` (f1_1990.dll) is 1; the first update plays the video, later updates return the stay link until it ends, then the contract skip continues |
| `Ne_Play_FullscreenVideo` | 0x14416de50 | call: `neon::NeUIGenericFeatureSystem::playFullscreenVideo(path, {fn, ctx} end callback, null, layer 4)`, as the session intro videos |
| `g_Game_Root` | 0x14601db08 | data: the feature system is `(module+0x10)->vt[0xf0]`, module `*(*(root+0xa0)+0x28)` (FePopup_Dispatch_FeatureCmd's path); its player is at +0x108 |
| `g_Game_FlowLinkStay` | 0x146cb2280 | data: the stay link |
| `Ne_Make_StrFromCStr` | 0x1441974e0 | call: the path NeStr |
| `Ne_Post_BusMessage`, `g_Game_MessageBus` | 0x1439876c0, 0x14618b3b8 | call / data: `AllowVideoSkip` on the live bus (holder +0x08, active byte +0x68) |

The DLL reads the file's header from the game folder (`videos/`, or `swofty/overlay/videos/` when the runtime file
feature serves it, passed as `videos/../swofty/overlay/videos/f1_1990_intro.bik`) and plays it only when it is a Bink 1
file of at most 600 s. It posts `AllowVideoSkip` on (the stock skip prompt; the game's persistent flow view model stops
the video on its skip input after 2 s) and off at the end, with `Ne_Post_BusMessage` on the live bus, not the core's
`swofty::career::post`, which refuses a message whose vtable is outside the exe image. The message's vtable is in the
DLL; its slot 1 returns the name the subscriber matches. The state stays
until the player's end callback runs (the video ended or was skipped), or the video's length plus 15 s has passed.
Without the file, the option, the player or the contract skip, the Intro state behaves as before. `src/intro_video.hpp`
(header parse, checks, stay/continue) is tested by `test/intro_video_test.cpp`.

It logs `[f1-1990] intro: new 1990 career; playing videos/f1_1990_intro.bik (Bink 1 rev i, 1280x720, 1099 frames at
29.97 fps, 36.7 s) on layer 4 (feature system ..., player ...)`, `intro: AllowVideoSkip on|off posted on bus ...` (or
`not posted: the game's message bus is not active`), then `intro: video ended|skipped|no end callback;
timed out after N s; the career continues`, or once why it plays nothing (`no videos/f1_1990_intro.bik (the addon
installs it from carpack/video)`, `is a Bink 2 file`, `no fullscreen video player`).

## Core libraries (`SWOFTY_API_CHARACTERS`, `SWOFTY_API_CAREER_WIZARD`)

Built against an SDK whose `swofty_api.h` defines `SWOFTY_API_CHARACTERS` and `SWOFTY_API_CAREER_WIZARD` (launcher 1.3.0 and later), four DLLs register with the core instead of hooking, all for data group `f1_career_1990`:

| DLL | registers |
|---|---|
| `f1_1990_agent.dll` | `swofty::characters::swap("elena", "flavio")` |
| `f1_1990_voice.dll` | `mute("agent")`, `mute("agent_cutscene")` (option `agent_voice_mute`), `strings_json` with `data/flavio-strings.json` |
| `f1_1990_wizard.dll` | `swofty::career::wizard::start_in_f1`, `tile_route("career_wizard_driver_career_1990")`, `skip_driver_icons`, `skip_driver_type`, from its init so the core's steps hook chains with its own (option `career_wizard_1990` clears them at load) |
| `f1_1990_tabs.dll` | `hide_driver_tab("header_f2")`, `hide_driver_tab("header_icon")` |

`f1_1990_wizard.dll` keeps its own Game_CareerWizard_PushNextStep wrap for the Full Season preset (no step change) and both back-route sites, which the core does not cover. Against an SDK without those macros the DLLs keep their own hooks, as described above. The route `career_wizard_driver_career_1990` still comes from `erp/wizard.json`.

## Session names (`f1_1990_sessions.dll`)

A separate DLL; option `session_names_1990` (default on); ships `data/session-strings.json` through its entry's `files`.
The game names a session by its type (long names 0x145736910, short names 0x14576e538 and the timing tower's
`race_data.uidb`; the 1990 weekend runs types 13/14 on Friday and 8/9 on Saturday) and a session group by its id
(`lng_sprintshootout`, `lng_qualifying`; also the weekend settings' weather and time-of-day rows). `strings.<language>.<key>`
are set through the core's runtime string overrides (API v8) on a 1990 `career.loaded` and cleared on `career.unloaded`
or another career's load; below SDK v8 the DLL only logs that the names stay stock.

It logs `[f1-1990] sessions: set|cleared N session name string(s) (M failed)`.

## Qualifying lap limit (`f1_1990_qualilaps.dll`)

A separate DLL; option `quali_laps_1990` (default on). In Friday and Saturday qualifying of a 1990 weekend (session ids 5..14 under the `f1_1990` setup mode's weekend), every car, player and AI, may start 12 laps.

| binding | address | use |
|---|---|---|
| `Sess_TyreAllocation_OnSessionStart` | 0x140bcc190 | wrapped (chains with `f1_1990_tyres.dll`): a 1990 qualifying session turns the rule on and resets every car's count; the player's car is the career entrant with the human flag (+0x4b bit 0x10), car index +0x82 |
| `Game_Pitlane_SetVehicleStatus` | 0x1434bc540 | wrapped: keeps each car's pit status (read back from `[self+0x68+car*16]+4`) and the pit lane system; entering status 13 (LeavingGarage) starts a lap |
| `session.lap` | core event | a line crossing while the car's status is 1 (OnTrack) or 15 (driving through the pit lane) starts a lap, when it comes at least 45 s after the car's last counted start |
| `Sess_InGame_UpdateBase` | 0x142d566a0 | wrapped: on the next session frame, sends each queued car in with `Game_PitLane_PutVehicleInPitBox` (0x1434d1260, the red flag's call) when its status is still 1, 7, 13, 14 or 15 |

A car that has started 12 laps and crosses the line again, or leaves its garage or its pit box again (status 13 or 7), is queued and put in its pit box (status 4). With the pit lane's forced-box byte (+0x300) set, the crew pushes it on into the garage; without it the car waits in the box, and each release sends it back again. Its count stays at 12. Garage exits and crossings follow the counter in `src/quali_laps.hpp` (host test `test/quali_laps_test.cpp`). The 45 s gap keeps a flashback over the line, a car reversing over it or a pit exit just before it from counting twice.

The player gets a race HUD toast (`swofty::hud::toast`, English) at each lap start: `LAP n OF 12` with the laps left after it, `Last qualifying lap: box at the end of it` on lap 12, and `NO LAPS LEFT` with the chequered flag icon when the car is sent in. AI cars get no toast.

Lines: `[f1-1990] quali laps: session <id>: 12 laps per car, player V<nn>`; `... V<nn>[ (player)] left its garage|crossed the line: lap <n> of 12 started` (every player lap, every AI lap 12, the first 8 other AI laps); `... V<nn>[ (player)] <crossed the line|left its garage|left its pit box> with all 12 laps used; sent to its garage on the next frame`; `... V<nn> put in its pit box (pit status <a> -> <b>, forced box <0|1>: ...)`; `... V<nn> is already on its way in (pit status <n>); not sent`. Without the core's `session.lap` (an older core) it logs `no session.lap` and only garage exits count. The environment variable `F1990_QUALI_LAPS` (1 to 12) lowers the limit for a test run and logs `F1990_QUALI_LAPS=<n> (test limit)`.
