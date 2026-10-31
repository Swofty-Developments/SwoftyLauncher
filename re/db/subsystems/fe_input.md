# fe_input: input-to-UI

Owner: see `subsystems.tsv`. Rows: `kb.py find '\tfe_input\t'`. How input devices reach the UI: provider attach/detach, the active-device/tooltip glyph, and the device-present bools the UI binds.

## Map
- `FeInput_Add_PlayerProvider` (0x1434d6fd0, L1, sig `fe_input.add_input_provider`, `Player::addPlayerInputProvider`): attaches a device input provider to a `Player`. On success it publishes device-present bools on the player's VM nodes at `player+0x290` / `+0x2b0` / `+0x2d0` via `Vm_NodeSet_Bool` (so the UI shows which controllers are connected) and bumps `player+0x580`. Returns 0x16 when the provider record is incomplete. `INPUT` log. (L1: reached indirectly, no clean rel32 caller.)
- `FeInput_Remove_PlayerProvider` (0x1434d7340, L1) / `FeInput_RemoveAll_PlayerProviders` (0x1434d7850, L1): the detach siblings.
- `FeInput_Setup_TooltipDevice` (0x142a7be00, L2, `GameInputModule::setupTooltipDeviceWithNoInput`): when there is no active input device, picks a default provider so button-prompt glyphs/tooltips still render; reads the input module at `(mgr+0x48)+0x140`, provider list at `+0x38/+0x40`. `INPUT` log.
- UI toggle input modes: `Vm_UiToggle_Init` (vm lane) reads a `toggleType` enum — Normal / Bumpers / Triggers / Button / BumperNoRepeat / TriggerNoRepeat / DpadOnly / NormalNoFocus — and `toggleInput`, which is how a UITG binds to controller navigation.
- `focus_changed` (h_focus_changed) is the djb2 command the VM gets when UI focus moves; handled in each screen's `onCommandRaised`.

## Hooking
- Detect controller connect/disconnect for mod UI: wrap `FeInput_Add_PlayerProvider` / the Remove siblings and read the device string + playerId.
- Force a tooltip device (e.g. for a headless/replay overlay): wrap `FeInput_Setup_TooltipDevice`.

## Open
- The GameInputModule object layout (provider list, active-device slots at module+0xd0/+0xd8) and `SettingsControlsScreenViewModel::deviceConnectionStatusChanged` (0x141d9efb0, game lane) are the next steps for a full input→UI map.

## Inbox
- engine-re finding for FeInput_Add_PlayerProvider 0x1434d6fd0: cxx=game::Player::addPlayerInputProvider; attaches an input device provider to a Player and publishes device-present bools on VM nodes player+0x290/+0x2b0/+0x2d0 via Vm_NodeSet_Bool; returns 0x16 if the provider is incomplete; INPUT log; L1 (no clean caller xref, reached indirectly)
- engine-re finding for FeInput_Remove_PlayerProvider 0x1434d7340: cxx=game::Player::removePlayerInputProvider
- engine-re finding for FeInput_RemoveAll_PlayerProviders 0x1434d7850: cxx=game::Player::removeAllPlayerInputProviders
