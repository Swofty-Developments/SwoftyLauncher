# Docs claims that something has not run in game (1.7.1 overnight inventory)

Format: file:line | feature | what a test observes | suite/test (A = automated, M = manual or needs RE, S = needs the session env)

## Car physics and handling
- docs/std/systems/car-physics.md:9 | live car physics (`cars::live`, layers, `car.profile`) "nothing in it has run in game yet" | `[cars] live car operations on: N fields`, a `[cars] car N: K car.profile layer(s)` line, no `<field> off: needs` for the profile's fields, no crash | car_physics/profile_layers_apply (A)
- docs/std/systems/car-physics.md:368 | `cars::live` fields "none of them is tested in game yet", only the KB build | `car[player].brake_ecu.brake_bias` read inside the profile's value after the layer, `f1_aero.total_drag` read | car_physics/profile_layers_apply (A)
- docs/std/systems/car-physics.md:368 | lift_scale, drag_scale, brakes.bias, engine.power_scale applied by a template `car.profile` | reads of brake_bias, a log line per layer, a sampled gear series still rising | car_physics/profile_layers_apply (A)
- docs/std/systems/car-physics.md:368 | DRS, fuel-mix and setup controls, career R&D upgrades, car index to participant | `session.start` plus reads in a race; no observable in a log | M (needs a fn test DLL)
- docs/std/systems/car-physics.md:383 | `tyres.*.grip_scale`, `refit_tyres`, fuel modes stay off | `[cars] car.profile ...: fuel_modes off` and no write | car_physics/off_fields_stay_off (A)
- docs/std/systems/cars.md:500 | `cars::live` set/get on 29 fields "experimental, untested in game" | a live `set`/`read` round trip through a test DLL (`cars::live::set`) | M (needs a fn test DLL; the agent's own `set` covers the 14 agent fields in car_physics/agent_set_read_roundtrip)
- docs/std/systems/cars.md:496, car-physics.md:396 | `vehicle.handling` records "stored, not applied" / applying at build is experimental | `[registry] vehicle.handling: N record(s)` line and whether it still says "applies nothing" | car_physics/handling_record_logged (A)
- docs/std/systems/cars.md:501 | who sets the per-car damage switches, tyre temperature model, compound switching | none observable | M (RE)

## AI
- docs/std/systems/ai.md:309 | `ai.controller.update` point "implemented, waiting on verification" (T145: slot 1 called every frame) | `[ai]` or `[core]` mixin line for the controller point, AI cars finish laps with the point armed, no crash | ai/ai_session_grip_site_armed (A)
- docs/std/systems/ai.md:307 | AI difficulty slider scale, grip index source, AI path builder unknown | none observable | M (RE)
- docs/std/systems/ai.md:308 | a changed AI config applies at the next session load, not verified for a later session | two benchmark races back to back in one game start, `Sess_Startup_VehicleAiLoop` effect | M (needs two sessions in one run)
- docs/std/systems/car-physics.md:224 | `ai_grip_scale` 0.87 through `site_Ai_TableGripInput` | `[cars] car levers armed`, no `ai.grip_scale off`, AI lap completes | ai/ai_session_grip_site_armed (A)
- docs/std/systems/ai.md:317 | the AI table holds 30 cars | a 26-car benchmark with all cars finishing laps | grid/benchmark_24_car_box (A, 24)
- docs/std/systems/index.md:151, sessions.md:229 | grid size is one boot setting; boxes clamp to it | car[23] exists in a 24-car box | grid/benchmark_24_car_box (A)
- docs/std/systems/sessions.md:495 | box points and penalties through the standings lookup | none observable (RE) | M

## Sessions, Time Trial, UDP
- docs/std/systems/graphics.md:243 | replays and telemetry partial, UDP holds 22 cars, per-wheel values unverified | `[udp] first telemetry packet rewritten: format N` in a Time Trial, the capture holds the packet | udp/time_trial_udp_session and udp/time_trial_udp_pad (A, needs UDP on in hardware_settings_config.xml)
- docs/std/registry.md:365 | driver ids 208..16382, never 254/255/low byte 0xff so 2024/2025 UDP formats don't read a network player | the rewritten packet's ids in the capture | udp/time_trial_udp_session (A)
- docs/std/systems/power-units.md:314 | PU swap checked over UDP | `[udp]` first packet plus a read of the car's engine max_torque | M (needs a PU box and a decoder)
- native/src/core/udp_ids.cpp | stock ids on, `[udp] stock ids on: ... sendto slot(s)` | log line at boot | guards/boot_guards_armed (A)
- docs/std/testing.md:48 (what runs today) | `grand_prix`, `time_trial` environments planned | the `session` env | S: udp, tracks, grid suites

## Tracks and the track guard
- docs/std/content.md:373, 549, tracks.md:465 | the box's track list builder needs RE, a guard stops a track that doesn't fit | `[tracks] guard: <track> in <box> (Time Trial): refused (...)` and the popup, session does not load | tracks/guard_refuses_unfit_track (S, expects a refusal), tracks/guard_fits_pad (A)
- docs/std/systems/tracks.md:197 | Time Trial on added tracks crashes or freezes, so `time_trial` stays false | a Time Trial on an added track is refused, never loaded | tracks/guard_refuses_unfit_track (S)
- docs/std/systems/tracks.md:571 | track fuel (`track.per_package`) read from UDP `m_fuelInTank` at Monaco | a 5-lap race at Monaco with the ig11.checks addon, `[tracks] box fuel: ...` lines | tracks/monaco_fuel_per_package (A)
- docs/std/systems/index.md:120 | driver count ceiling may be higher than 254 | none observable | M
- docs/std/registry.md:200,206 | the four tracks registries are experimental (1.6.5) | `[tracks] seeded N track row(s)` and `[tracks] track-facts.json: N track(s)` | tracks/tracks_registries_seeded (A)

## Guards
- native pit_slots.cpp | pit slot guard `[pit] slot guard armed` and its skip line in a race with stops | boot line, race with pit stops, no crash | guards/boot_guards_armed, guards/race_with_stops_no_crash (A)
- native tyre_slots.cpp | tyre slot guard armed | boot line, a race on a year-box tyre class | guards/boot_guards_armed (A)
- native garage_crew.cpp | garage crew guard (a third car has no crew) armed | boot line, a 24-car race | guards/boot_guards_armed, grid/benchmark_24_car_box (A)
- native standings_guard.cpp | standings guard (round with no track data skipped) armed | boot line; a career standings round | guards/boot_guards_armed, saves/career_load_writes_index (A)
- native era_picker.cpp | session vehicle guard (>26 vehicles), setup mode guard, career id aliases | boot lines; `[era] id aliases: N team id(s)...`; `[era] career load: mapped N saved entrant id(s)` on an old-id career | guards/boot_guards_armed, saves/career_load_writes_index (A; the mapped line needs an old-id fixture: M)
- native save_splice.cpp | save splice armed (unclaimed save containers kept) | boot line | guards/boot_guards_armed (A)
- native track_select.cpp | track select guard armed | boot line | guards/boot_guards_armed (A)

## Saves
- docs/std/systems/career.md:836, 977, abi.md:254 | career saves (index, per-addon data, `career.saving/saved/missing`) experimental since 1.7.0 | `[saves] career <key> is in the career index (N career(s))` after a career load, careers/index.json written (`swofty careers`) | saves/career_load_writes_index (A)
- native saves | launcher saves: the profile save served from the data folder | `[saves] launcher saves are on for profile X`, `[saves] launcher saves armed`, sandbox lines in a test run | saves/boot_launcher_saves (A)
- docs/using/career-editor.md:75 | Career Editor edits marked Experimental (free agent into a seat) not fully tested | launcher side; `swofty test career` cannot press the editor | M
- docs/std/testing.md:48 | `advance`, `capture`, career `weekend_hub`/`session`, title alone: "not working yet" | `advance` steps in a career test; `capture` | saves/career_advance_try, saves/career_capture_try (A, expected to fail until fixed; tagged `known-broken`)

## Experimental feature gates (shape unproven, not part of this run)
- docs/std/launcher / reference: `file` runtime, `overlay`, `telemetry`, `shaders`, `postfx` experimental | enabling each through `experimental_features` | M, separate postfx/overlay runs

## Teams, logos, importer, content (RE-bound, not automatable here)
- docs/std/systems/teams.md:394 | logo timing: file route logs `the game opened this path before` | log absent after a team logo addon | M
- docs/std/content.md:184,548 | sound rows after boot | none | M (RE)
- docs/std/build.md:162, importer.md:233 | swapping an archive per session | none | M (RE)
- docs/std/systems/drivers.md:415 | ratings in Grand Prix read DriverStatisticsComponent | none | M (RE)
- docs/std/hooks.md:121 | `career.season.process` at L0 | none | M (RE)
- docs/std/systems/tyres.md:446,451 | wear and temperature writers, AI strategy; `Car_Tyre_SetWearFactor` experimental | none | M (RE)
- docs/std/systems/sessions.md:504 | pit lane bindings experimental, no std operation | none | M
- docs/std/systems/power-units.md:346 | the PU panel slot site experimental | none | M
- docs/guide/installation.md:13 | EA app build experimental | M (EA copy)
- docs/std/services.md:138 | service status line in ADDON SETTINGS | M (launcher UI)
