# Port: core features gated on the KB build, Steam build 26566308

Lane `port-live` (v1.10.0), 2026-11-17. **Static only**: no game run, no debugger. Addresses are VAs with image base
`0x140000000`; "port" is Steam 26566308.

| build | file | PE timestamp | size | sha256 |
|---|---|---|---|---|
| KB (1.0.141.7865, Steam 25251167) | `/mnt/work/VSC/f1-modding/work/exe/F1_25.exe` (copy also in `/mnt/gaming/f1-builds/1.0.141.7865-build25251167/`) | 1789118616 | 408,063,872 | `d873a1c8…` |
| port (1.0.145.2472, Steam 26566308) | `/mnt/gaming/f1-builds/1.0.145.2472-build26566308/F1_25.exe`, the installed exe's copy | 1794312636 | 391,613,312 | `46009bd3…` |

Both are read only. Overlay `re/db/builds/steam-26566308.json` maps `.xtls` to `.tls$` and `.data1` to `.trace`; every
binding of the set resolves on the port (3,698: 3,689 by hint, 9 by scan, `bindverify.py --build steam-26566308`).

## Method

`re/tools/portcheck.py --port <port exe> --build steam-26566308` checks each gated feature's dependencies (its
`FEATURES` table):

- a **binding** resolves on both images with the core's rules. A function is compared instruction by instruction over
  its whole `.pdata` extent with rip displacements and branch targets masked, through the relocated body for a protector
  thunk. A code site is checked through its containing function, at the same offset. In a protector body, the
  protector hides some constants per build as `mov r32, K; sub r32, [rip+X]` with `[X]` a constant in the image.
  These are folded with `[X]` read from each file, and alignment `nop`s are dropped. Example: KB `or esi, 0x490120bd -
  [0x151966e08]` = `or esi, 0x20bd`, which is the port's plain immediate.
- a **field** passes when the KB instruction at its `xref:` (or another instruction with that displacement, never a
  stack slot) in the KB function has an aligned counterpart with the same base and displacement in the port's twin
  (`layoutcmp.py`'s alignment). A twin that a head pattern cannot find is now located by `sigport.py` (a call site or
  a protector thunk's jump).
- a **raw RVA** gets a `sigport.py` signature that matches once on both images, plus the same code or vtable slots on
  the port.

## Results

| feature | gate before | verdict on the port | change |
|---|---|---|---|
| car points (`car.built`, `car.reset`, `car.upgrades.replayed`, `session.pit`/`pit_stop`, `car.setup.applied`) | `car_live.cpp` `arm_car_points` | proven | `hooks/car_proofs.hpp` `HOOK_SITES` names Steam 1.0.145 |
| car levers (engine torque, AI grip, AI racecraft mixins) | `car_levers_mixins` | proven (sites and AI fields) | kept off: the layers they apply come through the car operations |
| live car operations | `car_live.cpp` `setup` | **not proven**, 4 fields (below) | off (`car_proofs.hpp` `OPERATION_FIELDS`: KB only) |
| live car wheel spin and slip (`wheels`) | `live_car_logic.hpp` | proven: `CarSuspension.wheels` +0x70, `CarWheel.spin_rate` +0x188, `slip_angle` +0x3b0, `slip_ratio` +0x3b4 | `live_car_logic.hpp` `WHEEL_FIELDS` |
| live car track wetness (`wetness`) | `live_car_logic.hpp`, `live_car.cpp` (hard-coded RVA) | proven, below | `TRACK_WETNESS`, and the global is found by signature |
| live car wheel surface and wetness, orientation, angular velocity | `live_car_logic.hpp` | not proven (no KB function found that reads +0x200, +0x280, +0x190 or +0x210 and ports) | off |
| game database edits after the database is built | `gamedb.cpp` `late_inplace` | proven | `RECORD_CLASSES` names Steam 1.0.145; record classes found by signature |
| Time Trial player name bridge | `player_name.cpp` `setup` | proven: `Game_PresenceNameCache_Lookup` 0x140816b10 = port 0x140816b60, 362/362 instructions equal, with the user id reads +8, +0x10, +0x39, +0x4c; the registry walk's functions and the four node vtables below | `copies.hpp` `TT_NAME_LAYOUT` names Steam 1.0.145; `REGISTRY_VTABLES` holds each build's vtables |
| crash symbol names | `crash.cpp` `KB_STAMP` | n/a | names from the binding set on any exe that `symbols.tsv` does not cover |

### Car points

| dependency | evidence |
|---|---|
| `site_Car_LiveCarBuilt` (rdi = car) | site +0x17d9 in 0x144628190 = port 0x14462aa00, 2641/2641 instructions equal |
| `Car_Systems_Reset_Thunk` (loader, arg) | body 0x1555bdf80 = port 0x154ce1490 after folding one hidden constant on each side |
| `Car_Upgrades_ApplyToVehicle` | 0x144742c70 = port 0x144745630, 510/510 |
| `site_Car_SetupTuningApplied` (rdi = F1Tuning) | site +0x21d1 in 0x140c11550 = port 0x140c11e90, 1902/1902 |
| `Game_Pitlane_SetVehicleStatus` (lane, vehicle, status) | 0x1434bc540 = port 0x1434beac0, 550/550; the handler's lane +0x68, entry +4 and +0x14 are this function's own chain |
| `CarLive.vehicle_id` +0xe8, `upgrades_allocator` +0x4a0 (`CAR_LOADER`), `CarComponentBase.car` +8 | same store in the car and component constructors (0x14462963e = 0x14462beae, 0x14462878f = 0x14462afff, 0x144748528 = 0x14474aee8) |

### Car levers

`site_Car_EngineDriveTorque`: site +0x511 in 0x1446e2290 = port 0x1446e4b00. 1499 of 1509 instructions are equal. The
other ten are the CarErs flag and mode reads, which moved by four bytes (+0x1a62..+0x1a68 to +0x1a66..+0x1a6c; Live
State's ERS mode is per build already). The lever reads none of them. `site_Ai_TableGripInput` (3413/3413) and
`site_Ai_BuildSystem_RecordPushed` (3761/3761) are equal, and so are all 84 `AiDriverConfig` fields and the
`AiSystem`/`AiCarRecord` fields.

### Track wetness

`g_Game_WeatherSurfaceState` 0x146ce91d8 ports to 0x146ceb1d8. The signature is
`48 8B 05 ?? ?? ?? ?? 48 85 C0 74 35 80 B8 8B 0A 3F 00 00` (riprel). Its match also reads the state's +0x3f0a8b
flag at the same displacement. Four readers of the wetness float `[reg + 0x3f0a38]` are equal on the port, each over
its whole `.pdata` extent: 0x14013c8d0 (438/438), 0x140158da0 (84/84), 0x1403612c0 = 0x140361300 (219/219) and
0x140a832a0 = 0x140a832f0 (96/96). The first reads the global's flags +0x3f0a8b and +0x3f0a8a just before the float.

### Game database map and loaded flag (assets/dbread.cpp, assets/gamedb.cpp)

`DB_MAP` +0xa8 and the Robin map's fields: `Gdb_Lookup_Key` 0x1445a22f0 = port 0x1445a4b60, 86/86 instructions equal.
It reads the database's +0xb8, +0xc0, +0xc8, +0xd8, +0xe8 and +0xec, which are the map's multiplier, slots, info,
mask, increment and shift at +0x10..+0x44 from +0xa8, and returns a pointer into a 16-byte slot at +8. `DB_LOADED`
+0xf0: `Gdb_Load_GameDatabase` 0x1445a0350 = port 0x1445a2bc0, 779/779 instructions equal. It clears
`[db + 0xf0]` on entry and sets it to 1 at the end. The string record layout dbread reads was not checked.

### Live car operations: what is still missing

`layoutcmp.py` over every field of the car structs: 665 proven. These fields that `car_live::FIELDS` and its helpers use
are not proven yet:

Locating the protector bodies through their data vtables' slots, instead of a head pattern, proves five of them.
`CarChassis.sqrt_mass` +0x98, `CarFuelEcu.fuel_burn_scale_lean` +0x28, `temperature_scale_lean` +0xe8 and
`CarDifferential.diff_on_throttle` +0xf0 / `diff_off_throttle` +0xec each have an instruction with the same base and
displacement in the aligned port body (alignment ratio 0.96 to 0.97). `portcheck.py` does not do this vtable-slot
locating yet. Four fields stay unproven: `CarSteering.max_steering_lock`, `CarTurbo.max_boost`,
`CarChassis.tyre_physics_group` +0x118 and `CarFuelEcu.temperature_scale_max` +0x118. The bodies above do not read
the last two.

| field | why |
|---|---|
| `CarSteering.max_steering_lock` +0x28 | the reader `Car_Read_Steering` 0x1446a7070 is a protector thunk to 0x15541f940; no unique pattern found its port twin |
| `CarDifferential.diff_on_throttle` +0xf0, `diff_off_throttle` +0xec | Reset body 0x1553d6f70. Its data vtable 0x145bfa120 slot +0x18 ports to 0x145bfc2e0 → body 0x154af6cb0, which is protector-mangled (9/165 equal before folding) |
| `CarFuelEcu.fuel_burn_scale_*`, `temperature_scale_*` +0x28..+0x118 | reader body 0x1553fb310; vtable 0x145bfa6f0 slot +0x10 → port body 0x154b15310. It differs by hidden stack offsets (`mov ecx, [X]; xor ecx, K; lea r8, [rsp + rcx]`) and junk writes to protector globals |
| `CarChassis.sqrt_mass` +0x98, `tyre_physics_group` +0x118 | reader 0x144676730 (thunk); vtable 0x145bf9c10 slot +0x10 → port body 0x154ae0140, the same kind of mangling (102/126) |
| `CarTurbo.max_boost` +0x2d8 | read in the protector body 0x15543fad0 only |

## Next update

1. Add the overlay and run `bindverify.py --build <new>`.
2. Run `portcheck.py --port <exe> --build <new>`.
3. Add the build to `runtime/build_proof.hpp` `KNOWN[]`. For every feature it reports proven, set the build's bit in
   that feature's proof: `hooks/car_proofs.hpp`, `live/live_car_logic.hpp`, `assets/gamedb.cpp` `RECORD_CLASSES`,
   `frontend/player_name/copies.hpp` `TT_NAME_LAYOUT`, with the build's row in `REGISTRY_VTABLES` (the vtable RVAs
   portcheck prints). The signature-found parts (game database classes, crash names) follow on their own.
4. A failing dependency names the function to port by hand.

## Appendix: portcheck.py output for this build

`re/tools/portcheck.py --port <port exe> --build steam-26566308`, 2026-11-17:

### car_levers: proven

Gate: `native/src/core/hooks/car_live.cpp car_levers_mixins`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | site_Car_EngineDriveTorque | yes | site +0x511 in 0x1446e2290 vs port 0x1446e4b00: 1499/1509 instructions equal over 0x202c; the rest differ only by displacements that moved on this build and no gated feature reads (+0x1a62 -> +0x1a66, +0x1a64 -> +0x1a68, +0x1a65 -> +0x1a69, +0x1a68 -> +0x1a6c) |
| binding | site_Ai_TableGripInput | yes | site +0x883 in 0x14063e370 vs port 0x14063e3b0: 3413/3413 instructions equal over 0x437f |
| binding | site_Ai_BuildSystem_RecordPushed | yes | site +0xd8d in 0x1405ff750 vs port 0x1405ff790: 3761/3761 instructions equal over 0x4b25 |
| field | AiSystem.records_begin | yes | 0xa8: 0x1405ffbe0 `mov qword ptr [rbx + 0xa8], rax` = port 0x1405ffc20 (fn ratio 1.00, 1 insn) |
| field | AiCarRecord.vehicle_index | yes | 0x0: 0x1406001a0 `mov byte ptr [rdi], bpl` = port 0x1406001e0 (fn ratio 1.00, 2 insn) |
| field | AiCarRecord.driver_config | yes | 0x238: 0x140600451 `lea rcx, [rdi + 0x238]` = port 0x140600491 (fn ratio 1.00, 2 insn) |

### car_ops: NOT proven (CarSteering.max_steering_lock, CarDifferential.diff_on_throttle, CarDifferential.diff_off_throttle, CarTurbo.max_boost, CarChassis.sqrt_mass, CarChassis.tyre_physics_group, CarFuelEcu.temperature_scale_lean, CarFuelEcu.temperature_scale_max)

Gate: `native/src/core/hooks/car_live.cpp setup`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | g_Sess_VehicleTable | yes | data 0x146ce86e0 -> port 0x146cea6e0 |
| binding | g_Sess_VehicleCount | yes | data 0x146ce86dc -> port 0x146cea6dc |
| binding | Car_Engine_ComputeDerived_Thunk | yes | 0x1553f9a30 vs port 0x154b13860: 63/63 instructions equal over 0x125 |
| binding | Car_Tuning_SetFinalDrive | yes | 0x1446a4c20 vs port 0x1446a7490: 250/250 instructions equal over 0x50f |
| binding | Car_Tyre_SetWearFactor | yes | 0x142ea2300 vs port 0x142ea4890: 427/427 instructions equal over 0x782 |
| binding | Car_FuelEcu_SetMode_Thunk | yes | 0x1553ff3d0 vs port 0x154b19d40: 39/39 instructions equal over 0xab |
| rva | g_EngineModeAvail | yes | 0x14606ec28 -> port 0x146070c28 data (sections .rsrc / .xpdata) |
| field | CarLive.vehicle_id | yes | 0xe8: 0x14462963e `mov byte ptr [rdi + 0xe8], al` = port 0x14462beae (fn ratio 1.00, 1 insn) |
| field | CarLive.is_ai | yes | 0x124: 0x1446dc4f3 `cmp dword ptr [rax + 0x124], 1` = port 0x1446ded63 (fn ratio 1.00, 2 insn) |
| field | CarLive.engine | yes | 0x530: 0x14473c36e `mov qword ptr [rsi + 0x530], rbx` = port 0x14473ed2e (fn ratio 1.00, 1 insn) |
| field | CarLive.gearbox | yes | 0x538: 0x14473c3ee `mov qword ptr [rsi + 0x538], rbx` = port 0x14473edae (fn ratio 1.00, 1 insn) |
| field | CarLive.fuel_tank | yes | 0x540: 0x14473c46e `mov qword ptr [rsi + 0x540], rbx` = port 0x14473ee2e (fn ratio 1.00, 1 insn) |
| field | CarLive.fuel_ecu | yes | 0x548: 0x14473c4ee `mov qword ptr [rsi + 0x548], rbx` = port 0x14473eeae (fn ratio 1.00, 1 insn) |
| field | CarLive.brake_ecu | yes | 0x550: 0x14473c56e `mov qword ptr [rsi + 0x550], rbx` = port 0x14473ef2e (fn ratio 1.00, 1 insn) |
| field | CarLive.chassis | yes | 0x558: 0x14473c5ee `mov qword ptr [rsi + 0x558], rbx` = port 0x14473efae (fn ratio 1.00, 1 insn) |
| field | CarLive.steering | yes | 0x560: 0x14473c66e `mov qword ptr [rsi + 0x560], rbx` = port 0x14473f02e (fn ratio 1.00, 1 insn) |
| field | CarLive.f1_aero | yes | 0x570: 0x14473c77e `mov qword ptr [rsi + 0x570], rbx` = port 0x14473f13e (fn ratio 1.00, 1 insn) |
| field | CarLive.differential | yes | 0x578: 0x14473c8f2 `mov qword ptr [rsi + 0x578], rbx` = port 0x14473f2b2 (fn ratio 1.00, 1 insn) |
| field | CarLive.suspension | yes | 0x580: 0x14473c96d `mov qword ptr [rsi + 0x580], rbx` = port 0x14473f32d (fn ratio 1.00, 1 insn) |
| field | CarLive.turbo | yes | 0x5a0: 0x14473cb59 `mov qword ptr [rsi + 0x5a0], rbx` = port 0x14473f519 (fn ratio 1.00, 1 insn) |
| field | CarLive.f1_wing_damage | yes | 0x5c8: 0x14473cdc0 `mov qword ptr [rsi + 0x5c8], rbx` = port 0x14473f780 (fn ratio 1.00, 1 insn) |
| field | CarLive.f1_suspension_damage | yes | 0x5d0: 0x14473ce3b `mov qword ptr [rsi + 0x5d0], rbx` = port 0x14473f7fb (fn ratio 1.00, 1 insn) |
| field | CarComponentBase.car | yes | 0x8: 0x144748528 `mov qword ptr [rsi + 8], rdi` = port 0x14474aee8 (fn ratio 1.00, 1 insn) |
| field | CarComponentBase.data_vtable | yes | 0x18: 0x144748512 `call qword ptr [rax + 0x18]` = port 0x14474aed2 (fn ratio 1.00, 2 insn) |
| field | CarSteering.max_steering_lock | NO | 0x28 not proven: 0x1446a7070: no unique head pattern on the other build |
| field | CarGearBox.final_drive_ratio | yes | 0x28: 0x1446a3b4b `xor eax, dword ptr [rsi + 0x28]` = port 0x1446a63bb (fn ratio 1.00, 2 insn) |
| field | CarGearBox.gear_ratio1st | yes | 0x58: 0x1446a3b9b `xor eax, dword ptr [rsi + 0x58]` = port 0x1446a640b (fn ratio 1.00, 2 insn) |
| field | CarGearBox.gear_ratio9th | yes | 0xd8: 0x1446a3f66 `mov eax, dword ptr [rsi + 0xd8]` = port 0x1446a67d6 (fn ratio 1.00, 2 insn) |
| field | CarGearBox.max_clutch_torque | yes | 0xe8: 0x1446a3f9a `mov eax, dword ptr [rsi + 0xe8]` = port 0x1446a680a (fn ratio 1.00, 2 insn) |
| field | CarGearBox.use_setup_ratios | yes | 0x540: 0x1446a4caf `movzx eax, byte ptr [rsi + 0x540]` = port 0x1446a751f (fn ratio 1.00, 2 insn) |
| field | CarDifferential.diff_on_throttle | NO | 0xf0 not proven: 0x1553d6f70: 1/1 instruction(s) differ (ratio 0.52) |
| field | CarDifferential.diff_off_throttle | NO | 0xec not proven: 0x1553d6f70: 1/1 instruction(s) differ (ratio 0.52) |
| field | CarTurbo.max_boost | NO | 0x2d8 not proven: 0x15543fad0: no instruction with displacement 0x2d8 |
| field | CarFuelTank.fuel_capacity | yes | 0x98: 0x155403755 `xor eax, dword ptr [rsi + 0x98]` = port 0x154b24bde (fn ratio 0.93, 2 insn) |
| field | CarFuelTank.fuel_mass | yes | 0x100: 0x14468f04d `vmovss xmm1, dword ptr [rsi + 0x100]` = port 0x1446918bd (fn ratio 1.00, 2 insn) |
| field | CarDamageFrontWing.speed_threshold | yes | 0x28: 0x14465ff05 `vmovss xmm1, dword ptr [rdi + 0x28]` = port 0x144662775 (fn ratio 1.00, 1 insn) |
| field | CarDamageFrontWing.full_detach_scale | yes | 0x38: 0x1446600d6 `vmovss xmm5, dword ptr [rdi + 0x38]` = port 0x144662946 (fn ratio 1.00, 1 insn) |
| field | CarDamageSuspension.speed_threshold_obf | yes | 0x28: 0x14465342d `xor eax, dword ptr [rsi + 0x28]` = port 0x144655c9d (fn ratio 1.00, 7 insn) |
| field | CarDamageSuspension.angular_fracture_limit | yes | 0x7c: 0x14465350a `mov rbx, qword ptr [rsi + 0x78]` = port 0x144655d7a (fn ratio 1.00, 7 insn) |
| field | CarEngine.vtf_limiter_rotation_rate | yes | 0x8b0: 0x144685b7e `xor eax, dword ptr [rsi + 0x8b0]` = port 0x1446883ee (fn ratio 1.00, 2 insn) |
| field | CarF1Aero.lift_scale | yes | 0x19a4: 0x1446dda89 `vmulss xmm0, xmm0, dword ptr [rdi + 0x19a4]` = port 0x1446e02f9 (fn ratio 1.00, 2 insn) |
| field | CarF1Aero.drag_scale | yes | 0x19a0: 0x1446ddff8 `vmulss xmm3, xmm2, dword ptr [rdi + 0x19a0]` = port 0x1446e0868 (fn ratio 1.00, 1 insn) |
| field | CarF1Aero.aero_slip_stream_air_density_scale_front_wing | yes | 0xe8: 0x1446c88ed `xor eax, dword ptr [rsi + 0xe8]` = port 0x1446cb15d (fn ratio 1.00, 2 insn) |
| field | CarF1Aero.aero_slip_stream_car_width | yes | 0x128: 0x1446c89cd `xor eax, dword ptr [rsi + 0x128]` = port 0x1446cb23d (fn ratio 1.00, 2 insn) |
| field | CarBrakeEcu.brake_bias | yes | 0x21d8: 0x144676279 `vmovss xmm3, dword ptr [rcx + 0x21d8]` = port 0x144678ae9 (fn ratio 1.00, 1 insn) |
| field | CarBrakeEcu.brake_pressure | yes | 0x21dc: 0x144676351 `vmovss xmm0, dword ptr [rcx + 0x21dc]` = port 0x144678bc1 (fn ratio 1.00, 1 insn) |
| field | CarChassis.sqrt_mass | NO | 0x98 not proven: 0x144676730: no unique head pattern on the other build |
| field | CarChassis.tyre_physics_group | NO | 0x118 not proven: 0x144676730: no unique head pattern on the other build |
| field | CarSuspension.tyre_types | yes | 0x38: 0x1446aa9a7 `mov dword ptr [r12 + 0x38], 0x3ad1b717` = port 0x1446ad217 (fn ratio 1.00, 10 insn) |
| field | CarSuspension.axle0 | yes | 0x90: 0x14475324d `mov qword ptr [rsi + 0x90], rbx` = port 0x144755c0d (fn ratio 1.00, 3 insn) |
| field | CarSuspension.axle1 | yes | 0x98: 0x1447532d2 `mov qword ptr [rsi + 0x98], rbx` = port 0x144755c92 (fn ratio 1.00, 3 insn) |
| field | CarSuspAxle.local_rest_position_z | yes | 0x1598: 0x1446a7c24 `vmovss xmm0, dword ptr [rsi + 0x1598]` = port 0x1446aa494 (fn ratio 1.00, 7 insn) |
| field | CarSuspAxle.setup_camber | yes | 0x1650: 0x1446a7cbd `vmovss dword ptr [rax + 0x1650], xmm0` = port 0x1446aa52d (fn ratio 1.00, 2 insn) |
| field | CarSuspAxle.setup_toe | yes | 0x1654: 0x1446a7eb5 `vmovss dword ptr [rax + 0x1654], xmm0` = port 0x1446aa725 (fn ratio 1.00, 2 insn) |
| field | CarSuspAxle.setup_anti_roll | yes | 0x1664: 0x1446a80e0 `vmovss dword ptr [rax + 0x1664], xmm0` = port 0x1446aa950 (fn ratio 1.00, 2 insn) |
| field | CarSuspAxle.ride_height_offset | yes | 0x15e4: 0x1446dd457 `vmovss xmm0, dword ptr [rax + 0x15e4]` = port 0x1446dfcc7 (fn ratio 1.00, 2 insn) |
| field | CarFuelEcu.fuel_burn_scale_lean | yes | 0x28: 0x1553fb347 `xor eax, dword ptr [rsi + 0x28]` = port 0x154b24ab7 (fn ratio 0.82, 2 insn) |
| field | CarFuelEcu.fuel_revs_scale_max | yes | 0xd8: 0x1553fb545 `xor eax, dword ptr [rsi + 0xd8]` = port 0x154b24cb9 (fn ratio 0.82, 2 insn) |
| field | CarFuelEcu.temperature_scale_lean | NO | 0xe8 not proven: 0x1553fb310: 2/2 instruction(s) differ (ratio 0.82) |
| field | CarFuelEcu.temperature_scale_max | NO | 0x118 not proven: 0x1553fb310: no instruction with displacement 0x118 |
| field | CarFuelEcu.fuel_mode | yes | 0x128: 0x1553fdf6c `movsxd r14, dword ptr [rcx + 0x128]` = port 0x154b1804c (fn ratio 0.98, 2 insn) |

### car_parts: NOT proven (Car_Register_DetachablePart_Thunk)

Gate: `native/src/core/hooks/car_live.cpp car.parts.register and car.damage points`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | Car_FrontWing_Init | yes | 0x14465f620 vs port 0x144661e90: 296/296 instructions equal over 0x791 |
| binding | Car_FrontWing_OnCollision | yes | 0x14465fdc0 vs port 0x144662630: 310/310 instructions equal over 0x5bf |
| binding | Car_Register_DetachablePart_Thunk | NO | 0x15537d4f0 vs port 0x154abb5b0: 106/147 instructions equal over 0x2e2, first difference at 40; still differs after folding (mov r9d, 0x3c5df055 / lea rdx, [rsp + 0x40]) |
| binding | Game_Get_ModelNodeId | yes | 0x143a3d780 vs port 0x143a3fd50: 506/506 instructions equal over 0x7e4 |
| field | CarComponentBase.car | yes | 0x8: 0x144748528 `mov qword ptr [rsi + 8], rdi` = port 0x14474aee8 (fn ratio 1.00, 1 insn) |
| field | CarDamageFrontWing.full_detach_scale | yes | 0x38: 0x1446600d6 `vmovss xmm5, dword ptr [rdi + 0x38]` = port 0x144662946 (fn ratio 1.00, 1 insn) |
| field | CarDamageFrontWing.left_damage | yes | 0x48: 0x1446600db `vmovss xmm3, dword ptr [rdi + 0x48]` = port 0x14466294b (fn ratio 1.00, 1 insn) |
| field | CarDamageFrontWing.right_damage | yes | 0x4c: 0x1446600e0 `vmovss xmm4, dword ptr [rdi + 0x4c]` = port 0x144662950 (fn ratio 1.00, 1 insn) |

### car_points: proven

Gate: `native/src/core/hooks/car_live.cpp arm_car_points`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | site_Car_LiveCarBuilt | yes | site +0x17d9 in 0x144628190 vs port 0x14462aa00: 2641/2641 instructions equal over 0x3afe |
| binding | Car_Systems_Reset_Thunk | yes | 0x1555bdf80 vs port 0x154ce1490: 34/64 instructions equal over 0xc2; equal after folding 1+1 protector constant(s) and dropping alignment |
| binding | Car_Upgrades_ApplyToVehicle | yes | 0x144742c70 vs port 0x144745630: 510/510 instructions equal over 0x8e0 |
| binding | site_Car_SetupTuningApplied | yes | site +0x21d1 in 0x140c11550 vs port 0x140c11e90: 1902/1902 instructions equal over 0x2230 |
| binding | Game_Pitlane_SetVehicleStatus | yes | 0x1434bc540 vs port 0x1434beac0: 550/550 instructions equal over 0x963 |
| field | CarLive.vehicle_id | yes | 0xe8: 0x14462963e `mov byte ptr [rdi + 0xe8], al` = port 0x14462beae (fn ratio 1.00, 1 insn) |
| field | CarLive.upgrades_allocator | yes | 0x4a0: 0x14462878f `mov qword ptr [rdi + 0x4a0], r13` = port 0x14462afff (fn ratio 1.00, 1 insn) |
| field | CarComponentBase.car | yes | 0x8: 0x144748528 `mov qword ptr [rsi + 8], rdi` = port 0x14474aee8 (fn ratio 1.00, 1 insn) |
| field | PitLaneSystem.car_slots | yes | 0x68: 0x142cfd85d `mov rax, qword ptr [rcx + 0x68]` = port 0x142cffded (fn ratio 1.00, 1 insn) |
| field | PitLaneCarState.state | yes | 0x4: 0x1408140ac `mov edx, dword ptr [rdx + 4]` = port 0x1408140fc (fn ratio 1.00, 3 insn) |
| field | PitLaneCarState.pit_stops | yes | 0x14: 0x142cfe2fe `mov eax, dword ptr [rax + 0x14]` = port 0x142d0088e (fn ratio 1.00, 1 insn) |

### gamedb_late: proven

Gate: `native/src/core/assets/gamedb.cpp late_inplace`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | g_Gdb_Database | yes | data 0x146cc4278 -> port 0x146cc6278 |
| binding | Gdb_Lookup_Key | yes | 0x1445a22f0 vs port 0x1445a4b60: 86/86 instructions equal over 0x12e |
| rva | vt_Gdb_IntRecord | yes | 0x145bee788 -> port 0x145bf0948 same (vtable, 8 slot(s) alike 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00) |
| rva | vt_Gdb_FloatRecord | yes | 0x145bee798 -> port 0x145bf0958 same (vtable, 8 slot(s) alike 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00) |
| rva | vt_Gdb_BoolRecord | yes | 0x1456e6518 -> port 0x1456e8638 same (vtable, 2 slot(s) alike 1.00 1.00) |

### tt_player_name: proven

Gate: `native/src/core/frontend/player_name.cpp setup`

| kind | name | ok | evidence |
|---|---|---|---|
| binding | g_Vm_Registry | yes | data 0x146ca54f8 -> port 0x146ca74f8 |
| binding | Vm_Set_String | yes | 0x140846270 vs port 0x1408462c0: 234/234 instructions equal over 0x2fb |
| binding | Ne_Make_StrFromCStr | yes | 0x1441974e0 vs port 0x144199ab0: 107/107 instructions equal over 0x16e |
| rva | Game_PresenceNameCache_Lookup | yes | 0x140816b10 -> port 0x140816b60 same (entry 0x60 alike 1.00) |
| rva | vt_Vm_ObjectNode | yes | 0x145a70080 -> port 0x145a72200 same (vtable, 8 slot(s) alike 1.00 1.00 1.00 1.00 0.93 0.79 0.87 1.00) |
| rva | vt_Vm_ListNode | yes | 0x145a6ff90 -> port 0x145a72110 same (vtable, 8 slot(s) alike 0.73 0.95 1.00 1.00 0.93 1.00 0.87 1.00) |
| rva | vt_Vm_StringLeafNode | yes | 0x14565f200 -> port 0x145661200 same (vtable, 8 slot(s) alike 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00) |
| rva | vt_Vm_StringBinding | yes | 0x145660370 -> port 0x145662370 same (vtable, 8 slot(s) alike 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00) |
| function | Vm_GetOrCreate_ChildNode | yes | 0x1442b2a50 vs port 0x1442b5020: 115/115 instructions equal over 0x1a0 |
| function | Vm_Unk_140846570 | yes | 0x140846570 vs port 0x1408465c0: 109/109 instructions equal over 0x193 |
| function | Game_Vm_HolderGetCStrOrEmpty | yes | 0x14429db40 vs port 0x1442a0110: 141/141 instructions equal over 0x1d7 |
| function | Vm_ListLookup | yes | 0x1442b1130 vs port 0x1442b3700: 93/93 instructions equal over 0x11f |

