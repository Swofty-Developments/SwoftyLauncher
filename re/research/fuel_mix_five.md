# A fifth fuel mix: every place that assumes four (static)

engine-re, 2026-11-20. Build: Steam d873a1c8 (the KB build). Everything here is STATIC (capstone disassembly of
`work/exe/F1_25.exe` and the existing decomps in `re/decomp`); nothing is runtime-confirmed. Question from the
vmknolle season add-ons: can F1 25 offer five fuel mixes (today lean / standard / rich / max)?

## Verdict

- **By data: no.** The handling file has exactly 16 named FuelECU values (4 scales x 4 mixes) and the FuelECU
  component has no room for a fifth set (see [layout](#layout)).
- **A real fifth game mode (index 4): not reasonably feasible.** Nine code paths index or pack the mode as 0..3
  without a bound, among them the `.data1` bodies of SetMode and Reset, the replay/flashback snapshot (2 bits) and
  the online controls message (ranged 0..3). See [sites](#sites).
- **A "virtual" fifth mix through a Swofty hook: feasible, small.** The game keeps its four modes. The fifth row
  exists only on the MFD; while it is selected the hook gives one carrier mix (the highest offered one) the fifth
  mix's four scales through the existing `fuel_ecu.*` layer path of `car_live.hpp`, and removes them when the player
  leaves the row. The InputRecord request (+0x00) and `CarFuelEcu.fuel_mode` (+0x128) never leave 0..3, so the
  snapshot, network, UDP, AI and virtual-session readers below see a normal mix. See [route](#route).

## Layout

| item | finding | evidence |
|---|---|---|
| `CarFuelEcu` (0x170 bytes) | four obfuscated 4-entry tables, stride 0x10: burn +0x28, power +0x68, revs +0xa8, temperature +0xe8; `fuel_mode` int32 +0x128; active pairs +0x130/+0x140/+0x150/+0x160. The allocation is exactly 0x170 (`mov edx,0x170` 0x1555cc583), so the struct has no tail space. +0x12c..+0x12f: no access found by a scan of every `[reg+0x548]` -> `[reg+0x12c]` load (HYPOTHESIS: padding; only 4 bytes) | structs/CarFuelEcu.h |
| handling data | `Car_Read_FuelECU` (0x1553fb310) reads 16 named VTF values (FuelBurnScaleLean .. TemperatureScaleMax, strings 0x145bfa727..). The count is fixed by the names; there is no list or count field | structs/CarFuelEcu.h |
| `g_Game_EngineModeAvail` object (0x368 bytes, class string `EngineFuelMixAvailability` 0x14578adb0 HYPOTHESIS) | header 0x28 + 26 cars x 0x20: per car fuel lock dwords +0x28..+0x34 [lean, standard, rich, max] and ERS lock dwords +0x38..+0x44. 0x28 + 26*0x20 = 0x368, the allocation size: no spare dword for a fifth mix. A fifth availability has to live in the hook | globals.tsv 0x14606ec28; native/src/core/hooks/car_live.hpp MODE_AVAIL_* |

## Sites

Every place found that assumes four mixes. "virtual" = what the virtual route needs there.

| # | site | what is hard-coded | a real mode 4 would | virtual |
|---|---|---|---|---|
| 1 | `Car_FuelEcu_SetMode_Thunk` 0x14468e6a0 -> `.data1` body 0x1553ff3d0 | no bound: `movsxd rbx,edx; shl rax,4` (0x1553ff3f8) then reads +0x28/+0x68/+0xa8/+0xe8 + mode*0x10 | read the NEXT table's lean entry for each scale (burn <- power lean, power <- revs lean, revs <- temperature lean, temperature <- fuel_mode/active_burn bytes) = garbage values | unchanged (called with the carrier only) |
| 2 | `Car_FuelEcu_Reset_Thunk` 0x14468e550 -> body 0x1553fdf60 | same unbounded index from +0x128 (0x1553fdf6c); flag 0x2000 forces Standard | same as 1 | a Reset copies the carrier's own values back: the hook re-applies after `car.reset` |
| 3 | `Sess_ControllerSystem_ApplyRecord` 0x14389a2b0 | `cmp edx,3; jg` (0x14389bacf): a request > 3 skips the availability check and the SetMode call; availability dword [mode*4+0x28] (0x14389badb) | code patch + availability table outside the 0x368 object | nothing (request stays 0..3) |
| 4 | `Game_Mfd_FuelMix_BuildOptions` 0x141788bc0 | four fixed bit tests and four `AddRow` calls (ids 0..3, lng_hud_fuel_lean/standard/rich/max); Max skipped when (session-15)<3 | a fifth `AddRow` | hook: add a fifth row (own lng key) when the add-on enables it |
| 5 | mask builders: `Game_Mfd_FuelMix_Refresh` 0x1417883a0 (0x1417884xx), `Game_Mfd_FuelMix_Update` 0x1417888e0 (0x14178893f..0x141788979, compare with +0x324 at 0x14178897d), 0x141788d40 (0x141788ec0..0x141788ef5, compare 0x141788ef9) | mask = 4 bits from the four availability dwords | bit 4 from the hook's own table | the two compares must see the same mask the hook stores, else the options are rebuilt every frame |
| 6 | `Game_Mfd_FuelMix_Update` 0x1417888e0 | shown mix = FuelAuto ([[sub+0xb8]+veh*0x14+0xd7]) ? `fuel_mode` : controller dword (+0x00); if not offered: nearest lower offered, else next upper up to max(3, mix) (`cmp edx,4; mov r9d,3; cmovge` 0x1417889ef) | works for 4 if bit 4 is set | hook: select row 5 (`Game_Mfd_FuelMix_SetSelected(sub, 4)`) while the virtual mix is on |
| 7 | `Game_Mfd_FuelMix_SetSelected` 0x1417891f0 | none: row = number of offered bits below the mix (vectorised popcount), so it is generic for bits 0..31 | - | usable as is |
| 8 | `Game_Mfd_FuelMix_AddRow` 0x141789520 | none: (sub, list, id string, lng key, mix id); the id string is hashed with djb2 (0x1505) | - | usable as is |
| 9 | `Game_Mfd_FuelMixUp` 0x141533370 / `Down` 0x141533570 | Up: `cmp ebx,3; je` (0x1415334cb) and bit (mix+1) of [vm+0x604] (= fuel sub +0x324; sub at vm+0x2e0, 0x14153617e); Down stops at 0; one step only, a locked neighbour blocks the step | imm 3 -> 4 | hook Up at the carrier (top offered mix) -> virtual on; Down while virtual -> virtual off |
| 10 | replay / flashback snapshot: writer 0x14470bd80 (0x14470c3f1..0x14470c3ff: `shl al,4; and al,0x30`), reader `Car_State_Read_Snapshot` 0x14470d760 (0x14470e07c: `shr edx,4; and edx,3`) | fuel mode = 2 bits (4..5) of a packed byte; bits 0..3 and 6..7 belong to other fields | mode 4 is stored as 0 = Lean | nothing (fuel_mode stays the carrier); the hook re-applies the virtual values after the restore |
| 11 | online controls message: `Game_VehicleMechanicalManager_BuildVehicleStateMessage` 0x143242a70 (ranged writes 0..3 at 0x1432491ce = controls byte +0x65, and 0x143244b41 = another field; log text 'fuelmix(%u)' 0x14582cc76, value = byte +0x65 of the controls) and `..ParseVehicleStateMessage` 0x14324c110 (`Ne_BitStream_ReadRangedUInt` 0..3 at 0x14324db53, 0x14324f13c) | the requested mix is a ranged 0..3 field | wire format change on every peer | nothing: the request stays 0..3 (virtual mix not sent; remote peers see the carrier) |
| 12 | online mechanical state: `Game_NetworkMechanical_VehicleRacingVehicleCar` 0x1432495e0 | byte from `fuel_mode` (0x14324b2df) sent; the receiver calls SetMode with it unchecked (0x14324b7b8) | a 4 would reach site 1 on the remote peer | nothing (carrier) |
| 13 | UDP car status: `Game_TelemetryUdp_CollectDrsZones` 0x142d00060 (also the car-status writer) | raw `fuel_mode` byte into the 0x3b-byte car entry +0x1954 (load 0x142d0066b, store 0x142d00673); u8, public spec 0..3 | tools would see 4 | UDP shows the carrier |
| 14 | `Ai_Car_CaptureVehicleState` 0x1405ee660 | copies only the lean/standard/rich BURN scales (out[6..8], decomp lines 153-158) and `fuel_mode` (+0x294) | AI cost model has no slot for a fifth | AI unchanged (AI never selects the virtual mix) |
| 15 | virtual session (cars run in the background): 0x142f71790 `switch(fuel_mode)` 0 -> 0, 1 -> table 0x145803e30 by ERS mode, 2/3 -> 1, default -> 3; 0x142f72b1e and 0x142f6c8e0 (param_7) index the burn table by mode*0x10 | mode 4 reads the wrong table | nothing (carrier) |
| 16 | `Ai_QualiPace_CalcSegmentTime` 0x1429805c0 | passes `fuel_mode` to 0x142f6c8e0 (site 15) | as 15 | nothing |
| 17 | `Car_Visual_UpdateRenderState` 0x14039aa80 | +0x108c = `fuel_mode`, blend = mode * (1/3) (decomp line 2055) | blend 1.33 | nothing |
| 18 | practice programmes 0x142947400, 0x142971900, 0x14297f3c0, 0x14298d9d0, 0x142990230, 0x1429a06b0, 0x1429a22d0, 0x1429b1390 | `cmp [ecu+0x128],1` then an inline copy of the Standard scales (+0x38/+0x78/+0xb8/+0xf8) = SetMode(1) inlined; lock bits on lean/rich/max | - | they force Standard: the hook leaves virtual mode (or re-applies after the programme) |
| 19 | career/objective checks 0x140a83940 (`cmp mode,2` Rich) and `.data1` 0x1490766f0 (`cmp mode,0` Lean) | exact values | - | carrier semantics apply |
| 20 | wheel display `Game_ThrustmasterLcd_UpdateSectors` 0x142fd95e0 | `fuel_mode` copied to LCD state +0x200 (0x142fd9ecb); its reader not traced | - | shows the carrier |
| 21 | text keys | lng_hud_fuel_lean/_standard/_rich/_max (0x145707614..0x14570764e), ids lean/standard/rich/max | own key | own lng key via the Swofty language data |
| 22 | engineer speech | no fuel-mix speech string found in `.xtext` (only 'low_fuel_mode', 'Engine Lean Fuel' / 'EngineLeanFuel' ids, roles not read) | - | OPEN (T332) |

Places with spare room for a fifth: none found in the exe. No array of 5 or 8 mix entries. The 26 x 0x20
availability block, the 0x170 FuelECU block and the 2-bit snapshot field are all full. The UDP byte and the
mechanical-state byte are u8 (room for values, not for meaning).

## Route

Virtual fifth mix (recommended; nothing above has to change):

1. **Offer the row.** Replace `Game_Mfd_FuelMix_BuildOptions` 0x141788bc0 (0x17c bytes, same calls) with a
   version that adds a fifth `Game_Mfd_FuelMix_AddRow(sub, sub+0x350, id, lng_key, 4)` when the add-on enables it,
   and stores mask | 0x10 at +0x324. The two per-frame compares (0x14178897d, 0x141788ef9) then need the same
   bit: a small code-site hook on each that ORs 0x10 into the computed mask (or replace the mask builders).
2. **Step into it.** Wrap `Game_Mfd_FuelMixUp` / `Down`: Up at the top offered mix -> virtual on (no write to the
   controller dword); Down while virtual -> virtual off. Keep the VM notification the originals do (+0xb68 bool,
   +0xb80 = 1).
3. **Show it.** After `Game_Mfd_FuelMix_Update` 0x1417888e0 returns, call `Game_Mfd_FuelMix_SetSelected(sub, 4)`
   while virtual is on (the update itself selects the carrier row).
4. **Drive it.** Virtual on: a `fuel_ecu.*.<carrier>` layer with the fifth mix's four scales (the core already
   re-runs the active mix after a `fuel_ecu.*` write). Virtual off: remove the layer. Re-apply after `car.reset`
   and after the snapshot restore of a flashback.
5. **Rules.** Off-line only to start with (the online peers see the carrier). FuelAuto on: no virtual row (the
   game drives the mix). Practice programmes that force Standard end the virtual mix.

Size estimate: one replaced function (BuildOptions), two mask-site hooks, two wrappers (Up/Down), one post-hook
(Update), plus the existing layer path: about 200 lines in `native/src/core/hooks/` and a `cars::live` op. Needs
L2 + a verify `agree` on BuildOptions, the three mask sites, Update, SetSelected, AddRow, Up/Down before a
behaviour hook (PROTOCOL 2), and the in-game test T330 before a PR.

## Tests

- T330: virtual fifth mix end to end (row, step, values, flashback, qualifying lap reset).
- T331: what the game does with a request of 4 today (ApplyRecord drops it; MFD shows the nearest offered row).
- T332: engineer radio and wheel display on mix changes (does any of them follow the mix).
