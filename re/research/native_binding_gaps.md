# Three native features and the records they stand on (re-bindings, 2026-11-18)

Static only, on the KB exe (`d873a1c8`) and Steam 1.0.145 (`46009bd3`). The export state is what `bindgen.py` writes on this branch: `bindings.json`, `bindings.steam-26566308.json` and `bindings.ea-1-0-145.json`. Each section gives the record, what is proven, what the native code assumes, the risk, and what closes it. No native code was changed.

## 1. The era picker's 26-vehicle clamp

Native: `vehicles_entry` in `native/src/core/boxes/picker/time_trial.cpp`, armed by `tt_guard_commit` in `native/src/core/boxes/era_picker.cpp` ("a session with more vehicles than its table of 26 sets up the first 26 instead of writing past it"). It wraps `Sess_Setup_VehicleComponents`. When the session holds more than 26 vehicles, it sets the vehicle vector's end (module +0x30) to begin + 26 × 8 for the call. It puts the old end back afterwards, unless something changed it during the call.

Record: `Sess_Setup_VehicleComponents` 0x142f45960, L2, exported as experimental on all three sets. Its newest verify row is V20008, an agree. On Steam 1.0.145 it is the same code at 0x142f47ef0 (`sigport.py --witness`: 539 of 539 instructions, with +0x28, +0x30, +0xb0 and +0x180 used at the same instructions).

Proven:
- The function takes the vehicle count as the byte ((end − begin) >> 3) & 0xff. It stores one component per vehicle in a 26-slot table at module +0xb0, with the count at +0x180. Nothing bounds the store at 0x142f45a09, so a 27th vehicle overwrites the count (V1603, V20008).
- Clamping end around the call is memory-safe inside this function. The other readers of the table, vtable slots 0x20 and 0x88, are bounded by +0x180 (V1603).
- Shortening and restoring the vector cannot leak memory or free it twice. It is freed through begin only, by slot 0xc0 0x142f43500 and the destructor 0x142fe77f0, and no destructor runs per element (V2134).
- The cost of the clamp: vehicles past 26 skip this function's per-vehicle setup. That setup covers the component 0x146ce8a00 config copy, the vehicle controller, +0x132 bit 2, +0x2580, +0x9f2c onwards, and 0x144632590 (V1603).
- The AI update 0x1405ee110 re-reads the same vector every frame through `[[ai+0x10]+0xc0]`, +0x28..+0x30 (0x1405ee157 to 0x1405ee1cf). It fills its 16-byte cache at ai+0x128 for i below that byte count, and writes 0 as the component when the lookup fails (0x1405ee1a0).
- Job 0x14063e370 then walks [ai+8] AI vehicles. It loads `[cache + i*16 + 8]` at 0x14063e5dd and reads +0x18 of it at 0x14063e889.
- IG10 run 20261030-061735 faulted at 0x14063e889 with i = 26 and a null component, 23 ms after the guard's log line (V2134, partial).

The native code assumes that a vehicle this call does not set up is harmless for the rest of the session.

Risk: with 27 or more AI vehicles, the guard trades the write past the table for a null read in the AI job.
- The static evidence does not show which path left cache entry 26 null in IG10. Either the AI update read the vector inside the clamped window, or it read it after the restore and the component lookup failed for the vehicle whose setup was skipped.
- Both paths end in the same fault. Shortening this vector cannot make a session with more than 26 vehicles safe. Only keeping the session at 26 vehicles or fewer, before it is set up, can (V2134).

Closes it:
- Move the cap upstream, into the entrant selection, so the session never holds more than 26 vehicles.
- Or trace the AI job at 0x14063e5dd: i, [ai+8] and the vector's count, before and after the call.

## 2. The pit box set's race tyre rule

Native: `change_entry` in `native/src/core/guards/garage/pit_box.cpp` wraps `Car_Tyre_ChangeToNextCompound` for content.package `rules.race_tyre_change` (`native/src/core/apply/content/race_fuel.hpp`). It resolves the name with `swofty::api->resolve`.

Record: `Car_Tyre_ChangeToNextCompound` 0x142e94040.
- Level L1, evidence `pat:log-channel-PITSTOP_TYRES`, proto `-`.
- One agree row, V5528: "body, strings and callees match name/description".
- bindgen exports only L2 and L3 records whose newest verify row is an agree. L1 records go out only in `--dev` sets. So the record is in none of the three sets.
- Every shipped core therefore logs `[pit] race tyre change rule off: this build does not resolve Car_Tyre_ChangeToNextCompound`, and a package with `race_tyre_change` false has no effect.
- The box guard in the same core set is not affected: `site_Game_PitLane_BoxSlotRead` and `site_Game_PitLane_BoxPoseFalse` resolve on all three sets.

Proven (this note, on the KB exe; on Steam 1.0.145 it is the same code at 0x142e965d0, 406 of 406 instructions, with +0x30, +0x98d0, +0x238 and +0xde0 at the same instructions):
- rcx is the TyreComponent. dl is a flag, kept in ebx and tested at 0x142e941eb and 0x142e94219. When it is set, the function calls 0x142e96390.
- The function has one exit, and it returns 0 (`xor eax, eax` at 0x142e946bc).
- There are two callers, and neither reads the result:
  - `Game_PitLane_ApplyPitstopTyres` 0x1434bdd60 passes 1 (`mov dl, 1` at 0x1434bdd9d, call at 0x1434bdd9f).
  - `Game_PitBoxCarActor_Update` 0x1403c8cf0 passes 0 (`xor edx, edx` at 0x1403c8dd1, call at 0x1403c8dd3).
- The wheel records: the count is the u64 at +0x98d0, and the records start at +0x30 with stride 0xde0. Each record's dword +0x208 (+0x238 + i × 0xde0 from the component), masked with 0x1f, is OR-ed into ebp (0x142e94280 to 0x142e9428a).

The native code assumes the following, and all of it matches the code:
- `uint64_t(void *tyre_component, char pit_stop)`;
- it always returns 0;
- flag 1 is the pit stop call;
- `TYRE_WHEELS` 0x30, stride 0xde0, flags +0x208, count +0x98d0 and mask 0x1f.

The reading of those flag bits as damage or a puncture is marked HYPOTHESIS in `race_fuel.hpp` and is not shown here.

Risk: in shipped builds the rule is off without a visible sign beyond the log line. Nothing crashes. If it is turned on, a car whose +0x208 bits are set always gets the game's change, whatever those bits mean.

Closes it: raise the record to L2 with the proto `uint64_t Car_Tyre_ChangeToNextCompound(void *tyre_component, uint8_t pit_stop)`, the caller evidence above and a 1.0.145 proof.
- V5528 is already an agree, so the raise alone would export the record and turn the rule on in every shipped core.
- That is a behaviour change for the pit box lane to decide. V5528 also did not check the proto. So the raise should come with a fresh blind agree, not on its own.

## 3. Career contents on the 2025 model

Native: `native/src/core/career/career_content.cpp` (bind list at `b.populate_2025`). When a registered content uses model 2025, the core resolves `FeTile_CareerDefaultItem_Populate`. It calls that function from its own populate (vtable slot 4) before writing the title and description.

Record: `FeTile_CareerDefaultItem_Populate` 0x1417264c0.
- Level L2, raised by native-1990 for the F1 1990 Career menu.
- Its newest verify row is V0905, a partial (2026-10-21). The notes say the `is_dlc_content` value is item +0x148, but the code passes item +0x190 (`lea r9, [rsi+0x190]` at 0x1417264fb). +0x148 receives the return of vtable slot +0xa0 (`mov [rsi+0x148], al` at 0x1417264e9).
- A partial as the newest row keeps the record out of all three sets.
- When it does not resolve, the bind fails with "needs FeTile_CareerDefaultItem_Populate, which this build does not resolve". `career_content_mixins` then calls `turn_off` on every registered slot, so 2026-model contents registered alongside go off too.
- The other 17 names the file resolves are exported on all three sets.

Proven:
- V0905 confirmed vtable 0x145703ff0 slot 4, the three arguments passed on to `FeTile_ItemBase_Populate`, and the two offsets above. Both offsets were re-read for this note.
- On Steam 1.0.145 it is the same code at 0x141727c40. The address comes from the 0x1780 shift that `FeTile_Route_CareerItem` and `FeTile_CareerDlcItem_Populate` share on either side, and the body is 81 of 81 instructions equal, with +0xa0, +0x148 and +0x190 at the same instructions.
- `sigport.py --witness` finds no unique pattern, because the function is a clone of the 2026 populate. The record's signature starts in the bytes before the function.

The native code assumes the record's proto (`HomeItem *item, void *node, uint32_t a7`) and `ITEM_IS_DLC` = 0x190. The native offset matches the code, and the record's note does not.

Risk: nothing crashes. A 2025-model content never shows, and it takes every other registered content down with it.

Closes it: correct the record's notes to +0x190 and +0x148 as V0905 asks, then get a fresh agree from someone other than the person who corrected them.
