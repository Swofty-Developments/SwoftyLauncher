# fe_popup: transient front-end overlays

Owner: see `subsystems.tsv`. Rows: `kb.py find '\tfe_popup\t'`. The "on top of everything" UI: notifications, popups, busy spinner, full-screen video, tutorial masks. (The `render` lane owns mod overlay drawing; this is the game's own transient UI.)

## Map
- `FePopup_Show_Notification` (0x1406d1db0, L2, sig `fe_popup.show_notification`, `GameEadpModule::showNotification`): builds a notification message, maps the type byte 1-7 through the string table at 0x145660538 (8 = skip), resolves+links the type's node in `g_Vm_Registry`, and posts it. `EADP_INVITES` log. Callers 0x1406d1d50/0x1406d2470/0x1406d2700/… (friend requests, invites, etc).
- `FePopup_Broadcast_Notifications` (0x143885700, L1, `UIConnectionManager::broadcastNotifications`): pushes connection/notification state to registered listeners (`mgr+0xd8..+0xe0`, each `listener->vt[1](listener, flag)`); `TOP_LEVEL_UI` log. **KB addr is mid-function** — the true `.pdata` entry is 0x143883040; hook there.
- `FePopup_Dispatch_FeatureCmd` (0x1402669f0, L2, `NeUIGenericFeatureSystem`, vtable slot 0x1455efff8): executes a UI feature command parsed from a token; covers fade-out (`FADE_REQUESTS` log, sets fade flags on the UI root +0x7b8) and `playFullscreenVideo` on a VideoLayer (e.g. `videos/car_reveal/car_reveal_fx.bk2`).
- `FePopup_Start_BusySpinner` (0x1408225a0, L1, `FullScreenBusySpinner::startBusySpinner`): allocates a 0x218 spinner-controller object (vtable 0x14565f790) with linked-list nodes. The start and stop pretty-functions are both inlined here, so the start/stop split is not resolved (L1).
- `FePopup_Tutorial_OnMaskMoveDone` (0x1438488f0, L2, `UITutorialElementBehaviour::handleMaskMoveComplete`): onboarding tutorial mask element; drives the next onboarding step via `g_Vm_UiSystem` vt[0x68]/vt[0x48] and `Vm_Link_RegistryNode`; `ONBOARDING` log. Callers 0x143847410/0x14384a5d0/0x143ca7e30.

## Hooking
- Intercept/suppress a notification: wrap `FePopup_Show_Notification` (filter on sender/type).
- Suppress the busy spinner or a fullscreen video: wrap `FePopup_Start_BusySpinner` / `FePopup_Dispatch_FeatureCmd`.
- Observe connection-state toasts: wrap the broadcast entry 0x143883040.

## Open
- Busy-spinner stop path (and the controller struct) not split out.
- Popup message boxes (`CheckPrivilegeTask::showPopupMessageGeneratedFromTask` 0x143371320, game lane) and award ceremonies (`AwardCeremonyViewModel` 0x1419ee670) are candidates to pull in next.

## Inbox
- 2026-10-21 20:16 intro-1990: Fullscreen video player documented in core.md (Ne_Play_FullscreenVideo 0x14416de50, L2 dev): feature system path, on_end callback, videos/ prefix, Bink 1 .bik support, AllowVideoSkip skip path. Used by addons/f1-1990/native/src/intro_video.cpp.
- engine-re finding for FePopup_Start_BusySpinner 0x1408225a0: game::FullScreenBusySpinner::startBusySpinner; allocates a 0x218 spinner controller (vtable 0x14565f790) with linked-list nodes; start and stop pretty-functions are both inlined here; L1 (start vs stop ambiguous)
- engine-re finding for FePopup_Broadcast_Notifications 0x143885700: cxx=game::UIConnectionManager::broadcastNotifications; KB addr is mid-function (true pdata entry is 0x143883040); broadcasts connection/notification state to listeners (mgr+0xd8..+0xe0); TOP_LEVEL_UI log; L1 inlined region lb19-verify partial/disagree
