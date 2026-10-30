# F1 1990

Adds a 1990 Driver Career next to the 2025 and 2026 ones. 1990 careers live in the normal profile save, in their own slot list (data group `f1_career_1990`).

## Grid (db/grid.json, db/grid_standings.json, lang/grid.json, tools/grid.py)

The 1990 teams (`<team>_1990`), drivers (`f1990_<driver>`), their looks, holders and entrants, and the entrant lists `f1_1990`, `f1_1990_driver_pool` and `f1_career_1990` are new rows; no 2025 or 2026 row changes. The **Cars** setting (`cars`) is 22 (default: the 11 strongest teams) or 26 (adds Larrousse and Dallara; needs 26-car track layers). Every 1990 driver wears his team's 1990 suit and gloves (`registry/kit.json`, `db/kit.json`, `tools/kit.py`): 13 new suit and gloves rows with their own archives, worn only by the 1990 looks and team kits. See `tools/README.md`.

### Faces (faces/, db/faces.json, tools/faces.py)

A driver with a face in `faces/faces.json` wears his own head, `f90_<driver>`: a stock scanned head (`base`) with the face map from the developer's face zip. Nigel Mansell's is on `felipe_massa`. The head row is a new row copied from `male_ls222`, and only the driver's 1990 row (`f1990_<driver>`) wears it. The archives `faces/heads/f90_<driver>.erp` and `faces/morph_poses/f90_<driver>.erp` are build output (not committed). A new face zip in the same format becomes a head with:

```sh
python3 addons/f1-1990/tools/faces.py ~/Downloads/<driver>.zip [--base <scanned head>]
```

## Front end and tracks (erp/frontend.json, erp/badges.json, erp/hq_logos.json, erp/tracks.json, art/logos/, tools/frontend.py, tools/badges.py, tools/logo_art.py, tools/hq_logos.py, tools/tracks.py)

The 1990 rows get their own Driver Select and Team Select backdrop names and their period team logos, added next to the stock art, and the `f1_1990` setup mode loads its own track worlds (26-car garages and grid slots when `cars` is 26). See `tools/README.md`.

The logos (`art/logos/`, sources and licences in `art/logos/SOURCES.md`) fill a slice per 1990 team in 22 UI texture arrays, each keyed by the name the game builds from the team row (`entrants.ui.asset_templates.team.textures`, `[TEAM]` = `<team>_1990`):

| surface | arrays |
|---|---|
| Team Select (tiles, cards), grid rundown, timing towers, standings and post-race lists, Driver Select, R&D and Team HQ tiles, career team logos, career rewards | `team_logos.f1_2026`, `teams.mp_team_select_logos`, `broadcast_ui_generic.grid_team_logos`, `osd.timing_tower.team_badges_hud` / `_broadcast`, `teams.badges_broadcast`, `teams.driver_select_team_logos`, `team_logos.driver_career_2024_team_select`, `driver_career_rewards.recognition.team_icons` |
| contracts: career hub offers, negotiation, Team HQ cards and backdrop, holding screen banner; press interview pill | `teams.team_contract_logo`, `team_hq.contract_negotiation_logos`, `team_hq.contract_logos_mid`, `team_hq.contract_logos_large`, `team_hq.contract_holding_screen_bgs`, `press_interviews.pill_bg` |
| F1 World fan zone | `teams.fanzone_room`, `teams.teams_f1_2024_gp_voting`, `teams.fanzone_expressions(_large)` |
| blank, as for the stock F1 teams | `teams.career_laptop_header` (the career laptop header; the garage monitor shows its logo only when this key exists), `broadcast_ui_generic.team`, `teams.career_team_select_logo_small` |

In the Driver Career HQ (`erp/hq_logos.json`, `tools/hq_logos.py`), the R&D folder and notebook ask for the material context `2026-<livery_name>`; each 1990 team gets that context in `f1_driver_hq.erp`, with its own material and diffuse texture: the stock unbranded prop in the team colour, with the team logo where the stock Williams prop has its logo. The team cap and helmet props have no 2026 team contexts in stock, so every team gets their default.

Not covered: the garage boards and pit perches of the 1990 track worlds still show the 2026 team of each garage slot (`garage_board_f1.emb?context=2026-<team>` and `pit_perch_<team>` are placed per slot in each track's layers).

## Rules (db/rules.json, db/rules_text.json, tools/rules.py)

`tools/rules.py [stock gamedatabases.2025.bin]` writes both op files from `tools/rules_fields.json` (the field lists of the cloned rows), `tools/career.py` (`TRACK_DATES`, `TRACK_NAMES`) and the stock database (the option tag lists it extends). The 1990 rules live in their own rows; 2025 and 2026 rows keep their stock values, and stock rows only gain the `f1_1990` name in lists and higher row counts. `tools/check_rules.py <stock> <built>` checks a build.

- Setup mode `f1_1990` (stat_id picked by the build; `native/f1_1990_wizard.dll` reads it by label), a clone of `f1_26` with option tag `gamesetup_f1_1990`, no tyre warmers, DRS lap group `f1_1990` (opens on lap 200), `tyre_regulations.allow_strategy_mandatory_stop_check` False, and the grid cutscene and grid rundown off unless `grid_cutscene` or `grid_rundown` is on.
- Weekend (default structure): P1, P2, Friday qualifying (a single sprint-shootout session), Saturday qualifying (a single qualifying session), race. Full qualifying runs the single sessions; the race grid comes from Saturday qualifying; the race scores with `f1_1990_race` (9-6-4-3-2-1, no fastest-lap point).
- Option tag `gamesetup_f1_1990`: the `gamesetup_f1` restrictions plus safety car Off only, parc fermé Off only, standard weekend only and no full qualifying. The 38 options shown for `gamesetup_f1` are also shown for it.
- Track dataset `f1_1990` (parent `f1_2026`, stat_id 20, default format only): on every 2026 track C4 for the softer dry compound, C3 for the harder and C5 for the medium (qualifying) compound, presets `f1_1990` and `f1_1990_mandatory`, no required pit stop and the default weekend format; the 16 calendar tracks also carry the 1990 dates, or with `career_year` 2026 the Sunday on or before each (TrackGeneralData), and grand prix names (TrackNameData) from `tools/career.py`.
- Compound class `f1_1990` (stat_id 12): the softer slot is the red Supersoft (`Supersoft_f1_1990`, C4), the harder slot the yellow Soft (`Soft_f1_1990`, C3) and the medium slot the purple Ultrasoft qualifying tyre (`Ultrasoft_f1_1990`, C5). Each row sets its own sidewall context (`SuperSoft`, `Soft`, `UltraSoft`), colour, HUD icon (`icon_code` U+E190, U+E192, U+E191) and pictures (`texture_name` `SuperSoft_f1_1990`, `Soft_f1_1990`, `UltraSoft_f1_1990`), the 2018-style SS, S and US icons below. Its frontend compounds are its own Soft and wet (`frontend_compound` `Soft_f1_1990`, `frontend_compound_wet` `Wet_f1_26`): a showroom car shows the class's frontend compound through the class's own model slots, and the `Hard_2018` of `f1_26` has none in this class. Allocation `f1_1990`: `soft_sets` supersoft sets and `soft_sets` soft sets in P1, P2 and the race, `quali_sets` ultrasoft sets in Friday and Saturday qualifying. Mandatory `f1_1990_mandatory`: one ultrasoft set in the shootout sessions. `no_tyre_allocation` turns the allocation screen off for this class.
- The track lighting probes also list `f1_1990`.
- Pit stops: the `f1_1990` setup mode takes the pit crew probability set `f1_1990` (`pit_crew_training.probabilities`, both `pit_crew_training_probability_set` and `..._equal_performance`). Its routine stop runs 10.5 to 12.5 s at a crew training level up to 20 %, 9.5 to 11.5 s up to 40 %, 8.5 to 10.5 s up to 60 % (the Driver Career default is 50 %), 7.5 to 9.5 s up to 80 % and 7 to 8.5 s up to 100 %, with the stock F1 mistake chances and an 18 s nose change. Each 1990 team has a `pit_lane.pit_times` row: McLaren 7 to 8.6 s, Ferrari 7.1 to 8.8, Williams 7.2 to 9, Benetton 7.5 to 9.4, Tyrrell 8.2 to 10.2, Lotus 8.4 to 10.5, Leyton House 8.5 to 10.6, Larrousse 8.6 to 10.8, Ligier 8.8 to 11, Minardi 9.2 to 11.6, Brabham 9.3 to 11.8, Dallara 9.4 to 12, Arrows 9.5 to 12.2 (fully upgraded 0.5 and 0.6 s lower). Every track of the `f1_1990` dataset carries `TrackPitData` with the stock F1 pit loss (`pitStopTime_Dry`, `pitStopTime_Wet`) plus 7 s and the stock pit speed limit. Other modes keep the `f1` set, the stock teams' rows and the stock pit loss.
- Session names (`data/session-strings.json`, `tools/sessions.py <dir of language_<code>.tsv>`): the Friday session
  reads Qualifying 1 and the Saturday session Qualifying 2. The game names a session by its type and a group by its id,
  from stock keys every mode shares, so `native/f1_1990_sessions.dll` sets these keys only while a 1990 career is loaded
  (core API v8): `lng_sprintshootout_single`, `lng_sprintshootout_oneshot`, `lng_sprintshootout`, `lng_sprint_shootout`
  take each language's `lng_qualifying_1`, `lng_sprintshootout_single_short` and `lng_sprintshootout_oneshot_short` its
  `lng_qualifying_1_short`, `lng_qualifying_length_short`, `lng_qualifying_length_one_shot` and `lng_qualifying` its
  `lng_qualifying_2`, `lng_qualifying_short_short` its `lng_qualifying_2_short`; `lng_sprintshootout_results` and
  `lng_qualifying_results` read Qualifying 1 Results and Qualifying 2 Results in every language.
  `lng_qualifying_one_shot_short` stays stock: it is also the concise label of the One-Shot qualifying length.
- Power-unit parts (`data/pu-strings.json`, `native/f1_1990_parts.dll`, option `pu_parts_1990`): in a 1990 career the
  power-unit list of the garage monitor and the laptop has six rows: Engine (the ICE slot, the race engine), Qualifying
  Engine (MGU-K), Clutch (ES), Fuel System (TC), Electronics (CE) and the event gearbox. MGU-H has no row. Allocations:
  4 engines and 3 qualifying engines (the 2026 component set's), 6 clutches, 5 fuel systems, 5 electronics. The
  qualifying engine wears only in Qualifying 1 and 2, the race engine in every other session, the clutch, fuel system
  and electronics in every session at 1.2, 0.9 and 0.8 times the race engine's rate; with `pu_rules_1990` on nothing
  wears.

Not scoped to 1990 (global rows, left stock): the safety-car and parc-fermé option defaults, the short-race restriction (very short races grey out red flags, formation lap and safety car) and the power-unit component sets (`engine_management.components`), which the game picks by season only: `f1_1990` runs on `season_2026`, so 1990 has the 2026 allocation, wear and grid drops.

## Cars (db/cars.json, cars/, tools/build_cars.py, tools/cars.py)

The 1990 cars live in their own vehicle package, `f1_1990`. The build picks the stat ids of the packages and vehicle rows (no `stat_id` in their add-row ops); native code finds them by label. Its `resource_path` is `f1_1990_vehicle_package/teams/[FILENAME]/wep/[FILENAME]`. `db/cars.json` sets its `ai_package_folder` to `f1_2026_vehicle_package`; `carpack/db/ai.json` moves it to the 1990 cars' own AI data (see AI below). No 2026 archive, row or file changes. The models, the package's `teams/common.erp` (13-inch tyres) and its tyre compounds come from the car pack in `carpack/` (see Car pack below), which registers one asset group per car at runtime. Without the built car pack archives, a session with a 1990 vehicle hangs on load.

| Vehicle | Car folder (AI data) | Chassis | Engine | Power | Limiter | Mass | Wheelbase | Plank OffsetZ/ExtentZ |
|---|---|---|---|---|---|---|---|---|
| mclaren_1990 | mclaren | McLaren MP4/5B | Honda RA100E V10 | 690 hp at 13,500 rpm | 14,000 rpm | 601 kg | 2.934 m | +0.114/1.123 |
| ferrari_1990 | ferrari | Ferrari 641 | Ferrari 036 V12 | 680 hp at 12,750 rpm | 13,250 rpm | 570 kg | 2.865 m | +0.150/1.018 |
| williams_1990 | williams | Williams FW13B | Renault RS2 V10 | 660 hp at 12,800 rpm | 13,300 rpm | 616 kg | 2.789 m | −0.086/0.992 |
| benetton_1990 | lotus | Benetton B190 | Ford HBA4 V8 | 660 hp at 12,800 rpm | 13,300 rpm | 613 kg | 2.882 m | +0.149/1.026 |
| tyrrell_1990 | mercedes | Tyrrell 019 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 634 kg | 2.910 m | +0.155/1.040 |
| lotus_1990 | redbull | Lotus 102 | Lamborghini 3512 V12 | 640 hp at 13,000 rpm | 13,500 rpm | 617 kg | 2.758 m | −0.046/0.962 |
| leytonhouse_1990 | force_india | Leyton House CG901 | Judd EV V8 | 640 hp at 12,500 rpm | 13,000 rpm | 642 kg | 2.910 m | +0.150/1.040 |
| larrousse_1990 | myteam_ferrari | Lola LC90 | Lamborghini 3512 V12 | 640 hp at 13,000 rpm | 13,500 rpm | 610 kg | 2.865 m | +0.150/1.030 |
| ligier_1990 | sauber | Ligier JS33B | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 642 kg | 2.934 m | +0.161/1.068 |
| minardi_1990 | toro_rosso | Minardi M190 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 640 kg | 2.910 m | +0.153/1.043 |
| brabham_1990 | gm_cadillac | Brabham BT59 | Judd EV V8 | 640 hp at 12,500 rpm | 13,000 rpm | 650 kg | 2.758 m | −0.046/0.964 |
| dallara_1990 | myteam_honda | Dallara F190 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 654 kg | 2.789 m | −0.086/0.987 |
| arrows_1990 | haas | Arrows A11B | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 648 kg | 2.865 m | +0.150/1.024 |

Power and mass are the default `handling` 1990 values. The figures are the period ones for each engine (the Lamborghini 3512 at 640 bhp and 13,000 rpm, the Ford DFR at 620 bhp and 11,250 rpm with an 11,750 rpm limit); every limiter sits 500 rpm above the power peak.

- Vehicle rows: full copies (246 keys) of the donor folder's 2026 row, `vehicle_package f1_1990`, `hud_dial` from the option (default `f2`), `drs_capable False`, `equal_performance_vehicle` empty, `performance_tier` 0 (McLaren, Ferrari, Williams, Benetton), 1 (Tyrrell, Lotus, Leyton House, Minardi, Brabham, Arrows, Dallara) or 2 (Ligier, Larrousse), and the grid lane's strings `lng_f1990_team_<team>`, `lng_f1990_chassis_<team>`, `lng_f1990_engine_<team>`. The labels use the grid lane's tokens (`leytonhouse_1990`, `ligier_1990`). `engine` is the 1990 engine row `f1990_engine_<supplier>` (A and B rows).
- Engine rows (`db/cars.json`, `lang/engines.json`): seven `entrants.engines` rows, one per 1990 engine (`f1990_engine_honda`, `_ferrari`, `_renault`, `_ford_hb`, `_ford_dfr`, `_lamborghini`, `_judd`), copies of `Cosworth_F1_V8_2009-2013` (the build picks the stat_id). The rev lights are that row's thresholds × the engine's limiter / 18,000, so the lights end at the limiter: start 72 %, red 89 %, purple 94 % (just under the power peak), warning 99 %. Honda 10,110 / 12,440 / 13,220 / 13,840 / 14,000 rpm, Ford DFR 8,490 / 10,440 / 11,100 / 11,620 / 11,750. `revDial*` stay 0 as on every stock F1 row (only road cars set them). `supplier` is the stock Honda, Ferrari and Renault string, or `lng_f1990_engine_supplier_ford`, `_lamborghini`, `_judd`.
- Package row: a copy of `f1_2026` with the fields above, the 2026 cockpit halo flag (True), the 2026 animation, crew, garage, pit-stop and pit-wall groups, and `is_26_aero_regulations False` when `classic_aero` is on.
- Handling: each car's handling file is built from its donor's stock 2026 handling file and shipped as `cars/handling/<ers>_<tyre_size>/<folder>_handling.erp`; `cars/handling.json` puts the one matching the options at `f1_1990_vehicle_package/teams/<folder>/wep/<folder>_handling.erp`, which the car's asset group lists. No EnergyRecoverySystem section (or zeroed, option `ers`), naturally aspirated engine with the engine's power peak and limiter from the table (the stock 15-vertex rev grid scaled so its vertex 9, where the stock curve and the R&D engine deltas peak, sits at the power peak; engine braking on the same grid; ICE wear by revs and the optimal fuel revs scaled with it), mass = 575 kg × the team's weight scalar, option `handling`: `1990` (the default) scales every lift curve and wing lift coefficient by 0.75, sets Tyre GripMultiplier 0.915 front / 0.925 rear, scales MOIX/Y/Z and the BrakeECU torques by the mass ratio; `1990_low` scales the lift by 0.5, sets GripMultiplier 0.83 / 0.85, keeps the donor's MOI and scales the brakes by the mass ratio × 0.9; both scale FFBScale by 1.3 and use period power, mass and drag (the engine's power from the table, 620 kg × the team's weight scalar, drag × 1.25 on top of the team scalar, the 620 kg being 500 kg dry, a 72 kg driver and 48 kg for the period fuel the game's fuel model leaves out) (archives `cars/handling/<ers>_<tyre_size>_<handling>/`); `modern` scales MOIX/Y/Z by the mass ratio, uses 650 hp × the team's power scalar, drag × its drag scalar, no moving front wing, no DRS hardware, tyre physics group `F1_1990` (the car pack's compounds are `F1_1990_*`, so no stock compound file shares their names), the donor's tyres (option `tyre_size` 2026, the default; the car pack draws the 13-inch rim with a tyre of the 2026 outer radius) with the car 18 mm higher (RestLength and its setup range + 18 mm; MinHeight, BumpStopHeight and the HeightTo* splines − 18 mm; RideHeightOffset, RideHeight and the setup ride-height range + 18 mm, so the aero reads the donor's ride heights), or with `13in` 13-inch tyres (TyreRadius 0.323/0.330, AlloyRadius 0.182, damage graphs remapped, final drive scaled), each axle's suspension height frame (MinHeight, MaxHeight, BumpStopHeight, the HeightTo* splines) moved down by the radius drop and RideHeightOffset set to it, so the car keeps the 2026 ride height, travel and aero ride heights; `13in_raised` moves the frame by twice the drop and adds the drop to RideHeight and the setup ride-height range. The R&D upgrade blocks carry the team's ceilings from `tools/rnd.py`: the Engine Power deltas scaled to the supplier's bhp (× bhp × 745.7 / 53988), the fuel-consumption multipliers at 12/15 (works) or 8/15 (customer) of their stock effect, and the weight-reduction mass set to the team's weight target.
- Axles: the physics takes the wheel positions from the model (`UseModelPositions`). The handling fits the car to the car pack's model (`MODEL_AXLES`, its `wheel_fl` and `wheel_bl` nodes) against the donor's 2026 model (`DONOR_AXLES`): `LocalRestPositionX/Z` are the model's, MOIX and MOIY also scale by (1990 wheelbase / donor wheelbase)², Chassis `COMZ` (and F1Tuning `ChassisCOMZMin/Max`) and FuelTank `COMZ` keep their fraction of the wheelbase (the donor's 54–55 % front), and `F1Plank` keeps the donor's distance from each axle to the end of the plank (table above). The Williams, Dallara, Lotus and Brabham models sit 0.2 m off centre; with the 2026 `COMZ` those cars carried 63–65 % on the front axle.
- Engine sounds (`db/audio.json`, `tools/build_audio.py`, `tools/audio.py`): a car's engine sound is the `audio.audio_car_data.<switchName>` row named by the AudioF1Car `switchName` in its carmodel layer. The AudioF1Car setup (0x1409f4fa0) also sets the label's Wwise ID (0x1449087e0, case-insensitive) as the engine switch state, so a label the soundbanks don't hold is silent, while the database lookup is case-sensitive. The car pack gives each 1990 car its donor's label in other letter case (`F1_M_<name>_26`, such as `F1_M_mclaren_26`; Larrousse `F1_M_WILLIAMS_26` and Dallara `F1_M_HAAS_26`, apart from Williams and Arrows): the donor's switch state and the car's own row. `db/audio.json` adds those 13 rows (put-record, the donor's values) with `bundle_name` `v12_1990.bdl` (Ferrari, Lotus, Larrousse), `v10_1990.bdl` (McLaren, Williams), `v8j_1990.bdl` (Leyton House, Brabham) or `v8_1990.bdl` (the Ford teams), plus four `audio.audio_bundle_settings` rows for those bundles (the registry 0x1409f1b50 lists every row; the loader 0x144528310 has no format check and reads the pack's format 106 with its pre-116 header layout). `operations.files` adds the four `.bdl` files from the F1 25 Engine Sound Pack under those new names. No stock audio row or bundle changes. Option `engine_sounds` 2026 points the 13 rows at the donor's stock 2026 bundle instead.
- B-spec cars: every team also has `<token>_1990b` (package `f1_1990b`, folder `f1_1990_vehicle_b_specs`), a full copy of its A row with the same car folder, so the same AI data, handling archive and common.erp; only the model archive differs. The B rows and the `f1_1990b` package carry `available True`: the startup vehicle loader (0x142505f30) only registers available vehicles, and the content pass (0x1413e4cf0) makes available only the vehicles an entrant list names, which no B car is. `native/f1_1990_bspec.dll` (option `bspec_cars`) moves a team's career entrant rows to the B vehicle once its R&D upgrade `f1990_bspec*` is Completed, checked on career load and every hub visit, 1990 careers only (`/mnt/work/.f1-1990/addon/native-requests.md` N11, `/mnt/work/.f1-1990/v2/rnd-bspec-interface.md`).

```sh
python3 addons/f1-1990/tools/build_cars.py --game "<pristine F1 25 copy>"          # compare with the shipped files
python3 addons/f1-1990/tools/build_cars.py --game "<pristine F1 25 copy>" --write  # replace them
```

`--game` must hold stock 2026 team archives: a game folder with a test build installed carries modified handling files. `/mnt/work/.f1-1990/data-physics/fakegame-full` is a pristine copy.

## Car pack (carpack/)

`carpack/` holds the car models of the `f1_1990` vehicle package. Private test content, not for release: the Codemasters classics (F1 2019/2020, from Jefry), ported with Jefry's F1modding3dtoolV1. Seven teams run a classic body recoloured in flat 1990 colour zones (base colours and the main sponsor blocks, no logos; specs in `/mnt/work/.mp45b/classic/zones/<team>.json`, built by `classic_layout.py --zones`). The archives are built locally and never committed (`carpack/erp/` and `carpack/video/` are ignored).

What it adds, all under new paths, nothing replacing a game file:

- `2025_asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>.erp`: the 13 cars, 13-inch rims, per-driver number plates, LOD 0–5 from the body's classic LOD 0–5, a far LOD 6 of the classic LOD 5 body plus a shadow caster cut from its classic LOD 3 (`classic_layout.py --lods 0,1,2,3,4,5,5 --shadow-lod 3`; `build_pack.py` fails on a shadow caster over 40,000 triangles), detail, wheel and steering-wheel textures of at most 1024 px, and the pod camera (T-cam) just ahead of each body's airbox with a 100° field of view (`POD_CAMERA` and `POD_FOV` in `carpack/tools/build_pack.py`; the offset and side pod cameras move with it and keep their own field of view).
- `2025_asset_groups/f1_1990_vehicle_package/teams/common.erp`: the package's shared resources, from the stock 2026 common.erp, with the 2026 tyre meshes reshaped for 13-inch rims (bead 0.175 m, outer radius 0.351 m front and 0.354 m rear, the stock 2026 radius; dry and wet, stock 2026 tyre materials and textures, no deformation data; `/mnt/work/.mp45b/porter_a/tyres_cm13.py`). The tyre set is renamed with the rest of the archive, to `eaid://f1_1990_vehicle_package/shared/tyres/`, and is the archive's only tyre set.
- `2025_asset_groups/f1_1990_vehicle_package/tyrecompounds/tyrecompounds.erp`: the 2026 compound file once, as `eaid://f1_1990_vehicle_package/tyrecompounds/f1_1990_compounds.vcmpd`, with its 16 compounds renamed from `F1_2026_*` to `F1_1990_*`. The `fom_car` handling files left in the car archives and in common.erp have `TyrePhysicsGroup` `F1_1990`.
- `2025_asset_groups/f1_1990_vehicle_b_specs/teams/<folder>/wep/<folder>.erp`: the 13 B-spec cars (the A body with gold rims, `gridc13b_*`). Their own resources carry the B package path; references to the shared resources and to the car's handling file point at the A package, so a B group lists the A common.erp and the A handling archive.
- Runtime asset groups (`carpack/assets/cars.json`): `asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>.assetgroup` listing common.erp, the car archive and `<folder>_handling.erp` (`cars/handling.json`), and the tyre-compound group. Each takes its flags from the matching 2026 group.
- `carpack/db/ai.json` (written by `tools/ai_data.py`): the `ai.asset_data.f1_1990_vehicle_package` rows and the switch of the `f1_1990` and `f1_1990b` packages to `ai_package_folder` `f1_1990_vehicle_package`: they read the pack's own AI archives.
- `videos/f1_1990_intro.bik`: the intro video that opens a new 1990 Driver Career (`native/f1_1990_intro.dll`, option `intro_video_1990`), from `carpack/video/f1_1990_intro.bik` (ignored, like `carpack/erp/`). Bring your own video: the build fails without that file.

| Team | Folder | Model | Source build |
|---|---|---|---|
| McLaren | mclaren | CM c_mclaren_90 | gridc13_mclaren |
| Ferrari | ferrari | CM c_ferrari_90 | gridc13_ferrari |
| Williams | williams | CM c_williams_92 | gridc13_williams |
| Benetton | lotus | CM c_benetton_94 | gridc13_lotus |
| Tyrrell | mercedes | CM c_jordan_91, Tyrrell colours | gridc13_mercedes |
| Lotus | redbull | CM c_mclaren_91, Lotus colours | gridc13_redbull |
| Leyton House | force_india | CM c_jordan_91, Leyton House colours | gridc13_march |
| Larrousse | myteam_ferrari | CM c_ferrari_90, Larrousse colours, on the Williams donor, stem `larrouss` | gridc13_larrousse |
| Ligier | sauber | CM c_mclaren_90, Ligier colours | gridc13_ligier_prost |
| Minardi | toro_rosso | CM c_jordan_91, Minardi colours | gridc13_minardi |
| Brabham | gm_cadillac | CM c_mclaren_91, Brabham colours | gridc13_brabham |
| Dallara | myteam_honda | CM c_williams_92, Dallara colours, on the Haas donor, stem `dall` | gridc13_dallara |
| Arrows | haas | CM c_ferrari_90, Arrows colours | gridc13_arrows |

### Building

```sh
python3 addons/f1-1990/carpack/tools/build_pack.py            # all cars, common.erp, tyre compounds, carpack/assets/cars.json, carpack/assets/ai.json
python3 addons/f1-1990/carpack/tools/build_pack.py --ops-only # only carpack/assets/cars.json, carpack/assets/ai.json and carpack/db/test.json
python3 addons/f1-1990/carpack/tools/build_pack.py --bspec-only # only the B-spec archives
python3 addons/f1-1990/carpack/tools/build_pack.py --common-only # only common.erp and the tyre compounds
```

`build_pack.py` renames each porter output from `eaid://f1_2026_vehicle_package/` to `eaid://f1_1990_vehicle_package/` byte for byte (same length), leaves the bare `.mipmaps` sidecar paths on the stock 2026 files, drops the car's own handling file (the f1-1990 addon ships it per option), and sets the AudioF1Car `switchName` in each carmodel layer to the car's 1990 engine audio label (`addons/f1-1990/tools/audio.py`: the donor's label in other letter case, such as `F1_M_mclaren_26`, which keeps the donor's Wwise switch state and names the car's own `audio.audio_car_data` row). Larrousse and Dallara sit on My Team folders for their AI data but were built on team donors: their resources carry a stem of the donor's length, and their World, event graph and layers are also stored under the folder name. Every instance of each carmodel and carshowroommodel layer, A and B-spec, gets a `Tyres` override naming the package's tyre set (`tyre model`, `tyre wet model`, `material` and `wet material`, `TYRE_PATHS` in `carpack/tools/build_pack.py`); the car templates (`F1CarModern2026`, `F1ShowRoomCarModern2026`) name the stock 2026 set. The build fails if any `eaid://f1_2026_vehicle_package/` reference or `f1_2026_vehicle_package/shared/tyres/` resource survives. Sources default to `/mnt/work/.mp45b/porter_a/tries` (`--tries`, `--common`, `--game`).

The intro video:

```sh
nix shell nixpkgs#ffmpeg nixpkgs#wine64 nixpkgs#xvfb-run nixpkgs#p7zip -c \
    addons/f1-1990/carpack/tools/intro_video.sh <input video>   # writes carpack/video/f1_1990_intro.bik
```

`intro_video.sh` scales the input to 1280x720 (letterboxed; `WIDTH`, `HEIGHT`), resamples it to 29.97 fps and 48 kHz stereo, and encodes it as Bink 1 at `RATE` bytes per second (1500000) with `radvideo64.exe binkc` and `binkmix` under Wine. The game plays a `.bik` as Bink 1; Bink 2 encoding needs a licence. `RADVIDEO` points at an installed `radvideo64.exe`; otherwise the script downloads the free RAD Video Tools from radgametools.com into `~/.cache/f1-1990-intro`. RAD Video Tools are for non-commercial use only.

### AI data

`carpack/tools/ai/build_ai.py` writes the 1990 cars' per-track AI archives, `carpack/erp/ai/<track>/<folder>.erp`, for the 16 calendar tracks (`tools/career.py` ROUNDS) and the 13 car folders. `carpack/assets/ai.json` registers them: per archive a `file.put` to `2025_asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.erp` and a `group.register` of `asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.assetgroup` with the flags of the matching `f1_2026_vehicle_package` group. A full `build_pack.py` run fails at the end when one of those archives is not built.

```sh
nix shell --impure --expr 'with import <nixpkgs> {}; python314.withPackages (p: [p.numpy p.scipy])' -c \
  taskset -c 2-7,10,11,14-31 python3 addons/f1-1990/carpack/tools/ai/build_ai.py \
    --game "<F1 25 folder>" --swofty-ai <feat/swofty-ai checkout>/tools/swofty-ai
python3 addons/f1-1990/carpack/tools/ai/build_ai.py --ops-only      # only carpack/assets/ai.json
python3 addons/f1-1990/carpack/tools/ai/build_ai.py --report-only   # index.json, laps.json, checks.json from results.json
```

| Flag | Default |
|---|---|
| `--game` | `/mnt/gaming/SteamLibrary/steamapps/common/F1 25`, read only, with the stock `f1_2026_vehicle_package` track AI archives |
| `--swofty-ai` | `tools/swofty-ai` of this checkout (Swofty AI, branch `feat/swofty-ai`; Python 3.14, numpy, scipy) |
| `--handling` | `addons/f1-1990/cars/handling/removed_2026_1990` |
| `--tracks`, `--folders` | The 16 tracks, the 13 folders (comma-separated) |
| `--compounds` | `carpack/erp/tyrecompounds/tyrecompounds.erp`; when it is not built, the stock 2026 compounds renamed `F1_2026_*` to `F1_1990_*` |
| `--workers` | 12 |

Each archive is the stock 2026 archive of the same track and folder with every resource under `eaid://environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.<ext>`, in the stock order with the stock resource types:

| Resource | Contents |
|---|---|
| `.acpd .brpd .copd .crpd .thpd` | Swofty AI's car model (`carmodel.build_tables`, every axis) on the folder's 1990 handling VTF at the stock archive's downforce variation, in the stock tables' row order and axes (engine efficiencies, `x_mode`, copd and brake radii). Masses are the 1990 car's: light, VTF mass + 12 kg, + 110 kg fuel. Every `ers_mode` and `x_mode` row is the no-ERS row. Corner speeds are the stock 2026 copd of the folder × the model's 1990/2026 corner speed ratio at that radius, grip, mass row and tyre, the ratio held from the last radius where neither model car reaches its top speed, capped at the 1990 top speed. |
| `.vp_times` | Every set simulated with the 1990 tables on the track's stock AI line with the folder's stock brake line settings (`vp.build` with the plain set map Swofty AI uses for F2), the pit block on the stock pit route. Set masses are the 1990 car's (light × 1990/2026 mass ratio, VTF mass, VTF mass + 110 kg). Sample counts and the block hash are the stock file's. |
| `.fuel_burn_granular`, `.tyre_wear` | `strategy.build_fuel_burn` and `strategy.build_tyre_wear` with the 1990 tables on the stock line, F1 2026 fuel and wear coefficients. |
| `.ers_use_granular` | The stock layout, every sample 0. |
| `.ers_boost_data` | The stock entries with `baseERS`, `ers_delta_recorded`, `ers_amount` and `ers_burn_aim` 0, and `baseLapTime`, `lap_time_gain`, `baseFuelUsage` and `fuelUsage` scaled by the 1990/2026 lap and fuel ratio at full fuel. |
| `.vp_chars`, `.brakelinesettings` | The stock folder's. |

Outputs in `carpack/erp/ai/`, next to the archives:

- `index.json`: `track`, `folder`, `erp`, `target`, `group` and `template` of every archive written.
- `laps.json`, also written to `data/ai-laps-1990.json`: per track and folder the simulated flying lap with the 1990 tables (`lap1990`) and with the stock 2026 tables (`lap2026`) on the stock line and brake line settings, grip 1.0, the tables' middle mass, difficulty 1.0, and `model_ratio`, both cars from the car model; per track the median 1990/2026 ratio and the fastest 1990 car.
- `checks.json`: resource set and types against the stock archive, table decoding and monotonicity, ERS rows, 1990 laps slower than 2026, corner speeds at 50/150/300 m and 0–200 km/h for McLaren and Haas at Melbourne, and the total size.
- `results.json`: the raw per-archive results `--report-only` reads.

The tool exits non-zero when a job fails or a check reports an issue.

### Test option

`test_grand_prix` (default `off`, `carpack/db/test.json`) puts 1990 cars on the F1 2026 Grand Prix grid for testing: `ferrari` (the two Ferrari seats), `all` (all 22 seats, each on the 1990 car of the same AI folder), `myteam` (Larrousse on the Williams seats, Dallara on the Haas seats) or `bspec` (all 22 seats on the B-spec cars). Off leaves Grand Prix stock.

## AI (db/ai_difficulty.json, data/ai-laps-1990.json, tools/ai_data.py, tools/check_ai.py)

The game reads a car's AI data from `environment_package/tracks/<track>/ai/<ai_package_folder>/<asset_filename>/<asset_filename>.<ext>`: the path templates come from the row of `ai.asset_data.<ai_package_folder>` named by the car's folder (0x140614210), `<vehicle_package>` is the package row's `ai_package_folder` (0x142504a90) and `<team_name>` the vehicle row's `asset_filename`. A session asks for the AI data of the cars in it only (0x140612a80 walks the session's vehicles). On every track load (0x14065b570) the game reads `tracks.Track_N.ai_track_difficulties.<ai_package_folder>.{difficulty_ratio,mass_ratio,time,max_dificulty_no_balast_slider_position,max_slider_position}` for each available package and keeps the package only when all five keys exist. A car whose folder has no entry gets the track's first entry (Sess_Build_AiSystem, 0x140603a54), which is another package's difficulty.

- `db/ai_difficulty.json`: the five keys under `f1_1990_vehicle_package` on the 16 calendar tracks, new keys only: the `f1_2026_vehicle_package` values, with `time` scaled by the track's 1990/2026 lap ratio (`tracks.<track>.summary.median_ratio` of `data/ai-laps-1990.json`, the median over the 13 folders of the simulated 1990 lap over the stock 2026 lap). `difficulty_ratio`, `mass_ratio` (a ratio of the car's own mass) and both slider limits stay as in 2026. Without a ratio a track keeps the 2026 times and the build warns.
- `carpack/db/ai.json` adds `ai.asset_data.f1_1990_vehicle_package` (the 13 folders, the 2026 rows with `base_car` `<team_name>` in place of `fom_car`), points `f1_1990` and `f1_1990b` at `f1_1990_vehicle_package` and makes `f1_1990` available. `carpack/assets/ai.json` installs the 208 AI archives (16 tracks, 13 folders) and their asset groups. A build without the archives fails.

```sh
python3 addons/f1-1990/tools/ai_data.py [stock gamedatabases.2025.bin] [-v] [--check]
python3 addons/f1-1990/tools/check_ai.py <stock db> <built db> [--car-pack] [--assets addons/f1-1990/carpack/assets/ai.json --game <F1 25>]
```

`check_ai.py` fails when a stock `ai_track_difficulties`, `track_grip_modifiers`, `ai.asset_data` or vehicle package key changed, when a calendar track lacks a 1990 key or has another value than `ai_data.py` derives, when another track has one, when the package folders or the 1990 asset table do not match the build (`--car-pack` for a build with `carpack/db/ai.json`), or when the asset ops miss a track and folder, its 2026 template or the template archive.

## Driver Career (db/career.json, db/career-lang.json, tools/career.py, tools/check_career.py)

The 1990 career is a third career content next to the stock 2025 and 2026 ones. Its rows are full-key clones of the
2026 rows; no 2025 or 2026 row changes.

| Row | Name | Cloned from |
|---|---|---|
| career data group, in all eight data-group tables | `f1_career_1990` | `f1_career_2026` |
| mode row | `driver_career_1990` (statId 81, save container `driver_career_1990`, game setup modes `f1_1990`) | `driver_career_26` |
| championship | `F1_driver_career_1990` (statId 35, setup mode `f1_1990`, entrant list and setup data `f1_career_1990`) | `F1_driver_career_26` |
| event list | `F1_career_1990`: one season, five season events (no New Season Grid Reveal) | `F1_career_26` |
| track list | `F1_1990`: the 16 rounds in order (Hockenheim on Shanghai) | `F1_2026_16` |
| season lengths | `F1_1990_Long`, `_Medium`, `_Short`, all 16 rounds, no transfer window, setup mode `f1_1990` | `F1_2026_*` |
| track selection | `F1_1990` | `F1_2026` |

Content set `2026` lists `driver_career_1990` and `f1_1990`. The game registers a game setup mode only when the
content set lists it and an available mode row names it in `game_setup_modes`; no stock mode row names `f1_1990`, so the
1990 mode row does. Without it the 1990 championship has no setup mode, every 1990 team fails the championship's team
check and starting the career crashes. The career data group is the key the career slot list filters on, so 1990
careers have their own slots. The **Career year** setting runs the career in 2026 (default) or 1990. The game dates
each race weekend from the dataset's `weekend_day` and `weekend_month` in 2026 whatever the career year, so a career
clock started in 1990 never reaches a race. In 2026 `db/rules.json` moves each race to the Sunday on or before
its 1990 date (3 days earlier: Phoenix on 8 March), and `db/grid.json` moves each 1990 driver's birth year 36 years on,
so ages match 1990. The career runs its rounds by each event's own date (`schedule.all_track_events` `date_time`, the
stock 2026 weekends), so `native/f1_1990_wizard.dll` moves the 1990 career's calendar to the 1990 days in
`data/calendar-1990.json` (written by `tools/career.py`) and the season runs Phoenix first (see `native/README.md`).

The Choose Driver screen finds its F1, F2 and Icons drivers through the content set's official entrant lists named
`F1CAREER_<suffix>`, `F2CAREER_<suffix>` and `ICONS_<suffix>`, with the suffix of the Career tile that opened the wizard
(`25`, `26`, and `90` for the 1990 tile). Content set `2026` gains `F1CAREER_90` (`f1_1990`), `F2CAREER_90` (the 2026
F2 list; the 1990 wizard has no F2 start) and `ICONS_90` (`f1_1990_driver_pool`); the stock rows stay as they are.

The 1990 wizard has no F2 start, team, season length, driver icons or driver type choice (`native/f1_1990_wizard.dll`,
see `native/README.md`). `tools/wizard.py --game <F1 25>` writes `erp/wizard.json`: the route
`career_wizard_driver_career_1990`, a copy of the content select's `career_wizard_driver_career_to_f1` that opens the
settings step instead of the season length step, and the copies that take Choose Driver's back to the settings step
and the settings step's back to the Career tiles. Only the 1990 wizard fires them; stock routes stay as they are.

Strings are new keys: `lng_f1990_career_content(_description)` for the third Career tile, `lng_f1990_season`,
`lng_f1990_championship_season(_loading_screen)`, `lng_f1990_trophy_drivers_champion`, `lng_f1990_season_length`, and
per round `lng_f1990_gp_<code>` (long Grand Prix name) and `lng_f1990_gp_<code>_short`. `career.TRACK_NAMES` and
`career.TRACK_DATES` give the keys and the 1990 dates per track for the `f1_1990` track dataset.

The agent stays Elena in data: her character row, contact and strings are shared by all careers, and no data names the
agent per career. `data/flavio-strings.json` holds, per language, every stock string that names her rewritten for
Flavio Briatore (sign-offs say Flavio, the intro subtitles Flavio Briatore); the native side applies them, and mutes
her voice, only while a 1990 career is loaded. The character row `flavio` (stat_id 105, names `lng_f1990_agent_*`) is
the 1990 agent for that switch; its message avatar is `message_flavio` (`tools/portrait.py`).

```sh
python3 addons/f1-1990/tools/career.py <stock gamedatabases.2025.bin> <dir of language_<code>.tsv>
python3 addons/f1-1990/tools/check_career.py <stock gamedatabases.2025.bin> <built gamedatabases.2025.bin>
```

## R&D (db/rnd.json, db/rnd-lang.json, data/rnd-strings.json, data/rnd-art.json, tools/rnd.py, tools/check_rnd.py)

A 1990 career reads its R&D from `upgrades.1990`; 2025 and 2026 careers keep `upgrades.2026` and their rows stay stock.
The path is the `upgrades_path` of the career's ComUpgrades component, so the 1990 championship lists its own shared
component set, `shared_driver_career_1990_components`: ComUpgrades on `upgrades.1990`, ComResourcePoints and
ComAccolades on `f1_career_1990`, ComSchedule on `F1_career_1990`. Under `upgrades.1990` the engine supplier trees,
delivery schedules, regulation change schedules, R&D challenges and `general` are copies of the 2026 tables, with
each supplier tree linked to 1990 engine rows (`engines_linked_to`, `parent_engine` the first): ferrari → Ferrari,
honda → Honda, mercedes → Renault, redbull_ford → Ford HB and Ford DFR, audi → Lamborghini and Judd.
`general.ui_data_0.texture_array_dictionary_path` is the R&D UI package every card, icon and strip path starts with
(`#overrides.career.rnd_om_xm`); without it no R&D art shows in a 1990 career.

| Part | Rows |
|---|---|
| Trees | `trees.<token>` for the 12 tokens the game knows; `teams_linked_to_0.links` names each team's `_1990` and `_1990b` vehicles (Lotus and Larrousse share `redbull`) |
| Departments | Aerodynamics 6 upgrades, Chassis 6 (with `f1990_bspec_package`, the B-spec milestone), Engine 7 for the works teams (McLaren, Ferrari, Williams, Benetton) or 5, Durability 6. No DRS, ERS or turbo rows |
| Economy | 400 / 800 / 1400 RP, 7+7 / 14+10 / 24+14 days of research and development, no facility levels |
| Metadata | per team: the engine supplier's Engine Power ceiling (24 to 75 bhp), the weight target towards 500 kg, fuel consumption, a drag cost on downforce |
| Resource points | the `f1_career_1990` team rows, by the season pack's reputation: McLaren 1610 + 420 a week down to Ligier 750 + 220 |
| Text | `lng_rnd_f1990_*` names and descriptions; `data/rnd-strings.json` holds the 1990 department names on stock keys for the runtime string override; `data/rnd-art.json` maps each part to a stock card frame |

The qualifying line (`f1990_quali_rev_limit`, `f1990_quali_engine`) needs native request N12 to count only in
qualifying; without it, it is ordinary engine power. Design and numbers: `/mnt/work/.f1-1990/v2/rnd-design.md`.

## R&D look (erp/rnd_look.json, erp/rnd_look/rnd_look.erpk, art/rnd/, data/rnd-look.json, tools/rnd_look.py)

In 1990 careers each R&D department has a 1990 colour and a period icon; 2025 and 2026 careers keep the stock look.
Option `rnd_look_1990` (default on).

| department (code) | colour | icon (`art/rnd/`) |
|---|---|---|
| Aerodynamics (0) | Rothmans / Tyrrell blue `#2F74E8` | `aero.svg`: car front with the raised nose and anhedral wing |
| Chassis & Suspension (1) | Benetton green `#22A45D` | `chassis.svg`: pushrod coil-over on the carbon tub |
| Reliability & Gearbox (2) | Camel / Lotus yellow `#F6C21C` | `reliability.svg`: gear with an H-pattern gate and a spanner |
| Engine (3) | Ferrari / Marlboro red `#E2231A` | `engine.svg`: V engine with intake trumpets |
| Tyres (4, rnd-tyres-thermal) | orange `#FF7800`, rnd-tyres' own | `tyres.svg`: tall-sidewall slick on a 13-inch rim |

`erp/rnd_look.json` adds the uidb package `overrides.career.f1990_rnd`, a clone of the stock `rnd_om_xm` package whose
`rnd_dept_icons_small` (48 px white, alpha 255 + 128), `rnd_wheel_icon` (60 px coloured), `rnd_department_strips`
(1112 x 4) and `dept_card_icons` (148 px coloured with a glow) use new texture arrays: aero, chassis, maintenance,
engine, tyres and error_department, plus the stock regulation and shutdown wheel icons. The arrays are new resources in
`flow_career_hub.erp`, `flow_r_and_d.erp` and `flow_team_hq.erp` (where the stock ones are), so no stock array, list or
slice changes and nothing collides with arrays rnd-tyres-thermal grows. New resources change the archives' layout, so
the build writes those archives into the game files. `native/f1_1990_rndlook.dll` points the department atlases at
the package and writes the colours, only while a 1990 career is loaded (`native/README.md`).

```sh
python3 addons/f1-1990/tools/rnd_look.py --game <F1 25> [--baseline <launcher baseline>/<build>] [--preview <png>]
```

It needs Pillow, numpy, resvg (or nix), zstd and node. rnd_look.json must run before rnd-tyres-thermal's uidb edits,
which its `load_after` on this addon gives.

## Tyre icons (erp/tyre_icons.json, erp/tyre_icons/tyre_icons.erpk, tools/tyre_icons.py)

The 1990 compounds show 2018 Pirelli lettering: red SS (Supersoft), purple US (Ultrasoft), yellow S (Soft). A compound's icons come from two fields of its visual compound row: `icon_code`, a glyph of the HUD icon font (`art/fonts/icons.font` in `fonts_standard_icons.erp`: an embedded TrueType font for the code points, 306 glyph records with UVs, and a 2048x1024 BC3 colour page), and `texture_name`, a slice name in the compound texture arrays of `overrides.uidb`. `osd.car_settings_icons` also holds `<texture_name>_worn` (the game builds the name with `%s_worn`).

| resource | carried by | size | new entries |
|---|---|---|---|
| `icons.font` glyphs (text in the icon font) | `fonts_standard_icons` | 44x44 | U+E190 SS, U+E191 US, U+E192 S (records, TrueType glyphs `uniE190`..`uniE192`, cells at y 940 of the page) |
| `tyres.tyres_small` | `flow_pause`, `flow_garage`, `flow_post_race` | 28x28 | `SuperSoft_f1_1990`, `UltraSoft_f1_1990`, `Soft_f1_1990` |
| `tyre_allocation.compound_icons` | `flow_loading`, `flow_car_monitor`, `flow_options`, `flow_team_hq` | 44x44 | the same three |
| `tyre_allocation.key_icons` | `flow_st_hub`, `flow_loading`, `flow_post_race`, `flow_car_monitor`, `flow_options` | 28x28 | the same three |
| `tyre_allocation.tyre_icons` | `flow_loading`, `flow_options`, `flow_team_hq` | 144x144 | copies of the stock red `Soft_f1_26`, purple `SuperSoft_f2` and yellow `Medium_f1_26` tyres |
| `osd.car_settings_icons` (MFD) | `flow_post_race`, `flow_race` | 24x24 | the three, plus `_worn` (filled disc) |

The badges and glyphs start from the stock `Soft_f1_26` art (and `Soft_worn_f1_26`, the `~` glyph): the ring is recoloured to the accent of the stock `Soft_f1_26` (red), `Medium_f1_26` (yellow) or `SuperSoft_f2` (purple) badge, and the letter is replaced by the text in Formula 1 TV Bold (`formula1-tv-600_bold_web.font`), at the stock letter's height, or 86% of it for two letters. Slices are BC7 and glyph cells BC3, encoded with NVIDIA Texture Tools (`nvcompress`). The arrays grow in every archive that carries them, as `erp/badges.json` grows the team arrays; the existing slices and glyphs keep their bytes. `tools/tyre_icons.py` rebuilds the archive and the ops:

```sh
nix-shell -p nodejs zstd nvidia-texture-tools "python3.withPackages (ps: [ ps.numpy ps.pillow ps.fonttools ])" \
  --run "python3 addons/f1-1990/tools/tyre_icons.py --game <F1 25> --baseline <launcher baseline>/<build> --sheet <png>"
```

The garage tyre picker and the HUD, MFD, pause and timing tower tyre widgets reach a set's visual compound through the car's compound class, which the 1990 cars take from the 2026 cars; in a 1990 career `f1_1990_tyreclass.dll` gives each car the `f1_1990` class when it is created (option `tyre_class_fix`, native/README.md#tyre-class-f1_1990_tyreclassdll).

## 13-team lists (erp/ui26.json, ui/, tools/ui26.py)

At 26 cars (`when: cars 26`) the team lists get rows for 13 teams, in place in every archive that carries each resource.
Each new row is a clone of the last stock row, takes the list's per-row signals (`initialise`, `animate_in`,
`hide_selection_pip`) and binds a new scene row. The charts fit their rows into the height the stock layout gives 11
rows: their stacker gets a smaller `gap` and, where that is not enough, a vertical scale. Rows sit in name order.

| list | screen node | scene | fit |
|---|---|---|---|
| Constructor standings | `laptop_standings.ui` `constructor_list`: UILE `12` | `sl_constructor_stack.idf`: row `12` 0.4 below `11`, inside the stock panel | none |
| Vehicle Performance Comparison (email) | `laptop_overview.ui` `performance_list`: `entry_12`, `viewHeight` 13 | `r_and_d_performance_compare_stack_attach.idf`: `pill_11`, `pill_12` | gap 0.05 → 0.02, scale y 0.9007 |
| R&D charts (upgrades in progress, overview, research) | `laptop_r_and_d.ui`, replaced by rnd-tyres-thermal, which adds `entry_12` | `team_hq/research/perf_stack.idf`, `rnd_tree/rnd_perf_stack.idf`: `pill_12` | gap 0.1 → 0.05, scale y 0.9785 |
| Part fitting chart | `laptop_fit_upgrades.ui` `performance_chart`: `entry_12`, `viewHeight` 13 | `team_hq/research/perf_stack.idf` | as above |
| Team select chart (new career, HQ) | `driver_career_team_select.ui`, `team_hq_driver_career_team_select.ui` `performance_chart`: `entry_11`, `entry_12`, `viewHeight` 13 | `dc_ts_perf_panel.idf`: `pill_11`, `pill_12` | gap 0.16 → 0.066 |
| Team select carousel | | `dc_team_select.idf`: `carousel` → `1x1_sqr_carousel_team13.idf`, added to the four archives that carry `dc_team_select.idf` | `item_spawner` gap 0.205 → 0.12, scale 0.8932 |

The stock carousel scene, which `common_front_end.erp` also carries and the fan zone screen uses, is unchanged.

Driver standings already scroll 26 drivers through 13 rows. `tools/ui26.py` rebuilds `ui/` and the ops from the stock archives:

```sh
python3 addons/f1-1990/tools/ui26.py --game <F1 25> [--baseline <launcher baseline>/<build>]
```

It checks that each edited scene round-trips with valid element ids, node lookups and scene ids, that each screen round-trips, and that only the added rows, the lists' signals and their `viewHeight` differ from stock.

## Agent portrait (erp/portrait.json, erp/portrait/portrait.erpk, tools/portrait.py)

`tools/portrait.py` cuts Flavio Briatore's head and shoulders out of a photo (GrabCut for the body, a fitted background
plate for the hair), frames it like the stock portraits and encodes it as a 256x256 single-mip BC7 slice. The slice is
appended as `message_flavio` to the character message-avatar array
(`overrides.characters.message_avatars.texarray`, 87 stock slices), and a `patch-pkg` clones the list entry in
`databases.erp: overrides.uidb`. The four archives that carry the array (`flow_career_hub`, `flow_customisation`,
`flow_st_hub`, `flow_team_hq`) get the grown array by `put-resource`; a grown array changes the archives' layout, so
the build writes them into the game files. Stock slices, Elena's included, are unchanged.

```sh
python3 addons/f1-1990/tools/portrait.py --game <F1 25> --source <photo> [--baseline <launcher baseline>/<build>] [--preview <png>]
```

`check_career.py` fails when a key of a stock 2025/2026 career row, points preset, track dataset, content set or car
group changed (other than the row counts, content set 11's two lists and the agent's name fields), or a 1990 row is
missing.
