# F1 1990

Adds a 1990 Driver Career next to the 2025 and 2026 ones, and an F1 1990 box to Grand Prix and Time Trial (Grand Prix and Time Trial box below). 1990 careers live in the normal profile save, in their own slot list (data group `f1_career_1990`).

## Grid (db/grid.json, db/grid_standings.json, lang/grid.json, tools/grid.py)

The 1990 teams (`<team>_1990`), drivers (`f1990_<driver>`), their looks, holders and entrants, and the entrant lists `f1_1990`, `f1_1990_driver_pool` and `f1_career_1990` are new rows; no 2025 or 2026 row changes. The **Cars** setting (`cars`) is 22 (default: the 11 strongest teams) or 26 (adds Larrousse and Dallara; needs 26-car track layers). Every 1990 driver wears his team's 1990 suit and gloves (`registry/kit.json`, `db/kit.json`, `tools/kit.py`): 13 new suit and gloves rows with their own archives, worn only by the 1990 looks and team kits. Their pit and garage crews wear the same colours (`erp/crew.json`, `tools/crew.py`, option `crew_kit`). See `tools/README.md`.

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
- Compound class `f1_1990` (stat_id 12): the softer slot is the red Supersoft (`Supersoft_f1_1990`, C4), the harder slot the yellow Soft (`Soft_f1_1990`, C3) and the medium slot the purple Ultrasoft qualifying tyre (`Ultrasoft_f1_1990`, C5). Each row sets its own sidewall context (`SuperSoft`, `Soft`, `UltraSoft`), colour, HUD icon (`icon_code` U+E190, U+E192, U+E191) and pictures (`texture_name` `SuperSoft_f1_1990`, `Soft_f1_1990`, `UltraSoft_f1_1990`), the 2018-style SS, S and US icons below. Its frontend compounds are its own Soft and wet (`frontend_compound` `Soft_f1_1990`, `frontend_compound_wet` `Wet_f1_26`): a showroom car shows the class's frontend compound through the class's own model slots, and the `Hard_2018` of `f1_26` has none in this class. The badges keep SS, US and S; the names follow option `tyre_supplier` (`lng_f1990_tyre_soft`, `_race`, `_qualifier` with `_title` and `_desc`, in `db/rules_text.json`): Goodyear D, C and Q, Pirelli soft, medium and Q, or with 1990 suppliers SOFT RACE, RACE and QUALIFIER. Allocations `f1_1990` (Balanced, stat_id 14), `f1_1990_softer` (15) and `f1_1990_harder` (16), the track TyreData's softer, balanced and harder presets, give the same sets: `soft_sets` supersoft (soft race) sets and `soft_sets` soft (race) sets in P1, P2 and the race, and `quali_sets` ultrasoft (qualifying) sets in Friday and Saturday qualifying, default 2 (in 1990 each driver had two sets of qualifiers per qualifying session and no limit on race tyres). The player's choice among them picks the team's tyre supplier (`f1_1990_brand.dll`, Tyre suppliers below); the AI's picks change nothing. At session enter the game collects a car's usable sets of one compound (neither used nor handed back) into a 256-byte stack arena that holds 9 (0x14368fde0, 0x14f3d1720); a 10th crashed Friday qualifying at Phoenix (10 ultrasofts once FQ tyres had freed both sessions' sets). So `soft_sets` stops at 3 and `quali_sets` at 4, and `f1_1990_tyres.dll` hands back any set past the 9th usable one of a compound at every 1990 session start. Mandatory `f1_1990_mandatory`: one ultrasoft set in the shootout sessions. `no_tyre_allocation` turns the allocation screen off for this class.
- The track lighting probes also list `f1_1990` and gain `garage_12` (baked set `garage_10`, like stock `garage_11`), the 13th garage slot of a 26-car grid. The game reads the probe entry of the player's garage slot when a session or the hub loads, from a list of `garage_00` up to the first missing key.
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

The 1990 cars live in their own vehicle package, `f1_1990`. The package, vehicle, engine and driver head rows set the stat ids of `tools/stat_ids.json` (vehicles 509–534, packages 59 and 60, engines 34–40, heads 265–292), the ids the builds before launcher 1.6.11 gave them and 1990 careers saved. `tools/check_rules.py` checks them. Native code finds the rows by label. Its `resource_path` is `f1_1990_vehicle_package/teams/[FILENAME]/wep/[FILENAME]`. `db/cars.json` sets its `ai_package_folder` to `f1_2026_vehicle_package`; `carpack/db/ai.json` moves it to the 1990 cars' own AI data (see AI below). No 2026 archive, row or file changes. The models, the package's `teams/common.erp` (13-inch tyres) and its tyre compounds come from the car pack in `carpack/` (see Car pack below), which registers one asset group per car at runtime. Without the built car pack archives, a session with a 1990 vehicle hangs on load.

| Vehicle | Car folder (AI data) | Chassis | Engine | Power | Limiter | Mass | Wheelbase | Plank OffsetZ/ExtentZ |
|---|---|---|---|---|---|---|---|---|
| mclaren_1990 | mclaren | McLaren MP4/5B | Honda RA100E V10 | 690 hp at 13,500 rpm | 14,000 rpm | 601 kg | 2.934 m | +0.206/1.123 |
| ferrari_1990 | ferrari | Ferrari 641 | Ferrari 036 V12 | 680 hp at 12,750 rpm | 13,250 rpm | 570 kg | 2.865 m | +0.237/1.018 |
| williams_1990 | williams | Williams FW13B | Renault RS2 V10 | 660 hp at 12,800 rpm | 13,300 rpm | 616 kg | 2.789 m | +0.230/0.992 |
| benetton_1990 | lotus | Benetton B190 | Ford HBA4 V8 | 660 hp at 12,800 rpm | 13,300 rpm | 613 kg | 2.882 m | +0.238/1.026 |
| tyrrell_1990 | mercedes | Tyrrell 019 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 634 kg | 2.910 m | +0.246/1.040 |
| lotus_1990 | redbull | Lotus 102 | Lamborghini 3512 V12 | 640 hp at 13,000 rpm | 13,500 rpm | 617 kg | 2.758 m | +0.227/0.962 |
| leytonhouse_1990 | force_india | Leyton House CG901 | Judd EV V8 | 640 hp at 12,500 rpm | 13,000 rpm | 642 kg | 2.910 m | +0.241/1.040 |
| larrousse_1990 | myteam_ferrari | Lola LC90 | Lamborghini 3512 V12 | 640 hp at 13,000 rpm | 13,500 rpm | 610 kg | 2.865 m | +0.237/1.030 |
| ligier_1990 | sauber | Ligier JS33B | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 642 kg | 2.934 m | +0.253/1.068 |
| minardi_1990 | toro_rosso | Minardi M190 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 640 kg | 2.910 m | +0.244/1.043 |
| brabham_1990 | gm_cadillac | Brabham BT59 | Judd EV V8 | 640 hp at 12,500 rpm | 13,000 rpm | 650 kg | 2.758 m | +0.227/0.964 |
| dallara_1990 | myteam_honda | Dallara F190 | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 654 kg | 2.789 m | +0.230/0.987 |
| arrows_1990 | haas | Arrows A11B | Ford DFR V8 | 620 hp at 11,250 rpm | 11,750 rpm | 648 kg | 2.865 m | +0.237/1.024 |

Power and mass are the default `handling` 1990 values. The figures are the period ones for each engine (the Lamborghini 3512 at 640 bhp and 13,000 rpm, the Ford DFR at 620 bhp and 11,250 rpm with an 11,750 rpm limit); every limiter sits 500 rpm above the power peak.

- Vehicle rows: full copies (246 keys) of the donor folder's 2026 row, `vehicle_package f1_1990`, `hud_dial` from the option (default `f2`), `drs_capable False`, `equal_performance_vehicle` the row itself (Time Trial and equal performance sessions load the handling of that vehicle), `performance_tier` 0 (McLaren, Ferrari, Williams, Benetton), 1 (Tyrrell, Lotus, Leyton House, Minardi, Brabham, Arrows, Dallara) or 2 (Ligier, Larrousse), and the grid lane's strings `lng_f1990_team_<team>`, `lng_f1990_chassis_<team>`, `lng_f1990_engine_<team>`. The labels use the grid lane's tokens (`leytonhouse_1990`, `ligier_1990`). `engine` is the 1990 engine row `f1990_engine_<supplier>` (A and B rows).
- Engine rows (`db/cars.json`, `lang/engines.json`): seven `entrants.engines` rows, one per 1990 engine (`f1990_engine_honda`, `_ferrari`, `_renault`, `_ford_hb`, `_ford_dfr`, `_lamborghini`, `_judd`), copies of `Cosworth_F1_V8_2009-2013` (the build picks the stat_id). The rev lights are that row's thresholds × the engine's limiter / 18,000, so the lights end at the limiter: start 72 %, red 89 %, purple 94 % (just under the power peak), warning 99 %. Honda 10,110 / 12,440 / 13,220 / 13,840 / 14,000 rpm, Ford DFR 8,490 / 10,440 / 11,100 / 11,620 / 11,750. `revDial*` stay 0 as on every stock F1 row (only road cars set them). `supplier` is the stock Honda, Ferrari and Renault string, or `lng_f1990_engine_supplier_ford`, `_lamborghini`, `_judd`.
- Package row: a copy of `f1_2026` with the fields above, the 2026 cockpit halo flag (True), the 2026 animation, crew, garage, pit-stop and pit-wall groups, and `is_26_aero_regulations False` when `classic_aero` is on.
- Car setups: `car_setup.presets.vehicle_package_setups` rows `f1_1990_vehicle_package` and `f1_1990_vehicle_b_specs`, full copies of the `f1_2026_vehicle_package` row (420 keys: `ai` and `player` setups at five downforce levels). On session start `Car_Load_SetupPreset` (0x140c04fd0) finds a package's setups by folder 1 of its `resource_path`. A package without a row gets setups that ground the car, and its AI runs wide.
- Handling: each car's handling file is built from its donor's stock 2026 handling file and shipped as `cars/handling/<ers>_<tyre_size>/<folder>_handling.erp`; `cars/handling.json` puts the one matching the options at `f1_1990_vehicle_package/teams/<folder>/wep/<folder>_handling.erp`, which the car's asset group lists. No EnergyRecoverySystem section (or zeroed, option `ers`), naturally aspirated engine with the engine's power peak and limiter from the table (the stock 15-vertex rev grid scaled so its vertex 9, where the stock curve and the R&D engine deltas peak, sits at the power peak; engine braking on the same grid; ICE wear by revs and the optimal fuel revs scaled with it), mass = 575 kg × the team's weight scalar, option `handling`: `1990` (the default) scales every lift curve and wing lift coefficient by 0.5, sets Tyre GripMultiplier 0.915 front / 0.925 rear, scales MOIX/Y/Z and the BrakeECU torques by the mass ratio; `1990_low` scales the lift by 0.33, sets GripMultiplier 0.83 / 0.85, keeps the donor's MOI and scales the brakes by the mass ratio × 0.9; both put 41 % of the weight on the front axle (1990 cars ran about 40/60; the 2026 donor 45/55), scale the front lift curves and front wing lift by 1.08 and the rear by 0.952 (the front's share of the downforce at a medium wing 37.6 % → 40.6 %, total unchanged) and each AntiRollConstant and its setup range by 0.85 front and 1.45 rear (front roll stiffness share 79.5 % → 72.6 % at the base setup), scale FFBScale by 1.3, take a stronger slipstream and dirty air (the F1Aero `m_aeroSlipStreamAirDensityScale*` values are the air density a full tow leaves to the body drag, front wing, rear wing and body lift; the game blends each to 1 by the clean-air share, 0x1446dceb0): Drag 0.8 → 0.65 (1.75 times the 2026 tow at every gap), FrontWing 0.76 → 0.6, RearWing 0.82 → 0.75, Lift 0.91 → 0.85, and `m_aeroSlipStreamCarWidth` 1.5 → 1.25; and use period power, mass and drag (the engine's power from the table, 620 kg × the team's weight scalar, drag × 0.96 on top of the team scalar (the 2026 donor's wing drag drops to × 0.5 front and × 0.3 rear with its wings open on the straights, 0x1446dceb0; the 1990 car keeps its wings closed), about 320 km/h for a 690 hp car with a Monza wing and 305 km/h for a 620 hp car, the 620 kg being 500 kg dry, a 72 kg driver and 48 kg for the period fuel the game's fuel model leaves out) (archives `cars/handling/<ers>_<tyre_size>_<handling>/`); `modern` scales MOIX/Y/Z by the mass ratio, uses 650 hp × the team's power scalar, drag × its drag scalar, no moving front wing, no DRS hardware, tyre physics group `F1_1990`, or `F1_1990P` for the Pirelli teams (Tyrrell, Minardi, Brabham, Dallara) (`tools/tyres.py` writes `F1_1990_*` and `F1_1990P_*` compounds, so no stock compound file shares their names), the donor's tyres (option `tyre_size` 2026, the default; the car pack draws the 13-inch rim with a tyre of the 2026 outer radius) with the car 30 mm higher (RestLength and its setup range + 30 mm; MinHeight, BumpStopHeight and the HeightTo* splines − 30 mm; RideHeightOffset, RideHeight and the setup ride-height range + 30 mm, so the aero reads the donor's ride heights), or with `13in` 13-inch tyres (TyreRadius 0.323/0.330, AlloyRadius 0.182, damage graphs remapped, final drive scaled), each axle's suspension height frame (MinHeight, MaxHeight, BumpStopHeight, the HeightTo* splines) moved down by the radius drop plus 30 mm and RideHeightOffset raised by the same, so the car sits 30 mm above the 2026 ride height with the 2026 travel and aero ride heights; `13in_raised` moves the frame by the drop more (30 mm plus 2.4–2.8 cm); RideHeight and the setup ride-height range carry the lift. Kerbs (every value but `stock`): BumpStopHeight, MaxHeight and RestLength (and its setup range) + 15 mm, so the static car keeps its height with 15 mm more travel before the bump stop (front 38 mm, rear 95 mm, less the static load) and the bump stop's own range kept; MinHeight − 10 mm of extra droop; BumpStopRisingSpring × 0.25 (front 1e7, rear 1.5e7), BumpStopRisingDamping × 0.5 and FastBumpDamping × 0.6 (front 1680, rear 1140; slow damping and springs stay the donor's). The ConvexHull vertices keep their fraction of the wheelbase, moved onto the 1990 model's axles, so the 2026 hull's floor, nose and wings stay under the shorter 1990 car. F1Plank OffsetY stays −0.208: it moves with the body, so the raise lifts it with the floor. The R&D upgrade blocks carry the team's ceilings from `tools/rnd.py`: the Engine Power deltas scaled to the supplier's bhp (× bhp × 745.7 / 53988), the fuel-consumption multipliers at 12/15 (works) or 8/15 (customer) of their stock effect, and the weight-reduction mass set to the team's weight target. Gearbox: the Ferrari has 7 forward gears and every other car 6, laid out as on the stock F2 car (the ratios past the top gear 0, no GearUpSpeedMultiple from the top gear, no GearDnSpeedMultiple above it). 1st keeps the stock 1st gear's speed at the limiter, the top gear (ratio 1.0) reaches the limiter at 1.12 × the car's top speed with a Monza wing (`TOP_SPEED_KMH`, 342 to 360 km/h) and the gears between follow the stock progression; `modern` keeps the stock top-gear speed. The automatic gearbox upshifts at 72.5 % (1st) and 84 % (2nd) of the limiter speed and at 96 % from 3rd up, and downshifts at the stock share of the limiter in the lower gear, at least 5 % under its upshift. Shift times (clutch / gear delay / throttle cut, seconds): H-pattern cars up 0.1 / 0.05 / 0.1, down 0.12 / 0.06 / 0.1; Ferrari up 0.06 / 0.03 / 0.06, down 0.075 / 0.03 / 0.07; stock 2026 up 0.03 / 0.015 / 0.045, down 0.06 / 0.03 / 0.05.
- Axles: the physics takes the wheel positions from the model (`UseModelPositions`). The handling fits the car to the car pack's model (`MODEL_AXLES`, its `wheel_fl` and `wheel_bl` nodes) against the donor's 2026 model (`DONOR_AXLES`): `LocalRestPositionX/Z` are the model's, MOIX and MOIY also scale by (1990 wheelbase / donor wheelbase)². The centre of mass sits at −`COMZ`: the static axle loads (0x1446b5f60) put (−`COMZ` − rear axle z) / wheelbase of the weight on the front axle, so the stock 2026 car's +0.171 on ±1.7 m axles is 45.0 % front (the 2026 rules allow 44.5–46 %). Chassis `COMZ` (and F1Tuning `ChassisCOMZMin/Max`) puts 41 % on the front axle under `handling` 1990 and 1990_low and the donor's share under modern; FuelTank `COMZ` keeps its fraction of the wheelbase. `F1Plank` is placed from the centre of mass (the stock weight-redistribution upgrade adds the same 0.025 to `COMY` and to the plank's OffsetY) and keeps the donor's distance from each axle to the end of the plank (table above).
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
- `2025_asset_groups/f1_1990_vehicle_package/teams/common.erp`: the package's shared resources, from the stock 2026 common.erp, with the 2026 tyre meshes reshaped for 13-inch rims (bead 0.175 m, outer radius 0.351 m front and 0.354 m rear, the stock 2026 radius; dry and wet, stock 2026 treads, no deformation data; `/mnt/work/.mp45b/porter_a/tyres_cm13.py`). The tyre set is renamed with the rest of the archive, to `eaid://f1_1990_vehicle_package/shared/tyres/`, and is the archive's only tyre set. It also carries the 1990 sidewalls (see Tyre sidewalls below).
- `2025_asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>_tyres.erp`: the car's tyre material bindings, `eaid://f1_1990_vehicle_package/teams/<folder>/tyres/tyres.emb?context=<compound>` (9 dry contexts) and `tyres_wet.emb?context=wet`, the stock bindings with the sidewalls of one supplier. `carpack/erp/tyres/<supplier>/<folder>_tyres.erp` holds both suppliers' versions; option `tyre_supplier` picks one (`1990 suppliers`: the team's own, `All Goodyear`, `All Pirelli`). Each also holds both suppliers' bindings under their own names, `tyres_goodyear.emb`, `tyres_wet_goodyear.emb`, `tyres_pirelli.emb` and `tyres_wet_pirelli.emb`.
- The tyre compound archive the car pack's tyre-compound group lists comes from `tyres/<supplier>/tyrecompounds.erp` (Tyre suppliers below). The `fom_car` handling files left in the car archives and in common.erp have `TyrePhysicsGroup` `F1_1990`.
- `2025_asset_groups/f1_1990_vehicle_b_specs/teams/<folder>/wep/<folder>.erp`: the 13 B-spec cars (the A body with gold rims, `gridc13b_*`). Their own resources carry the B package path; references to the shared resources and to the car's handling file point at the A package, so a B group lists the A common.erp and the A handling archive.
- Runtime asset groups (`carpack/assets/cars.json`): `asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>.assetgroup` listing common.erp, the car archive, `<folder>_handling.erp` (`cars/handling.json`) and `<folder>_tyres.erp`, and the tyre-compound group. Each takes its flags from the matching 2026 group.
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
nix-shell -p nodejs nvidia-texture-tools "python3.withPackages (ps: [ ps.numpy ps.pillow ])" \
  --run "python3 addons/f1-1990/carpack/tools/tyre_art.py --sheet <png>"   # carpack/erp/tyres/tyre_art.erpk, before any common.erp build
  # tyre_art.py --sheet-only --sheet <png> draws the sheet from the built tyre_art.erpk
python3 addons/f1-1990/carpack/tools/build_pack.py            # all cars, common.erp, the tyre bindings, carpack/assets/cars.json, carpack/assets/ai.json
python3 addons/f1-1990/carpack/tools/build_pack.py --ops-only # only carpack/assets/cars.json and carpack/assets/ai.json
python3 addons/f1-1990/carpack/tools/build_pack.py --bspec-only # only the B-spec archives
python3 addons/f1-1990/carpack/tools/build_pack.py --common-only # only common.erp and the tyre bindings
```

`build_pack.py` renames each porter output from `eaid://f1_2026_vehicle_package/` to `eaid://f1_1990_vehicle_package/` byte for byte (same length), leaves the bare `.mipmaps` sidecar paths on the stock 2026 files except on `teams/<stem>/textures/` surfaces that declare more than 2048 px through a sidecar (livery, decal, details, wheel, steering): those lose the sidecar fragment and declare their embedded size, drops the car's own handling file (the f1-1990 addon ships it per option), and sets the AudioF1Car `switchName` in each carmodel layer to the car's 1990 engine audio label (`addons/f1-1990/tools/audio.py`: the donor's label in other letter case, such as `F1_M_mclaren_26`, which keeps the donor's Wwise switch state and names the car's own `audio.audio_car_data` row). Larrousse and Dallara sit on My Team folders for their AI data but were built on team donors: their resources carry a stem of the donor's length, and their World, event graph and layers are also stored under the folder name. Every instance of each carmodel and carshowroommodel layer, A and B-spec, gets a `Tyres` override naming the package's tyre meshes (`tyre model`, `tyre wet model`) and the car's own bindings (`material`, `wet material`; `paths` in `carpack/tools/tyre_sets.py`); the car templates (`F1CarModern2026`, `F1ShowRoomCarModern2026`) name the stock 2026 set. The build fails if any `eaid://f1_2026_vehicle_package/` reference or `f1_2026_vehicle_package/shared/tyres/` resource survives. Sources default to `/mnt/work/.mp45b/porter_a/tries` (`--tries`, `--common`).

### Tyre sidewalls

The 1990 cars run the sidewall lettering of their 1990 supplier on every compound, slick, intermediate and wet: Goodyear (McLaren, Ferrari, Williams, Benetton, Lotus, Leyton House, Larrousse, Ligier, Arrows) or Pirelli (Tyrrell, Minardi, Brabham, Dallara; `PIRELLI_TEAMS` in `carpack/tools/tyre_sets.py`, the same four as `tools/build_cars.py`). Goodyear's 1981–1992 Eagles carried white sidewall lettering, GOOD-wingfoot-YEAR and EAGLE, one of each per side; Pirelli's 1989–1991 tyres a white PIRELLI wordmark. Neither marked the compound on the sidewall (crews chalked the set and compound on the rubber); here the lettering takes the compound's colour, as the HUD icons do: red SS (soft race), yellow S (race), purple US (qualifier), the stock green intermediate and blue wet (`LETTERING` in `carpack/tools/tyre_sets.py`, the stock colour for the other compounds).

| Resource in common.erp | Contents |
|---|---|
| `texture_arrays/sidewall_logo_surface_goodyear.texarray`, `_pirelli.texarray` | the stock array with slice 0 R (slick lettering) and G (wet and intermediate lettering) replaced; B, A and slice 1 stock |
| `texture_arrays/sidewall_diffuse_1990.texarray` | the stock rubber without the barcode sticker |
| `materials/sidewall*_goodyear.material`, `_pirelli.material` | a copy of each of the 10 stock sidewall materials with the supplier's array, the 1990 diffuse and the compound's lettering colour (`me_logoColour`) |

`carpack/tools/tyre_art.py` draws the lettering and writes the three arrays (BC7, 1024x1024, 9 mips, NVIDIA Texture Tools) to `carpack/erp/tyres/tyre_art.erpk`; `build_pack.py` adds them and the materials to common.erp (`tyre_sets.add_sets`) and writes the per-car bindings. The car pack's tyre maps its 0.175–0.307 m sidewall onto the stock 18-inch band of the texture (texel radius 800 × radius + 213 against the stock 1537.5 × radius − 15.5), so the lettering is drawn at its physical size and resampled into that band. The Goodyear lettering comes from the sidewall of the ASR McLaren MP4/5B for Assetto Corsa (`--goodyear`, default `/mnt/work/.mp45b/gltf/asr_1990_mclaren_mp4-5b/textures/png/G9X_twall-r.png`) without its chalk and crayon marks; the Pirelli wordmark is the stock one at 1.3 times its stock size, at the top and the bottom of each side.

The intro video:

```sh
nix shell nixpkgs#ffmpeg nixpkgs#wine64 nixpkgs#xvfb-run nixpkgs#p7zip -c \
    addons/f1-1990/carpack/tools/intro_video.sh <input video>   # writes carpack/video/f1_1990_intro.bik
```

`intro_video.sh` scales the input to 1280x720 (letterboxed; `WIDTH`, `HEIGHT`), resamples it to 29.97 fps and 48 kHz stereo, and encodes it as Bink 1 at `RATE` bytes per second (1500000) with `radvideo64.exe binkc` and `binkmix` under Wine. The game plays a `.bik` as Bink 1; Bink 2 encoding needs a licence. `RADVIDEO` points at an installed `radvideo64.exe`; otherwise the script downloads the free RAD Video Tools from radgametools.com into `~/.cache/f1-1990-intro`. RAD Video Tools are for non-commercial use only.

### AI data

`carpack/tools/ai/build_ai.py` writes the 1990 cars' per-track AI archives, `carpack/erp/ai/<track>/<folder>.erp`, for the 16 calendar tracks (`tools/career.py` ROUNDS), the 12 other tracks of the box (`BOX_TRACKS`) and the 13 car folders. `carpack/assets/ai.json` registers them: per archive a `file.put` to `2025_asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.erp` and a `group.register` of `asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.assetgroup` with the flags of the matching `f1_2026_vehicle_package` group. A full `build_pack.py` run fails at the end when one of those archives is not built.

```sh
nix shell --impure --expr 'with import <nixpkgs> {}; python314.withPackages (p: [p.numpy p.scipy])' -c \
  taskset -c 16-31 python3 addons/f1-1990/carpack/tools/ai/build_ai.py \
    --game "<F1 25 folder>" --swofty-ai <feat/swofty-ai checkout>/tools/swofty-ai
python3 addons/f1-1990/carpack/tools/ai/build_ai.py --ops-only      # only carpack/assets/ai.json
python3 addons/f1-1990/carpack/tools/ai/build_ai.py --report-only   # index.json, laps.json, checks.json from results.json
```

| Flag | Default |
|---|---|
| `--game` | `/mnt/gaming/SteamLibrary/steamapps/common/F1 25`, read only, with the stock `f1_2026_vehicle_package` track AI archives |
| `--swofty-ai` | `tools/swofty-ai` of this checkout (Swofty AI, branch `feat/swofty-ai`; Python 3.14, numpy, scipy). The game puts the centre of mass at -COMZ (axle loads 0x1446b5f60), so `CarConfig.from_vtf` must read the front weight as `(-comz - zr) / (zf - zr)`; feat/swofty-ai ecdf1be4 reads `(comz - zr) / (zf - zr)` and needs that one-line change |
| `--handling` | `addons/f1-1990/cars/handling/removed_2026_1990` |
| `--tracks`, `--folders` | The 28 tracks (ROUNDS and `BOX_TRACKS`), the 13 folders (comma-separated) |
| `--compounds` | `tyres/historical/tyrecompounds.erp` (`tools/tyres.py`: Goodyear `F1_1990_*` and Pirelli `F1_1990P_*`, each car on its handling file's `TyrePhysicsGroup`); when it is missing, the stock 2026 compounds renamed `F1_2026_*` to `F1_1990_*` |
| `--workers` | 12 |

Each archive is the stock 2026 archive of the same track and folder with every resource under `eaid://environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.<ext>`, in the stock order with the stock resource types:

| Resource | Contents |
|---|---|
| `.acpd .brpd .copd .crpd .thpd` | Swofty AI's car model (`carmodel.build_tables`, every axis) on the folder's 1990 handling VTF in qualifying trim (the qualifier, code `uso` of the car's group, and the handling file's engine; outside qualifying `f1_1990_aiquali.dll` scales the AI's dry grip input to the tables by `AI_RACE_GRIP` 0.87, and `lap1990_race` is the lap at that table grip with the brake line modifiers at 1.0) at the stock archive's downforce variation, in the stock tables' row order and axes (engine efficiencies, `x_mode`, copd and brake radii). Masses are the 1990 car's: light, VTF mass + 12 kg, + 110 kg fuel. Every `ers_mode` and `x_mode` row is the no-ERS row. The model's drag is fitted to the 2026 cars, whose wings open on the straights (wing drag × 0.5 front, × 0.3 rear); for the 1990 car's closed wings it is scaled by (2026 model drag / 2026 open-wing drag) × (1990 closed-wing drag / 1990 model drag) at the variation's wing angles (`closed_wing_drag`, × 1.35 to 1.53). Corner speeds are the stock 2026 copd of the folder × the model's 1990/2026 corner speed ratio at that radius, grip, mass row and tyre, the ratio held from the last radius where neither model car reaches its top speed, capped at the 1990 top speed. Below radius 40 m the corner speeds are then scaled by 1 − 0.30 × ((40 − R) / 25)² (R ≥ 15 m, full 30 % at 15 m and below; `tight_corner_derate`, `TIGHT_*` in build_ai.py). The model's lap rises 4.1 % at Monaco, 0.9–1.1 % at Singapore, Montreal and Shanghai, 0.4–0.6 % at Monza and Suzuka and 0.1 % or less at Imola, Las Vegas, Hungaroring and Jeddah. |
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

## Grand Prix and Time Trial box (db/box.json, lang/box.json, tools/box.py, native/f1_1990_box.dll)

An **F1 1990** box sits in the Addons row of the Grand Prix and Time Trial era picker (order 1990, the stock F1 2025 tile art, title `lng_f1990_box`). `native/f1_1990_box.dll` registers it with `swofty::era::add_box`: the Grand Prix picker selects `f1_1990_gp`, the Time Trial picker `f1_1990_tt`.

| List | Cars | Contents |
|---|---|---|
| `f1_1990_gp` | 22 or 26 (`cars`) | the `f1_1990` grid: the same teams, cars, drivers, numbers and looks |
| `f1_1990_tt` | 13 | each team's first car and driver, Larrousse and Dallara included |

Both lists are new rows copied from `f1_2026` (season `season_2026`, leaderboard `classic`, the build picks their stat ids), list-appended onto the `swofty_f1` entrant setup mode and the `2025` and `2026` content sets, each with a `game_setup.scenarios` row under `swofty_f1` (statId 90 and 91). No driver, team or vehicle row is added. The `f1_1990` track layer, track data set and compound class also list `swofty_f1`; they cover season `season_2026` only, and no other `swofty_f1` list runs on season 2026, so a box session loads the 1990 track worlds (13 garages, 26 grid slots with `cars` 26), the 1990 tyres (C4 soft race, C3 race, C5 qualifier, the 1990 allocation presets), the 1990 pit loss and the SS / US / S badges. The data set and the class are default format only: a sprint weekend in the box has neither.

Box test (`native/src/box_gate.hpp`, decision in `box_decision.hpp`): a session is an F1 1990 box session when the setup's selected entrant list is `f1_1990_gp` or `f1_1990_tt` and the setup's game setup mode (`setup+0x2b0 -> +0xd8 -> +0x10`) is `swofty_f1` (a stale list under a Driver Career, My Team or stock mode never counts), whether or not the game holds a career entity (Grand Prix runs on the career machinery); a 1990 career takes the career path first. Rules in the box: the tyre class swap (`f1_1990_tyreclass.dll`), the race engine and the AI's qualifying grip (`f1_1990_quali.dll`, `f1_1990_aiquali.dll`, option `quali_engine_1990`, Qualifying and race engines below), the 1990 tyre allocation sets (`f1_1990_tyres.dll`), the player's tyre supplier choice with its Goodyear / Pirelli strings (`f1_1990_brand.dll`) and the slow-motion pit stops (`f1_1990_pitphases.dll`) follow the session. Time Trial and qualifying run the qualifying engine; practice and the race run the race engine. Power-unit wear and grid penalties are career rows only. Career-only features stay career-only: power-unit parts, R&D and B-spec cars, the 12-lap qualifying limit, the pit crew pace and session names.

The box sets `max_entrants` 26 on the `swofty_f1` entrant setup mode (copied from `f1` with 20), shared by every box: a list of up to 26 cars fields all its cars; lists of 20 to 22 cars (Classic Seasons) are unchanged and a 24-car list (F1 2012 Season) fields 24. Stock boxes use the `f1` mode and are unchanged.

Not covered: the pit crew training set is the `f1` one (the team `pit_lane.pit_times` rows do apply).

```sh
python3 addons/f1-1990/tools/box.py          # writes db/box.json and lang/box.json from db/grid.json
python3 addons/f1-1990/tools/box.py --check  # fails when either file is stale
```

## AI (db/ai_difficulty.json, data/ai-laps-1990.json, tools/ai_data.py, tools/check_ai.py)

The game reads a car's AI data from `environment_package/tracks/<track>/ai/<ai_package_folder>/<asset_filename>/<asset_filename>.<ext>`: the path templates come from the row of `ai.asset_data.<ai_package_folder>` named by the car's folder (0x140614210), `<vehicle_package>` is the package row's `ai_package_folder` (0x142504a90) and `<team_name>` the vehicle row's `asset_filename`. A session asks for the AI data of the cars in it only (0x140612a80 walks the session's vehicles). On every track load (0x14065b570) the game reads `tracks.Track_N.ai_track_difficulties.<ai_package_folder>.{difficulty_ratio,mass_ratio,time,max_dificulty_no_balast_slider_position,max_slider_position}` for each available package and keeps the package only when all five keys exist. A car whose folder has no entry gets the track's first entry (Sess_Build_AiSystem, 0x140603a54), which is another package's difficulty.

- `db/ai_difficulty.json`: the five keys under `f1_1990_vehicle_package` on the 28 tracks of ROUNDS and `BOX_TRACKS`, new keys only: the `f1_2026_vehicle_package` values, with `time` scaled by the track's 1990/2026 lap ratio (`tracks.<track>.summary.median_ratio` of `data/ai-laps-1990.json`, the median over the 13 folders of the simulated 1990 lap over the stock 2026 lap). `difficulty_ratio`, `mass_ratio` (a ratio of the car's own mass) and both slider limits stay as in 2026. Without a ratio a track keeps the 2026 times and the build warns.
- `carpack/db/ai.json` adds `ai.asset_data.f1_1990_vehicle_package` (the 13 folders, the 2026 rows with `base_car` `<team_name>` in place of `fom_car`), points `f1_1990` and `f1_1990b` at `f1_1990_vehicle_package` and makes `f1_1990` available. `carpack/assets/ai.json` installs the 364 AI archives (28 tracks, 13 folders) and their asset groups. A build without the archives fails.

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

The `f1_career_1990` career setup row (`career.career_setup.CareerSetupDataGroup`) sets `SpawnedF1` `car_1` to
`car_3` to `mclaren_1990`, `ferrari_1990` and `williams_1990`: the three cars behind the 1990 career setup steps are
the 1990 McLaren, Ferrari and Williams. The 2026 row keeps `gm_cadillac_2026`, `ferrari_2026` and `sauber_2026`.

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

## Tyre suppliers (tyres/, tools/tyres.py)

In 1990 Goodyear supplied most teams and Pirelli supplied Tyrrell, Minardi, Brabham, Dallara (BMS Scuderia Italia), Osella, EuroBrun and Coloni (rounds 1 to 8). Goodyear raced lettered compounds (B harder, C the regular race tyre, D the soft race tyre) and qualifiers that lasted a lap or two; Pirelli was often quicker over a qualifying lap and wore faster in the race. There was no mandatory stop and no compound rule.

The game names a wheel's compound `<TyrePhysicsGroup>_<physics code><F|R>` and finds it in the compound files of the loaded packages. The Goodyear teams' handling files carry `F1_1990` and the Pirelli teams' `F1_1990P` (`tools/build_cars.py`, `PIRELLI_TEAMS`). `tyres/<supplier>/tyrecompounds.erp` holds one compound file, `eaid://f1_1990_vehicle_package/tyrecompounds/f1_1990_compounds.vcmpd`, with the 16 stock 2026 compounds under both groups, and `tyres/compounds.json` puts the one option `tyre_supplier` names at `2025_asset_groups/f1_1990_vehicle_package/tyrecompounds/tyrecompounds.erp`:

| `tyre_supplier` | `F1_1990_*` (Goodyear teams) | `F1_1990P_*` (Pirelli teams) |
|---|---|---|
| `historical` (default) | Goodyear | Pirelli |
| `goodyear` | Goodyear | Goodyear |
| `pirelli` | Pirelli | Pirelli |

The class's three dry codes are scaled from the stock compound of the same code: grip on every Peak, PeakMin and PeakMax; wear on WearRate and the pressure WearRateMin/Max; fade on the WearGrip spline (1 − (1 − y) × fade, at least 0.3); heat on WorkEnergyRate and the pressure WorkEnergyRateMin/Max (warm-up). Every other code, the intermediate and the wet stay stock.

| Code (badge) | Goodyear | Pirelli |
|---|---|---|
| `uso` (US, qualifier) | Q: grip × 0.97, wear × 6, fade × 2, heat × 1.2 | qualifier: grip × 0.99, wear × 7, fade × 2.4, heat × 1.3 |
| `sup` (SS, soft race) | D: grip × 0.895, wear × 0.9 | soft: grip × 0.91, wear × 1.3, fade × 1.35, heat × 1.1 |
| `sof` (S, race) | C: grip × 0.895, wear × 0.85 | medium: grip × 0.91, wear × 1.25, fade × 1.3, heat × 1.1 |

The stock 2026 compounds share one Peak, so the grip factors alone set the gap between qualifiers and race tyres. In 1990 the race's fastest lap, set late on low fuel, was 4–8 % over pole: Monza 1:26.254 (Senna, lap 46) against 1:22.533 (+4.5 %), Monaco 1:24.468 (Senna, lap 59) against 1:21.314 (+3.9 %), Hungary 1:22.058 (Patrese, lap 63) against Boutsen's 1:17.919 (+5.3 %), Suzuka 1:44.233 (Patrese, lap 40) against Senna's 1:36.996 (+7.5 %). In the AI build's car model (`carpack/tools/ai/build_ai.py` laps at the tables' middle mass, front weight read at -COMZ; the race laps are `data/ai-laps-1990.json`) the race tyres put the best race lap 4.0–6.2 % over the qualifier lap on the 16 tracks: 4.4–4.8 % at Las Vegas and Monza, 4.2–5.1 % at Hungary and Monaco, 5.2–6.2 % at Suzuka. Las Vegas: qualifiers 1:40.0 (Ferrari) to 1:42.4 (Benetton), race tyres 1:44.4 to 1:47.1, and a full 110 kg tank about 2.5 % slower again.

Race engine: the handling files carry each engine's qualifying rating, the period power figures. In a 1990 career every car runs a race engine in practice and the race, about 2.75 % less power and a limit 300 rpm lower (`f1_1990_quali.dll`, option `quali_engine_1990`; Ferrari's 1990 race 036 gave 656 bhp at 13,200 rpm against 671 bhp at 13,600 for the qualifying 037, and from Hungary 681 against 700 bhp). Qualifying runs the handling file's engine, and the AI reads its qualifier tables there and its race tyre rows everywhere else (`f1_1990_aiquali.dll`).

The player's team runs the supplier of its weekend Tyre Allocation choice: Harder is Goodyear, Softer is Pirelli and Balanced the team's 1990 supplier (shown in a 1990 career as Goodyear, Pirelli and 1990 Supplier). `f1_1990_brand.dll` writes the chosen group into both team cars' live Chassis string at each 1990 session start and fits their tyres again (native/README.md). The sidewall look follows the launcher option only (Not done in tyrealloc-REPORT). The badges, the class and the allocation are the same for both suppliers. `f1_1990_tyres.dll` hands back every unused qualifier at the start of a 1990 race (option `qualifiers_quali_only`).

```sh
python3 addons/f1-1990/tools/tyres.py --game "<pristine F1 25 copy>"          # write tyres/
python3 addons/f1-1990/tools/tyres.py --game "<pristine F1 25 copy>" --check  # compare with the shipped files
```

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

## Car numbers (erp/numbers.json, erp/numbers/numbers.erpk, tools/car_numbers.py)

The car number on a Choose Driver or team-mate select card is the slice `nr_<car number>` of
`overrides.drivers.driver_numbers` (`driver_numbers_26` for a driver in a 2026 car; the 1990 cars use
`driver_numbers`). The game formats the key from the entrant's car number (0x1416a3630) and hides the number when no
slice has that name. F2 cards show no number. The stock array holds the numbers of the 2025 and earlier drivers, each
in its driver's lettering, so 1990 numbers 1-7, 10, 12, 16, 20, 22-24, 27 and 30 show that stock art.

`erp/numbers.json` appends a slice for each 1990 car number the stock array lacks: 8 (Modena), 9 (Alboreto),
11 (Warwick), 15 (Gugelmin), 19 (Nannini, Moreno), 21 (Pirro, Morbidelli), 25 (Larini), 26 (Alliot), 28 (Berger) and
29 (Bernard). No stock F1 card carries these numbers. Each slice is 80x80 BC7 like the stock ones: the number in
Formula 1 TV Bold (`formula1-tv-600_bold_web.font`), 46 px tall and at most 70 px wide, centred, with a gradient from
32% towards white at the top to 90% of the team colour at the bottom. The colour is the team's first colour, or its second
when the first is white or too dark for the card (Brabham and Dallara are white). The array grows in all three
archives that carry it (`flow_career_season_setup`, `flow_press_interview`, `screens/press_interview`); the stock
slices keep their bytes. `tools/car_numbers.py` rebuilds the archive and the ops:

```sh
nix-shell -p "python3.withPackages (ps: [ ps.numpy ps.pillow ps.fonttools ps.lz4 ])" \
  --run "python3 addons/f1-1990/tools/car_numbers.py --game <F1 25> --baseline <launcher baseline>/<build> --sheet <png>"
```

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
