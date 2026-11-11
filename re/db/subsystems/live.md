# live: in-process live game state (std lib osd)

Owner: see `subsystems.tsv`. Rows: `kb.py find '\tlive\t'`. Structs: `structs/*.h` with `// subsystem: live`.

The lane exists because the memory-backed std telemetry (stream L2 of the std-osd work) reads the same objects the game's own UDP telemetry builder reads. Everything below is the Steam build `d873a1c8`. The cross-reference for field meaning is `Game_TelemetryUdp_UpdateRace` (0x142cf9820), `Game_TelemetryUdp_SyncLapData` (0x142cfd6c0) and `Sess_CopyOptionsToState` (0x142cfaec0); the object links come from the UDP state initialiser 0x142cf5c00.

## Registry
- `[*g_Game_Manager + 0x30]` is a robin-hood map (`NeRobinHoodMapU64`). Slots are 16-byte `{key, value}` from `[map+0x18]`; `[map+0x20]` is the info-byte array and also the end of the slot array. A miss is `slots + ((meta - slots)) `, i.e. slot index == end.
- A key is the address of a static type record. `Live_Add_System` (0x144500dc0) files a system under `vtable[0](system)`, which for every system class is `lea rax, [KEY]; ret`. The type record's `+8` holds the class name string, stored by the class's static initialiser (the `Live_*_GetKey` rows name the slot-0 getter, the `k_Live_*Key` rows the key and the static-init site).
- Reading the registry needs no game call: hash-free scan of the slots for the key is enough (Halo does exactly that).

## session-system
`SessionSystem` (key 0x14607f1a0) holds one `SessionVehicleEntry` pointer per vehicle id at `[+0x148,+0x150)`: status, time penalty (XOR double), position - 1 (XOR u32), grid position - 1 (XOR u32). `Sess_RuleManager_Initialise` stores the system at rules+0x1c68; `Sess_Issue_TimePenalty` indexes it by car id and writes the penalty.

## Tyres
`TyreSystem` (key 0x146080d28, vtable 0x1457ef110) keeps one `VehicleTyreComponent *` per vehicle id at `[+0x110,+0x118)`; `TyreSystem::getCompounds` (0x142e97670) and `pitstopComplete` (0x142e946d0) bound-check the id against the count and index it. The component's slot table at `+0x9bd0` is filled by the slot build (thunk 0x142e9e540 into .data1 body 0x14f463120): per option 0..4 `{option, &PhysicalCompound, &VisualCompound}`, the tables being `[[0x1460806d8]+0xb0]` (stride 0x28) and `[[0x1460806d8]+0xd0]` (stride 0x58). `VisualCompound` byte 0 is the stat id, `+0x50` the audio label enum (Soft 3, Medium 4, Hard 5, Intermediate 7, Wet 8).

## Game mode and team badges
`GameModeSystem` (key 0x14604ed28) `+0x88` is the `GameMode` object; `+0x278` is its label C string (`time_trial`, `grand_prix*`). The timing pill (`Team_Set_TimingPillBadge` 0x141760020) is the consumer of the badge chain: `GameMode+0x260` is the `EntrantsManager`, `[+0x2e0,+0x2e8)` its 0x88-byte entrants (`+0x82` vehicle id, `+0x58` team word: byte 0 = team table 0..2, high word = index), `EntrantsSystem` (key 0x14606ecd8) `+0x2c8+table*0x10` the table, `[table+8,table+0x10)` its 0x10-byte `{allocator, TeamRecord *}` slots, `TeamRecord+0x4d8` the badge key and `+0x58d` the render-badge flag. The pill view-model init (0x141759a40) takes both systems from the registry by key.

## Pit lane
`PitLaneSystem+0x48` is the lane object; `+0x938+id*0x10` holds a pointer to the per-vehicle record whose byte `+0x103` is the in-lane flag. UDP lap data derives pit status from it (0x142cfe2b4): 0 when clear, else 2 when `PitLaneCarState.state == 4`, else 1. Ids are guarded `< 0x1b`.

## Track name and spectator gap
The racing-vehicles singleton (global 0x146ce72b8, `g_Live_RacingVehicles`, signature `live.racing_vehicles` through the ERS reader's own load) reaches the track key as `[[[g]+0xd0]+0xe8]+0xa0`; Car_Read_EnergyRecovery compares it to the TrackSettings blocks. `SpectatorDirector+0xe4` is the gap mode (see `SpectatorDirectorGap`).

## Open
- Overtake: the game has no overtake emitter; `OVTK` has a code string and no emitter xref (checked 2026-11-11). Collision: see the inbox.
- Not decompiled: the tower pill badge path (EntrantsSystem +0x2c8 team tables), the track-name chain, the tyre system +0x110 per-vehicle vector, the red-flag byte quirk, the Time Trial label chain.

## Inbox
- 2026-11-11 12:26 osd-l1: 2026-11-11 osd-l1: overtake. The UDP event table has an OVTK code string but strings_anchors lists no emitter xref and no code references it; the game has no overtake emitter. Derive overtakes from the SessionVehicleEntry position diff or road order (progress total) in the core. Collision: only the bus message 'r_car_to_car_col' exists (subscribe thunk 0x142dd4b20); the upstream poster was not found (physics contact path, message instance at CarLive+0x910). Hooking Game_TelemetryUdp_OnCollisionMessage is useless without a UDP sender; the core should Ne_Subscribe_BusMessage (0x143987810, existing L2 row, signature core.subscribe_bus_message) to the name with its own delegate. Retirement: message 'vehicle_status_changed' (thunk 0x142dd61a0).
- 2026-11-11 12:26 osd-l1: 2026-11-11 osd-l1: pit status map for the UDP-equivalent value. SyncLapData 0x142cfd6c0 (0x142cfe2dc..0x142cfe2fe): pit_status = 0 when [[PitLane+0x48]+0x938+id*0x10]+0x103 == 0, else 2 when PitLaneCarState.state == 4, else 1; numPitStops = low byte of PitLaneCarState+0x14; the HUD tower uses states 2..15 as PIT. Also: the same function derives sector as the first sector end >= clamp(lap_m) (g_Live_SectorEnds), result status/penalty/grid from SessionVehicleEntry, warnings and corner-cut warnings from PenaltyLog, unserved drive-through/stop-go from VehicleHelperSystem+0x568/+0x578.
- 2026-11-11 12:41 osd-vc: osd-vc corrections to xref lists: CarLiveInputs.brake must drop 0x142cf9a60 (it is CarBodyState +0x190); CarSteeringLive.steering_angle must drop 0x142cf9f97 (CarLive.vehicle_id read) and 0x142cfa16b (suspension wheel +0xe8), keep 0x142cf9f02,0x142cf9f0a. CarBodyActor.state, CarBodyState.position/velocity agree rows (V8743-V8745) cover the UDP read path only; no writer was located. SessionVehicleEntry.vehicle_id +0x00 gets a writer: 0x142c79954 mov byte [rcx],dl in 0x142c79940 (index passed by the participant build loop 0x142586a60) - owner may raise it to L2 (see V8818).
- 2026-11-11 13:59 osd-ve: osd-ve V8916/V8917: SpectatorDirectorGap.gap_mode is written 1 for session types 15..17 and 3 (not 2) otherwise (0x142e17a47 lea ecx,[rdx+rdx+1], edx=setae). Fix the field notes and the plate of 0x142e16eb0. Also 0x142588c24 in SessionEntrantView.vehicle_id evidence is 0x142588c25. GameModeSystem == [g_Game_Manager+0xc0] is still unproven by a store.
