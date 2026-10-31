# fe_home: front-end home screen

Owner: see `subsystems.tsv`. Rows: `kb.py find '\tfe_home\t'`. Long write-up: research/exe.md §3.

## Map
- **Entry:** `FeHome_Handle_Command` (cxx `game::FEHomeViewModel::onCommandRaised`, 0x141bb3c20) matches commands by djb2 (`h_*` rows in strings_anchors.tsv). The VM object is `FEHomeViewModel` (`structs/FEHomeViewModel.h`).
- **Build:** the home list is built by the generic `FeTile_Build_List` (fe_tiles) in its (0,0) case, block 0x141742f8f–0x141743055, which calls the twelve `FeHome_Add_*` adders in order. Every adder is a clone: `button_%d` node, Tile (0xd8) via `FeTile_Construct_Tile`, HomeItem (0x148) with a per-tile vtable `vt_FeHome_*Item`, push into `TileList.items`.
- **Index registers in the home block:** r14 = TileList, r15b/r12d = running index; every later index derives from r12 or r15.

## Data flow: select → route {#select}
`select_pressed` → `FeTile_List_GetSelectedItem_Thunk(vm->tile_list)` → `item->vt.is_selectable` (+0x70) → `gate_a`/`gate_b` (+0x30/+0x38) → `get_action` (+0x28) → `Action.type == 1` → `Vm_Fire_Route(vm->router, vm, &Action.str, 0)`. T000 confirmed the path up to `get_action`.

## Hook: MOD SETTINGS tile (shipped by native/src/core) {#mod-tile}
MidHook at `site_FeHome_AfterCareerCall` (sig `fe_home.after_career_site`), calls `FeHome_Add_CareerTile(list, r12b+1)`, re-types the new item with a copied Career vtable (slots 4/5 replaced), sets `HomeItem.id = "mod_menu"`, then `r12 += 1; r15 += 1`. T000 run1: fires once per home build, items 2 → 3, populate and getAction reached.

## Open
- Route firing (`Vm_Fire_Route`) and label rendering are not yet runtime-confirmed (need a trace test).
- `FeHome_Add_Tile5d40` (flag bit 47) and the dev tile (bit 53): what they are.

## Inbox
- 2026-10-19 14:03 ui-re: tab/screen builders beyond the home tile menu are now in the fe_flow lane (FeFlow_Add_DriversTab/StandardTab, the SessionDirector/Results container tabs). The home tile menu stays here; the generic frontend flow/transition/routing is fe_flow + vm.
- 2026-10-30 00:32 night2-ts: 2026-10-30 night2-ts: Game_FEHomeViewModel_OnScreenActivated (0x141bb17f0, decomp:re/decomp/141bb17f0.c) stores the TileList it builds at vm+0x358 and calls FeTile_Build_List(self = that list, vm+0x270, vm+0x278, ..., 0, vm+0x380, ...); the home view model's +0x2c8 is another object (its +0xd8/+0x130 are read there), so a populate-loop owner scan for tile screens at +0x2c8 never matches home. The test agent (native/src/core/testagent/agent.cpp) now matches FeHome_Handle_Command in vtable slot 10 with +0x358 == r14 and saw the home view model in game on 2026-10-30 00:08:45 (run shell-20261029-130721-1).
