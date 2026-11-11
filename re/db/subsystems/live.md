# live: in-process live game state (std lib osd)

Owner: see `subsystems.tsv`. Rows: `kb.py find '\tlive\t'`. Structs: `structs/*.h` with `// subsystem: live`.

The lane exists because the memory-backed std telemetry (stream L2 of the std-osd work) reads the same objects the game's own UDP telemetry builder reads. Everything below is the Steam build `d873a1c8`. The cross-reference for field meaning is `Game_TelemetryUdp_UpdateRace` (0x142cf9820), `Game_TelemetryUdp_SyncLapData` (0x142cfd6c0) and `Sess_CopyOptionsToState` (0x142cfaec0); the object links come from the UDP state initialiser 0x142cf5c00.

## Registry
- `[*g_Game_Manager + 0x30]` is a robin-hood map (`NeRobinHoodMapU64`). Slots are 16-byte `{key, value}` from `[map+0x18]`; `[map+0x20]` is the info-byte array and also the end of the slot array. A miss is `slots + ((meta - slots)) `, i.e. slot index == end.
- A key is the address of a static type record. `Live_Add_System` (0x144500dc0) files a system under `vtable[0](system)`, which for every system class is `lea rax, [KEY]; ret`. The type record's `+8` holds the class name string, stored by the class's static initialiser (the `Live_*_GetKey` rows name the slot-0 getter, the `k_Live_*Key` rows the key and the static-init site).
- Reading the registry needs no game call: hash-free scan of the slots for the key is enough (Halo does exactly that).

## session-system
`SessionSystem` (key 0x14607f1a0) holds one `SessionVehicleEntry` pointer per vehicle id at `[+0x148,+0x150)`: status, time penalty (XOR double), position - 1 (XOR u32), grid position - 1 (XOR u32). `Sess_RuleManager_Initialise` stores the system at rules+0x1c68; `Sess_Issue_TimePenalty` indexes it by car id and writes the penalty.

## Open
- Overtake: the game has no overtake emitter; `OVTK` has a code string and no emitter xref (checked 2026-11-11). Collision: see the inbox.
- Not decompiled: the tower pill badge path (EntrantsSystem +0x2c8 team tables), the track-name chain, the tyre system +0x110 per-vehicle vector, the red-flag byte quirk, the Time Trial label chain.

## Inbox
