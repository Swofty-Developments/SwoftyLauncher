#include "tt_bridge_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "apply/content/lower.hpp"
#include "apply/ids.hpp"
#include "apply/moments.hpp"
#include "direct_start_tt.hpp"
#include "hooks/hooks.hpp"

#include <atomic>
#include "presence_id_logic.hpp"
#include <cstdint>
#include <cstring>
#include <deque>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <tuple>
#include <vector>

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
}

// Time Trial boards in the game's own leaderboard screen (re/db: game.leaderboard_xml_loaded,
// game.f1os_request_ring_push, the five Game_F1os_OnResult_* of the Time Trial board methods). Nothing here is proven in
// game; every stage logs one [boards] line saying what it found.
//
// 1. Board definitions. The game reads its boards from system\leaderboard.xml (Game_NetworkSession_LoadDefinitions). At
//    site_Game_LeaderboardXml_Loaded the parser's own copies of the buffer pointer and length ([rsp+0x160], [rsp+0x168], made from the stream's [rsp+0x70] and rax) are replaced by a longer
//    buffer with one clone of a stock <leaderboard> element per addon descriptor (ids the core assigned, 6..99), per
//    track the stock boards name, per weather (tt_bridge_logic.hpp extend_xml). Only with "tt_bridge": true in tt.json.
// 2. Requests. A mid hook at the entry of Game_F1os_RequestRing_Push (rcx = the op) reads the op's method name and
//    LeaderboardID. For the five Time Trial methods on an addon board (descriptor >= 6) it returns from the push without
//    sending. The request is written to swofty\tt\outbox\<seq>.json; the flow tick polls swofty\tt\inbox\<seq>.json and
//    completes the op by calling the method's OnResult with a synthetic response (+0x88 0, +0x98 0, +0x9c status, +0xa0 ->
//    {data, ?, length}). Unanswered after 8 s: status 504. Only with "tt_bridge": true.
// 3. Guards, off until "tt_guards": true in tt.json or SWOFTY_TT_GUARDS=1 (their facts are not confirmed in game): a lap in
//    addon content never reaches the publisher's boards. StartRemoteStorageUpload, StartRemoteStorageDownload and
//    PostToRacenet return at their entry while the session's content is not the game's own; with the bridge off the
//    Add/Update requests of an addon board are answered locally.
//
// 4. Online-state gate (third pass, FINDINGS "Third pass: offline gate"). With the launcher's offline component the game
//    runs without a platform session, and every F1os op's OnStart (103 methods share one template) finishes with an
//    AccountServiceOperation state instead of sending: 10 no presence record, 8 EA login error, 9 platform overlay disabled
//    (the "Platform Client Overlay is not enabled" popup), 4/5/7 no session token. For the five Time Trial methods only,
//    and only for a request the bridge answers itself, a mid hook at the first of those checks resumes at the first
//    instruction after the last one, so the op builds its request and reaches Game_F1os_RequestRing_Push, where the push
//    hook swallows it. Nothing else changes: other methods and the publisher's own boards keep the game's checks.
// 5. SWOFTY_TT_DIAG=1 logs only: the gate values per Time Trial request, every push, the board XML, the popup trigger,
//    the ghost handler of the storage system. No behaviour changes in that mode.
//
// Every hook point is found by byte scan, gated on the KB build. SWOFTY_TT_UPLOAD_GUARD=0, SWOFTY_TT_DOWNLOAD_GUARD=0,
// SWOFTY_TT_RACENET_GUARD=0 and SWOFTY_TT_REQUEST_GUARD=0 turn the guards off.

namespace {

namespace tb = runtime::tt_bridge;

const runtime::Sig S_XML{"site_Game_LeaderboardXml_Loaded",
                         "48 89 8C 24 60 01 00 00 48 89 84 24 68 01 00 00 C5 C8 57 F6 C5 F8 11 B4 24 70 01 00 00", 0x30daa44, runtime::Sig::addr, 0, 16};
const runtime::Sig S_XML_FREE{"site_Game_LeaderboardXml_BufferFreed", "48 8B 54 24 70 48 85 D2 74 09 48 8B 0E 48 8B 01 FF 50 38 48 8B 8C 24 88 00 00 00", 0x30dae60};
const runtime::Sig S_PUSH{"Game_F1os_RequestRing_Push", "41 57 41 56 56 57 53 48 83 EC 30 4C 89 CF 4C 89 C0 48 89 D6", 0x8293d0};
const runtime::Sig S_UPLOAD{"Game_GhostLapHandler_StartRemoteStorageUpload", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 08 04 00 00 48 89 D7 48 89",
                            0x2fa3de0};
const runtime::Sig S_DOWNLOAD{"Game_GhostLapHandler_StartRemoteStorageDownload", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC C8 01 00 00 48 89 CE 80 3D",
                              0x2fa0320};
const runtime::Sig S_RACENET{"Game_GhostLaps_PostToRacenet", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 38 06 00 00 C5 F8 29 B4 24 20 06 00 00 48 8B 41 08",
                             0x2f8d030};
const runtime::Sig S_PB{"site_Game_GhostPb_Committed", "C6 46 66 01 80 3D ?? ?? ?? ?? 00 0F 84", 0x2fa3db8, runtime::Sig::addr, 0, 0};
// The five F1os result handlers are instances of one template: their code is the same but for rip displacements, so no pattern of their
// own is unique. Each is found from the nearest function before it that has a unique pattern (anchor = the distance to the handler, the same
// in the Steam and EA links) and then checked against the template's entry bytes (on_result_entry).
const runtime::Sig S_GET_TT{"Game_F1os_OnResult_GetTTLeaderboard",
        "56 48 83 EC 30 48 89 CE 80 BA 88 00 00 00 00 8B 05 ?? ?? ?? ?? 89 C1 74 03 8B 4A 08 84 C9 75 29 48 83 BA A0 00 00 00 00 74 1F 48 81 C2 90 00 00 00 48 8D 8E 50 01 00 00",
        0x3021580, runtime::Sig::addr, 0, 0x5a90};
const runtime::Sig S_GET_FRIENDS{"Game_F1os_OnResult_GetFriendsTTLeaderboard",
        "56 48 83 EC 30 48 89 CE 80 BA 88 00 00 00 00 8B 05 ?? ?? ?? ?? 89 C1 74 03 8B 4A 08 84 C9 75 29 48 83 BA A0 00 00 00 00 74 1F 48 81 C2 90 00 00 00 48 8D 8E A8 01 00 00",
        0x30172e0, runtime::Sig::addr, 0, 0x5ab0};
const runtime::Sig S_GET_BINARY{"Game_F1os_OnResult_GetBinaryDataForUser",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 38 48 89 CE 48 39 CA",
        0x2ffe480, runtime::Sig::addr, 0, 0x5b90};
const runtime::Sig S_ADD_ENTRY{"Game_F1os_OnResult_AddLeaderboardEntry",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 48 48 89 CE 48 8B 59 08",
        0x300df00, runtime::Sig::addr, 0, 0x5ac0};
const runtime::Sig S_UPDATE_BINARY{"Game_F1os_OnResult_UpdateLeaderboardEntryBinaryData",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 48 48 89 CE 4C 8B 61 08",
        0x3006cd0, runtime::Sig::addr, 0, 0x6b70};

const runtime::Sig S_SESSION_ENTER{"Game_GhostLapsStorageSystem_OnSessionEnter",
        "41 57 41 56 41 54 56 57 55 53 48 81 EC 80 00 00 00 48 89 CE 80 3D ?? ?? ?? ?? 01 75 6A B8 62 00 00 00", 0x2f95ee0};
const runtime::Sig S_ACCOUNT_ERROR{"Game_F1os_AccountStateError",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC A8 00 00 00 C5 F8 29 B4 24 90 00 00 00 48 89 CE 48 8B 81 98 00 00 00 48 85 C0 74 16 "
        "48 8B 40 10 48 8B 40 38 48 85 C0 74 09 80 38 00 0F 85 69 05 00 00 0F B6 86 B8 01 00 00", 0x336da00};

const runtime::Sig S_FINDER_GATE{"site_Game_GhostLapHandler_RivalFinderOnline",
        "8B 87 A0 00 00 00 20 CB 0F 85 8C 00 00 00 83 F8 01 0F 84 83 00 00 00 48 89 F9 BA 01 00 00 00 E8", 0x2f98ec6, runtime::Sig::addr, 0, 6};
const runtime::Sig S_TS_FLAG{"site_Game_TrackSelectVM_CanViewLeaderboard",
        "48 8B 05 ?? ?? ?? ?? 80 78 18 00 74 04 31 C0 EB 0D 48 8B 05 ?? ?? ?? ?? 0F B6 40 18 34 01 88 44 24 50", 0x1f27937, runtime::Sig::addr, 0, 30};
const runtime::Sig S_SELECT_BOARD{"site_Game_TimeTrialLeaderboard_SelectBoard_Search",
        "48 8B 83 78 04 00 00 48 8B 58 10 31 ED 48 85 DB 74 47 4C 8B 73 18 4C 3B 73 20", 0x2fce993, runtime::Sig::addr, 0, 11};
const runtime::Sig S_WRITE_READY{"FUN_142fa0100_LeaderboardWriteReady",
        "56 57 53 48 83 EC 20 48 89 CF 48 8B 41 10 48 8B 40 40 48 8B 80 40 01 00 00 48 8B 80", 0x2fa0100};
const runtime::Sig S_WRITE_TOKEN{"site_Game_GhostLapHandler_Update_SessionTokenTest",
        "80 B9 90 00 02 00 00 74 14 48 8B 89 50 01 02 00 48 85 C9 74 08 80 39 00 0F 95 C1 EB 02 31 C9 40 20 DF 40 80 FF 01", 0x2f99099, runtime::Sig::addr, 0, 31};
const runtime::Sig S_READER_FAIL{"Game_LeaderboardReader_OnTaskCancelledOrFailed",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 48 02 00 00 C5 F8 29 B4 24 30 02 00 00 48 89 D6 48 89 CF 80 3D", 0x2fc0ed0};
const runtime::Sig S_HANDLER_READY{"Game_GhostLapHandler_PbReady_142f91900",
        "56 57 55 53 48 83 EC 28 48 89 CE 48 8B 41 78 48 85 C0 75 17 48 8D 4C 24", 0x2f91900};
const runtime::Sig S_GARAGE_MSG{"Game_GarageMonitor_OnMessage_141f76ac0",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 48 01 00 00 48 8B 02 48 85", 0x1f76ac0};
const runtime::Sig S_SAVE_PB{"Game_GhostLaps_SavePersonalBest", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 58 01 00 00 49 89 D6", 0x2fa31e0};
const runtime::Sig S_HANDLER_RESET{"Game_GhostLapHandler_Reset", "41 56 56 57 53 48 81 EC C8 00 00 00 C5 F8 29 B4 24 B0 00 00 00 44", 0x2f91dd0};
const runtime::Sig S_ENTRY_WRITE{"Game_GhostLapHandler_StartLeaderboardEntryWrite", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 48 08 00 00 C5 F8 29", 0x2fa0a60};
const runtime::Sig S_WATCHDOG_REARM{"site_Game_SessionTimer_Rearm_1409d2790",
        "48 89 86 88 0B 00 00 48 83 BE 90 0B 00 00 00 74 07 48 89 86 90 0B 00 00 F6 86 C8 0B 00 00 01 0F 94 86 C0 0B 00 00", 0x9d2790};
const runtime::Sig S_WATCHDOG_TRIP{"site_Game_SessionTimer_Trip_1409d2853",
        "F6 86 A8 0B 00 00 01 0F 94 86 A0 0B 00 00 48 83 C4 30 5B 5D 5F 5E 41 5C 41 5E 41 5F C3", 0x9d284c};
const runtime::Sig S_GARAGE_CHECK_DONE{"site_Game_GarageCockpitViewModel_TtPrivilegeCheckDone",
        "0F B6 43 08 48 C7 84 24 30 01 00 00 00 00 00 00 48 8D 0D ?? ?? ?? ?? 48 89 8C 24 28 01 00 00 88 84 24 38 01 00 00", 0x1bc5481};
const runtime::Sig S_GARAGE_CMD{"site_Game_GarageCockpitViewModel_OnCommandRaised_entry",
        "4C 89 C3 48 89 D7 48 89 CE 48 8B 02 48 85 C0 74 27 8B 00 3D 7C DC 61 08", 0x1bc514a};
const runtime::Sig S_GARAGE_UPDATE{"site_Game_GarageMonitorPanel_Update_141f76f09",
        "48 89 CE 0F B6 99 48 09 00 00 48 8B 81 70 02 00 00 4C 8B A0 88 00 00 00", 0x1f76f09};
const runtime::Sig S_GARAGE_TAB_BLOCK{"site_Game_GarageMonitorPanel_BuildTabs_leaderboard_block_141f720ab", "80 79 18 00 0F 85 ?? ?? ?? ?? 48 B9 00 00 00 00 20 00 00 00 48 23 08 0F 84 ?? ?? ?? ??", 0x1f720ab, runtime::Sig::addr, 0, 0};
const runtime::Sig S_GARAGE_TAB_FLAGS{"site_Game_GarageMonitorPanel_BuildTabs_leaderboard_feature_141f720b5", "48 B9 00 00 00 00 20 00 00 00 48 23 08 0F 84 ?? ?? ?? ??", 0x1f720b5};
const runtime::Sig S_GARAGE_TAB_GATE{"site_Game_GarageMonitorPanel_BuildTabs_gate_byte_141f7208e", "80 B9 80 00 00 00 00 0F 84 ?? ?? ?? ?? 83 FD 12 0F 85 ?? ?? ?? ??", 0x1f7208e};
const runtime::Sig S_REPLAY_TICK{"Ne_ReplaySystem_ResetSlot24_1445d57e0", "56 48 83 EC 20 80 B9 89 00 00 00 00 0F 84 90 00 00 00 48 89 CE 80 3D", 0x45d57e0};
const runtime::Sig S_REPLAY_TICK17{"Ne_ReplaySystem_ResetSlot17_1445d5b90", "56 48 83 EC 20 80 B9 89 00 00 00 00 74 5F 48 89 CE 80 3D ?? ?? ?? ?? 01 75 6C B8 24 00 00 00 48 8D 0D ?? ?? ?? ?? 66 2E 0F 1F 84 00 00 00 00 00 8B 14 81 81 FA D3 AF 3F 84 74 38 85 D2 74 47 83 C0 0E 48 69 D0 43 43 66 5F 48 C1 EA 20 41 89 C0 41 29 D0 41 D1 E8 41 01 D0 41 C1 E8 08 41 69 D0 75 01 00 00 29 D0 83 F8 24 75 C5 EB 19 48 83 C4 20 5E C3 48 8D 0D ?? ?? ?? ?? 48 8D 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 8B 8E B0 00 00 00 FF 15 ?? ?? ?? ?? 48 89 F1 E8 ?? ?? ?? ?? 48 8B 8E B0 00 00 00 48 83 C4 20 5E 48 FF 25 ?? ?? ?? ?? CC CC 41 57", 0x45d5b90};
const runtime::Sig S_REPLAY_UPDATE{"Game_ReplaySystem_UpdateFromModule_1445d6040",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC D8 00 00 00 C5 78 29 AC 24 C0 00 00 00 C5 78 29 A4 24 B0 00 00 00 C5 78 29", 0x45d6040};
const runtime::Sig S_REPLAY_FRAME{"Game_ReplayBuffer_BeginFrameWrite_1445c4b20", "41 56 56 57 53 48 83 EC 68 C5 F8 29 74 24 50 C5 F9 6F F1 48 89 CE 80 79 51 00 74 30", 0x45c4b20};
const runtime::Sig S_REPLAY_SAVE{"Game_ReplayManager_SaveFullReplay_142c46880", "41 56 56 57 53 48 81 EC 38 02 00 00 80 B9 E1 06 00 00 00 74 46 48 89 CE 48 83 B9 50 01 00 00 00", 0x2c46880};
const runtime::Sig S_REPLAY_JOB{"Game_Replay_SaveFullReplay_job_142c46a00", "41 57 41 56 41 55 41 54 56 57 55 53 B8 08 14 00 00 E8 ?? ?? ?? ?? 48 29 C4 C5 79 29 8C 24 F0 13 00 00", 0x2c46a00};
const runtime::Sig S_REPLAY_JOB_COMPONENT{"site_Game_Replay_SaveFullReplay_session_component_142c46c02",
        "48 8B 46 28 48 8B 80 88 00 00 00 48 85 C0 0F 84 ?? ?? ?? ?? 48 8B 80 60 02 00 00 48 89 84 24 98 00 00 00 48 85 C0 0F 84 ?? ?? ?? ?? 48 89 4C 24 68 48 8B 84 24 98 00 00 00 4C 8B B0 08 0F 00 00 49 8D 4E 18",
        0x2c46c02, runtime::Sig::addr, 0, 64};
const runtime::Sig S_REPLAY_JOB_WRAPPER{"site_Game_Replay_SaveFullReplay_component_wrapper_142c46c51", "84 C9 0F 84 91 00 00 00 49 8B AE 38 02 00 00 4D 8B B6 40 02 00 00 4C 39 F5", 0x2c46c51};
const runtime::Sig S_SAVE_SLOT_CTOR{"Game_SaveSlotManager_Construct_1429cd140",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC D8 00 00 00 C5 F8 29 B4 24 C0 00 00 00 4C 89 4C 24 78 4C 89 84 24 80 00 00 00 48 89 94 24 88 00 00 00 48 89 CE", 0x29cd140};
const runtime::Sig S_SAVE_SLOT_SNAPSHOT{"Game_SaveSlotManager_SnapshotSlot_1429ce380",
        "41 57 41 56 41 55 41 54 56 57 55 53 B8 28 12 00 00 E8 ?? ?? ?? ?? 48 29 C4 C5 F8 29 B4 24 10 12 00 00 F0 FF 05", 0x29ce380};
// The fastest lap viewer's enter (0x142d9cee0), at its first check: rax = session host, rsi = the state.
const runtime::Sig S_VIEW_ENTER{"site_Game_SessionStateViewFastestLap_enter_142d9cf10",
        "48 8B 80 60 02 00 00 80 B8 2E 0E 00 00 00 74 10 48 8B 8E 60 0D 00 00 80 B9 88 00 00 00 00 74 19", 0x2d9cf10, runtime::Sig::addr, 0, 7};
// The pause menu's flying lap callback (0x141d59e90, no arguments): posts the request that puts the player's car on a flying lap.
// The replay loader's save-data load of the file's info block (0x142c4c710): (manager, block, size, container) -> status, 0 = loaded.
const runtime::Sig S_REPLAY_INFO_LOAD{"Game_Replay_LoadInfoBlock_142c4c710", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC F8 01 00 00 48 89 CB 48 8B 49 10 48 89 4C 24 40", 0x2c4c710};
const runtime::Sig S_FLYING_LAP{"Game_Request_PlayerFlyingLap_141d59e90",
        "48 83 EC 38 48 8B 0D ?? ?? ?? ?? 31 D2 E8 ?? ?? ?? ?? 48 8B 0D ?? ?? ?? ?? 48 C7 44 24 28 00 00 00 00 48 8D 15 ?? ?? ?? ?? 48 89 54 24 20 88 44 24 30 C7 44 24 34 01 00 00 00",
        0x1d59e90};
// A session view model's first-update setup (0x1414bb420): it reads the game mode's current session without a null check.
const runtime::Sig S_VM_SESSION_SETUP{"Game_SessionVm_Setup_1414bb420",
        "41 57 41 56 56 57 53 48 81 EC 80 00 00 00 C5 F8 29 74 24 70 C6 41 14 FF 48 83 B9 B0 00 00 00 00 0F 84", 0x14bb420};
// The save job, right after it asked for the player's display name (rax): the name becomes the file's title, the Theatre card's first line.
const runtime::Sig S_REPLAY_TITLE{"site_Game_Replay_SaveFullReplay_title_142c47a31", "E8 ?? ?? ?? ?? 48 85 C0 48 8D 8C 24 C8 02 00 00", 0x2c47a31, runtime::Sig::addr, 0, 5};
const runtime::Sig S_REPLAY_META{"Game_Replay_FillMetadata_142bfd950",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 28 02 00 00 4C 89 4C 24 28 4C 89 C3 49 89 D7 48 89 CE", 0x2bfd950};
// The default-slot and owner-refresh functions have their bodies in the protector's section; every caller on both builds reaches each
// through one jump thunk in ordinary code (0x1429ce1c0, 0x1429d1130), found here from a call to it. The hooks sit on the thunks.
const runtime::Sig S_SAVE_SLOT_DEFAULT{"Game_SaveSlotManager_MakeDefaultSlot_thunk_1429ce1c0", "E8 ?? ?? ?? ?? 31 F6 E9 ?? ?? ?? ?? 48 C7 86 C8 02 00 00 00 00 00 00", 0x29ce1c0,
                                       runtime::Sig::rel32, 0, 1, 5};
const runtime::Sig S_SAVE_SLOT_REFRESH{"Game_SaveSlotManager_Refresh_thunk_1429d1130",
                                       "48 89 F1 E8 ?? ?? ?? ?? E9 ?? ?? ?? ?? 80 3D ?? ?? ?? ?? 01 0F 85 ?? ?? ?? ?? B8 80 00 00 00", 0x29d1130, runtime::Sig::rel32, 3, 4, 5};
const runtime::Sig S_REPLAY_LOAD_SESSION{"site_Game_Replay_LoadFullReplay_session_142c4b3dd", "41 BD 01 00 00 00 48 85 C0 0F 84 ?? ?? ?? ?? 49 89 C4 48 8B 88 08 0F 00 00", 0x2c4b3dd,
        runtime::Sig::addr, 0, 6};
const runtime::Sig S_REPLAY_LOAD_SLOT{"site_Game_Replay_LoadFullReplay_save_slot_142c4b3fc", "48 8D 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 85 C0 0F 84 ?? ?? ?? ?? 48 85 FF 74 1E 4C 8B 17", 0x2c4b3fc,
        runtime::Sig::addr, 0, 12};
const runtime::Sig S_REPLAY_LOAD_START{"Game_ReplayManager_StartLoadSavedReplay_142c45ec0", "56 57 53 48 81 EC 80 02 00 00 80 B9 E1 06 00 00 00 74 2B 48 89 D7 48 89 CE", 0x2c45ec0};
const runtime::Sig S_REPLAY_LOAD_READY{"site_Game_ReplayManager_StartLoadSavedReplay_ready_142c45f47", "31 DB 84 C0 0F 84 6F 01 00 00 48 83 BE 28 04 00 00 FF", 0x2c45f47};
const runtime::Sig S_REPLAY_JOB_SELECTED{"site_Game_Replay_SaveFullReplay_component_selected_142c46cf5", "4D 85 F6 0F 84 ?? ?? ?? ?? 4C 8B A2 C0 00 00 00 4C 8B AA D0 00 00 00", 0x2c46cf5};
const runtime::Sig S_REPLAY_JOB_STORES{"site_Game_Replay_SaveFullReplay_backing_stores_142c46ebd", "49 83 BC 24 B8 02 00 00 00 0F 84 ?? ?? ?? ?? 49 83 BD B8 02 00 00 00 0F 84", 0x2c46ebd};
const runtime::Sig S_REPLAY_JOB_ALLOC{"site_Game_Replay_SaveFullReplay_allocation_142c46f33", "48 85 C0 0F 84 ?? ?? ?? ?? 48 89 C5 48 8B 07 48 89 F9 FF 50 20", 0x2c46f33};
const runtime::Sig S_REPLAY_UI_START{"Game_ReplaySaveListener_ShowSaving_142c44220", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 28 01 00 00 48 89 CE C6 41", 0x2c44220};
const runtime::Sig S_REPLAY_UI_RESULT{"Game_ReplaySaveListener_ShowResult_142c44760", "41 57 41 56 41 55 41 54 56 57 53 48 81 EC D0 01 00 00 48 89 D3 48 89 CF", 0x2c44760};
const runtime::Sig S_REPLAY_PATH{"Game_ReplayManager_BuildSavePath_142c48860", "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC D8 05 00 00 48 89 D6 48 89 CB 48 8D 79 20", 0x2c48860};
const runtime::Sig S_GARAGE_BUILD{"Game_GarageMonitorPanel_BuildTabs_141f71fa0",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 98 00 00 00 C5 F8 29 B4 24 80 00 00 00 48 89 CE 80 B9 49 09 00 00 00", 0x1f71fa0};
const runtime::Sig S_GARAGE_FRIENDS_TAB{"site_Game_GarageMonitorPanel_BuildTabs_friends_tab_141f725e9",
        "48 8B 8E 80 02 00 00 48 8B 01 BA E0 11 00 00 41 B8 10 00 00 00 FF 50 18 48 89 C7 48 85 C0 74 1E 4C 8B 86 80 02 00 00 48 8B 56 28 C7 44 24 20 01 00 00 00", 0x1f725e9};
const runtime::Sig S_GARAGE_PRIV_SUB{"Game_GarageMonitorPanel_OnTtLeaderboardPrivilege_thunk_141f79690",
        "48 8D 05 ?? ?? ?? ?? 48 89 44 24 30 48 89 74 24 38 48 8D 15 ?? ?? ?? ?? 4C 8D 44 24 30 E8 ?? ?? ?? ?? 48 8B 0D", 0x1f79690, runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig S_PRIV_GATE{"site_Game_UIUIPrivilegeCheckControl_HandleSignal_Result",
        "41 80 BE AB 00 00 00 00 0F 84 8C 00 00 00 45 84 FF 0F 84 8D 00 00 00", 0x3822cdf, runtime::Sig::addr, 0, 8};
// Addresses the replay code calls or compares. Each is found once at setup (resolve_game) by a pattern that matches once on the Steam and on
// the EA build; a function whose entry is a jump thunk is found from a call to it, a vtable or global from an instruction that names it.
const runtime::Sig S_QPC_OFFSET{"g_Timer_QpcOffset_1461089f8", "48 03 05 ?? ?? ?? ?? C4 E1 F3 2A C0 C5 FB 59 C7 C5 FB 5E C6 C4 C1 79 2E C0", 0x61089f8, runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig S_FRIENDS_RESUME{"site_Game_GarageMonitorPanel_BuildTabs_after_friends_tab_141f726ea", "48 8B 8E 60 09 00 00 E8 ?? ?? ?? ?? 80 B8 DB 00 00 00 00", 0x1f726ea};
const runtime::Sig S_REPLAY_SYSTEM_VT{"vt_GameReplaySystem_1457cf220", "48 8D 05 ?? ?? ?? ?? 48 89 01 48 8B 91 70 01 00 00 48 85 D2 74 29 48 8D 9E 70 01 00 00", 0x57cf220,
                                      runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig S_STRING_VT{"vt_Game_FixedString64_145613548",
                               "48 8D 05 ?? ?? ?? ?? 48 89 44 24 48 48 8D 8C 24 F8 00 00 00 48 C7 84 24 E8 00 00 00 40 00 00 00 48 89 8C 24 F0 00 00 00 48 B9 73 74 61 74 73 5F 64 61",
                               0x5613548, runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig S_MODE_VT{"vt_Game_SessionGameMode_145648300", "48 8D 05 ?? ?? ?? ?? 48 89 01 48 8D 1D ?? ?? ?? ?? 48 89 99 50 11 00 00", 0x5648300, runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig S_QUEUE_BUSY{"Game_JobQueue_Busy_14391dda0", "56 57 48 83 EC 28 48 89 CE 48 8B 49 18 FF 15", 0x391dda0};
const runtime::Sig S_REPLAY_SET_STATE{"Ne_ReplaySystem_SetState_1445c74b0", "56 57 53 48 83 EC 20 44 89 C3 89 D6 48 89 CF 48 8B 89 B0 00 00 00", 0x45c74b0};
const runtime::Sig S_REPLAY_SEEK{"Ne_ReplaySystem_Seek_1445cafc0", "56 57 48 83 EC 38 89 D7 48 89 CE 48 8B 89 B0 00 00 00", 0x45cafc0};
const runtime::Sig S_REPLAY_CLEAR{"Ne_ReplaySystem_Clear_thunk_1445d4c60", "E8 ?? ?? ?? ?? 48 8B 8E B0 00 00 00 FF 15 ?? ?? ?? ?? 48 8B 05 ?? ?? ?? ?? 48 8B 78 08", 0x45d4c60,
                                  runtime::Sig::rel32, 0, 1, 5};
const runtime::Sig S_REPLAY_RESET{"Ne_ReplaySystem_Reset_1445d5c40", "41 57 41 56 41 54 56 57 55 53 48 83 EC 40 44 89 C3 89 D7", 0x45d5c40};
const runtime::Sig S_VIEW_ENTER_FN{"Game_SessionStateViewFastestLap_enter_142d9cee0", "41 57 41 56 41 55 41 54 56 57 53 48 83 EC 30 48 89 CE 66 C7 81 78 0F 00 00 00 00", 0x2d9cee0};
const runtime::Sig S_VIEW_RESUME{"site_Game_SessionStateViewFastestLap_enter_resume_142d9d230", "48 8B BE 88 0C 00 00 80 3D ?? ?? ?? ?? 01 75 65", 0x2d9d230};

struct GameAddresses {
    uintptr_t qpc_offset = 0, friends_resume = 0, replay_system_vt = 0, string_vt = 0, mode_vt = 0, queue_busy = 0, replay_set_state = 0, replay_seek = 0, replay_clear = 0,
              replay_reset = 0, view_enter_fn = 0, view_resume = 0;
    bool view_ready() const { return queue_busy && replay_set_state && mode_vt && view_enter_fn && view_resume; }
} game;

int queue_busy(uint64_t queue)
{
    return game.queue_busy ? reinterpret_cast<int (*)(uint64_t)>(game.queue_busy)(queue) : -1;
}

void replay_set_state(uint64_t sys, uint32_t state, uint32_t flag)
{
    if (game.replay_set_state)
        reinterpret_cast<void (*)(uint64_t, uint32_t, uint32_t)>(game.replay_set_state)(sys, state, flag);
}

bool is_replay_system(uint64_t sys)
{
    return game.replay_system_vt && sys && runtime::readable(reinterpret_cast<void *>(sys), 8) && *reinterpret_cast<const uint64_t *>(sys) == game.replay_system_vt;
}
constexpr uintptr_t TS_FLAG_GLOBAL_A = 0, TS_FLAG_GLOBAL_B = 17;
constexpr int64_t TS_FORCED_WINDOW_MS = 10 * 60 * 1000;

constexpr uintptr_t PB_HANDLER_ID_BLOCK = 0x298, PB_HANDLER_TIME = 0x300;

using OnResult_t = void (*)(void *op, void *resp);

struct Resolved {
    uintptr_t on_result[6] = {};
};

std::mutex lock;
tb::Table table(1);
Resolved found;
bool bridge_on = false, boards_on = false, guards_on = false, request_guard = false, push_hooked = false;
std::atomic<bool> pb_armed{false};
std::string last_board, session_code;
std::atomic<bool> session_custom{false}, track_watch{false}, track_session{false};
std::atomic<int64_t> track_checked_ms{0}, diag_checked_ms{0};
std::atomic<bool> unknown_logged{false};
std::atomic<int> pending_count{0};
std::deque<std::unique_ptr<tb::Response>> responses;
std::deque<std::string> xml_buffers;
std::map<uint64_t, uint64_t> op_vtables;
std::atomic<int> handle_logged{0}, quiet_logged{0};
uint8_t ghost_block[tb::ID_BLOCK] = {};
bool have_ghost_block = false;
std::wstring outbox, inbox;
std::atomic<int> logged_requests{0}, logged_unreadable{0}, logged_passes{0}, xml_logged{0}, pb_logged{0};
std::atomic<int> hits_upload{0}, hits_download{0}, hits_racenet{0};
std::vector<std::string> seen_methods;

bool diag_on = false;
bool replay_capture_on = false;
std::atomic<int64_t> replay_pending_ms{0};
std::atomic<uint32_t> replay_pending_lap{0};
safetyhook::InlineHook h_rui_start, h_rui_result;
safetyhook::MidHook h_garage_done, h_garage_cmd, h_garage_update, h_tab_block, h_tab_flags, h_tab_friends, h_tab_build, h_tab_gate, h_replay_tick, h_replay_tick17, h_replay_update, h_replay_frame, h_replay_save, h_replay_job, h_job_component, h_job_wrapper, h_load_session, h_load_slot, h_job_selected, h_job_stores, h_job_alloc, h_finder, h_ts_flag, h_priv, h_select, h_write_token, h_reader_fail, h_rearm, h_trip, h_save_pb, h_reset, h_entry_write, h_xml, h_push, h_upload, h_download, h_racenet, h_pb, h_session_enter, h_account_error;

struct GateHook {
    uintptr_t at = 0, resume = 0, session_global = 0;
    safetyhook::MidHook hook;
};
GateHook gates[5];
std::atomic<uintptr_t> diag_storage{0};
uintptr_t ts_global_a = 0, ts_global_b = 0;
std::atomic<int64_t> ts_forced_ms{0}, enter_ms{0};
std::string ts_seen, priv_seen, select_seen, ready_seen;
safetyhook::InlineHook h_write_ready, h_ready;
safetyhook::MidHook h_garage;
bool lookup_logged = true;
std::atomic<int> finder_logged{0};
uintptr_t diag_handler_seen = ~uintptr_t(0);
std::atomic<int> diag_gate_logged{0}, diag_push_logged{0}, diag_error_logged{0}, diag_bypass_logged{0};
std::vector<std::string> diag_gate_seen;

bool read_mem(void *, uint64_t addr, void *out, size_t n)
{
    if (!runtime::readable(reinterpret_cast<const void *>(addr), n))
        return false;
    memcpy(out, reinterpret_cast<const void *>(addr), n);
    return true;
}

tb::Mem game_mem() { return {&read_mem, nullptr}; }

bool env_off(const char *name, const char *label)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    if (!n || n >= sizeof v || !tb::switch_off(v))
        return false;
    swofty::log("[boards] %s off (%s=%s)", label, name, v);
    return true;
}

int64_t now_ms() { return int64_t(GetTickCount64()); }

uint64_t unix_seconds()
{
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    uint64_t t = (uint64_t(ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
    return t / 10000000ull - 11644473600ull;
}

void make_dirs()
{
    std::wstring root = runtime::swofty_root();
    CreateDirectoryW((root + L"tt").c_str(), nullptr);
    outbox = root + L"tt\\outbox\\";
    inbox = root + L"tt\\inbox\\";
    CreateDirectoryW(outbox.c_str(), nullptr);
    CreateDirectoryW(inbox.c_str(), nullptr);
}

std::string current_content()
{
    auto facts = swofty_core::hooks::core().current();
    return facts ? facts->content : std::string();
}

bool content_ours()
{
    return session_custom.load(std::memory_order_relaxed) || tb::addon_content(current_content());
}

std::string hex_dump(const uint8_t *p, size_t n)
{
    std::string s;
    char b[4];
    for (size_t i = 0; i < n; i++) {
        snprintf(b, sizeof b, "%02x", p[i]);
        s += b;
        if (i % 8 == 7 && i + 1 < n)
            s += ' ';
    }
    return s;
}

std::string rva_list(void **frames, int n)
{
    std::string s;
    uintptr_t base = swofty::api->module_base();
    for (int i = 0; i < n; i++) {
        char b[24];
        uintptr_t a = reinterpret_cast<uintptr_t>(frames[i]);
        snprintf(b, sizeof b, "%s+0x%llx", s.empty() ? "" : " ", (unsigned long long)(a - base));
        s += b;
    }
    return s;
}

std::string backtrace_text(int skip, int depth)
{
    void *frames[12] = {};
    USHORT n = RtlCaptureStackBackTrace(DWORD(skip), DWORD(std::min(depth, 12)), frames, nullptr);
    return rva_list(frames, int(n));
}

uint64_t peek64(uint64_t at)
{
    uint64_t v = 0;
    if (runtime::readable(reinterpret_cast<void *>(at), 8))
        memcpy(&v, reinterpret_cast<void *>(at), 8);
    return v;
}

uint32_t peek32(uint64_t at)
{
    uint32_t v = 0;
    if (runtime::readable(reinterpret_cast<void *>(at), 4))
        memcpy(&v, reinterpret_cast<void *>(at), 4);
    return v;
}

void xml_mid(safetyhook::Context &c)
{
    if (!bridge_on && !diag_on)
        return;
    uint64_t len = 0, data = 0;
    uint64_t at_data = c.rsp + 0x160, at_len = c.rsp + 0x168;
    if (!runtime::readable(reinterpret_cast<void *>(at_data), 16))
        return;
    memcpy(&data, reinterpret_cast<void *>(at_data), 8);
    memcpy(&len, reinterpret_cast<void *>(at_len), 8);
    if (len != c.rax || data != c.rcx) {
        swofty::log("[boards] board definitions: the parser's copy of the buffer (data 0x%llx, length %llu) does not match the loaded one (rcx 0x%llx, rax %llu); left alone",
                    (unsigned long long)data, (unsigned long long)len, (unsigned long long)c.rcx, (unsigned long long)c.rax);
        return;
    }
    if (!bridge_on) {
        size_t boards = 0;
        std::string first;
        if (len >= 16 && len <= (size_t(32) << 20) && runtime::readable(reinterpret_cast<void *>(data), len)) {
            auto els = tb::leaderboard_elements(std::string(reinterpret_cast<const char *>(data), size_t(len)));
            boards = els.size();
            first = els.empty() ? "" : els.front().name;
        }
        swofty::log("[boards] diag: system/leaderboard.xml loaded by the game (%llu bytes, %zu boards, first %s); the board definitions are read at network subsystem "
                    "connect, online or not",
                    (unsigned long long)len, boards, first.c_str());
        return;
    }
    int n = ++xml_logged;
    if (len < 16 || len > (size_t(32) << 20) || !runtime::readable(reinterpret_cast<void *>(data), len)) {
        swofty::log("[boards] board definitions: the buffer at the load site is not readable (data 0x%llx, length %llu); left alone",
                    (unsigned long long)data, (unsigned long long)len);
        return;
    }
    std::string text(reinterpret_cast<const char *>(data), size_t(len));
    std::vector<int> descriptors;
    size_t dropped = 0;
    for (int64_t id : swofty_core::apply::ids::store().ids_of(swofty_core::apply::content::LEADERBOARD_IDS)) {
        if (id >= tb::FIRST_ADDON_DESCRIPTOR && id <= tb::LAST_NAMED_DESCRIPTOR)
            descriptors.push_back(int(id));
        else if (id > tb::LAST_NAMED_DESCRIPTOR)
            dropped++;
    }
    std::vector<int> custom_ids;
    const auto &track_base = swofty_core::apply::tracks::base();
    if (track_base.ready)
        for (const auto &t : track_base.tracks)
            if (t.kind != swofty_core::apply::tracks::Kind::stock && t.stat_id >= 0)
                custom_ids.push_back(t.stat_id);
    tb::XmlReport rep;
    std::string out = tb::extend_xml(text, descriptors, custom_ids, rep, true);
    if (!rep.template_name.empty() && n == 1) {
        for (auto &e : tb::leaderboard_elements(text))
            if (e.name == rep.template_name) {
                std::string t = text.substr(e.begin, std::min<size_t>(e.end - e.begin, 400));
                for (char &ch : t)
                    if (ch == '\r' || ch == '\n')
                        ch = ' ';
                swofty::log("[boards] board definitions: %zu bytes, template element %s: %s", text.size(), rep.template_name.c_str(), t.c_str());
                break;
            }
    }
    if (!rep.changed) {
        swofty::log("[boards] board definitions: left alone (%s); %zu addon descriptor(s), %zu stock track(s), %zu custom track(s)%s%s",
                    rep.why.c_str(), rep.descriptors, rep.tracks, rep.custom_tracks,
                    track_base.ready ? "" : " (the track base is not ready)", dropped ? ", descriptors above 99 have no name form" : "");
        return;
    }
    xml_buffers.push_back(std::move(out));
    if (xml_buffers.size() > 4)
        xml_buffers.pop_front();
    const std::string &keep = xml_buffers.back();
    uint64_t new_len = keep.size();
    uint64_t new_data = reinterpret_cast<uint64_t>(keep.c_str());
    memcpy(reinterpret_cast<void *>(at_data), &new_data, 8);
    memcpy(reinterpret_cast<void *>(at_len), &new_len, 8);
    swofty::log("[boards] board definitions: appended %zu board(s) for %zu addon descriptor(s) over %zu track(s) (ids %lld..%lld, %zu already "
                "defined%s), %zu of them for %zu custom track(s) (all addon descriptors and %zu stock descriptor(s)), buffer %zu -> %zu bytes",
                rep.added, rep.descriptors, rep.tracks, (long long)rep.first_id, (long long)rep.last_id, rep.existing,
                rep.capped ? (", capped " + std::to_string(rep.capped)).c_str() : "", rep.custom_added, rep.custom_tracks,
                rep.stock_descriptors, text.size(), keep.size());
}

void log_unreadable(uint64_t op, tb::Method m, const std::string &name)
{
    if (++logged_unreadable > 4)
        return;
    uint8_t raw[0x30] = {};
    uint64_t base = peek64(op + tb::ARGS_AT_OP) + tb::board_slot(m);
    bool ok = runtime::readable(reinterpret_cast<void *>(base), sizeof raw);
    if (ok)
        memcpy(raw, reinterpret_cast<void *>(base), sizeof raw);
    swofty::log("[boards] request %s: LeaderboardID not readable as a board name (slot %s: %s); passed through", name.c_str(),
                ok ? "bytes" : "unreadable", ok ? hex_dump(raw, sizeof raw).c_str() : "");
}

void push_mid(safetyhook::Context &c)
{
    uint64_t op = c.rcx;
    std::string name;
    tb::Method m = tb::method_at_op(game_mem(), op, &name);
    if (diag_on && ++diag_push_logged <= 60) {
        uint64_t sp = peek64(op + tb::OP_SERVICE);
        std::string service = sp ? tb::read_c(game_mem(), sp) : std::string();
        swofty::log("[boards] diag: RequestRing_Push op 0x%llx service %s method %s%s; caller %s", (unsigned long long)op, service.empty() ? "?" : service.c_str(),
                    name.empty() ? "?" : name.c_str(), m != tb::Method::none ? " (Time Trial board method)" : "", backtrace_text(1, 3).c_str());
    }
    if (m == tb::Method::none) {
        if (!name.empty() && seen_methods.size() < 40 && std::find(seen_methods.begin(), seen_methods.end(), name) == seen_methods.end()) {
            seen_methods.push_back(name);
            swofty::log("[boards] request %s passes (not a Time Trial board method)", name.c_str());
        }
        return;
    }
    tb::Args a = tb::decode_args(game_mem(), op, m);
    if (!a.ok) {
        log_unreadable(op, m, name);
        return;
    }
    std::string code;
    {
        std::lock_guard<std::mutex> g(lock);
        last_board = a.board_text;
        code = session_code;
    }
    tb::BoardTrack track = tb::request_track(swofty_core::apply::tracks::base(), a.board, code);
    tb::Route route = tb::route_of(m, a.board, bridge_on, track.custom || content_ours());
    if (route == tb::Route::local && !request_guard)
        route = tb::Route::pass;
    if (route == tb::Route::pass || !found.on_result[int(m)]) {
        if (route != tb::Route::pass)
            swofty::log("[boards] request %s %s: off, needs OnResult of the method (signature did not resolve); passed through", name.c_str(),
                        a.board_text.c_str());
        else if (++logged_passes <= 3)
            swofty::log("[boards] request %s %s passes (%s board, string form %s)", name.c_str(), a.board_text.c_str(),
                        tb::ours(a.board.descriptor, track.custom || content_ours()) ? "own" : "stock", tb::form_name(a.form));
        return;
    }
    uint64_t seq;
    {
        std::lock_guard<std::mutex> g(lock);
        seq = table.add(op, m, a, now_ms(), route == tb::Route::local);
        pending_count = int(table.size());
        op_vtables[seq] = peek64(op);
    }
    c.rax = 0;
    c.trampoline_rsp = c.rsp;
    if (++logged_requests <= 24)
        swofty::log("[boards] request %s %s redirected (seq %llu, %s, string form %s, track %s%s, args %s)", name.c_str(), a.board_text.c_str(),
                    (unsigned long long)seq, route == tb::Route::local ? "answered locally" : "to the mailbox", tb::form_name(a.form),
                    track.key.empty() ? "unknown" : track.key.c_str(), track.custom ? ", custom track" : "", swofty::json::dump(a.values).c_str());
}

void guard_entry(safetyhook::Context &c, const char *what, std::atomic<int> &hits)
{
    std::string content = current_content();
    if (!tb::ours_session(content, session_custom.load(std::memory_order_relaxed)))
        return;
    c.rax = 0;
    c.trampoline_rsp = c.rsp;
    tb::Say say = tb::say(++hits);
    if (say != tb::Say::none)
        swofty::log("[boards] guard: %s skipped for %s%s", what,
                    tb::addon_content(content) ? ("addon content " + content).c_str() : "a custom track in the game's own content",
                    say == tb::Say::last ? " (further hits are not logged)" : "");
}

void upload_mid(safetyhook::Context &c) { guard_entry(c, "remote storage upload", hits_upload); }
void download_mid(safetyhook::Context &c) { guard_entry(c, "remote storage download", hits_download); }
void racenet_mid(safetyhook::Context &c) { guard_entry(c, "Racenet post", hits_racenet); }

void pb_mid(safetyhook::Context &c)
{
    uint8_t blk[tb::ID_BLOCK];
    uint32_t ms = 0;
    uintptr_t h = c.rsi;
    if (!runtime::readable(reinterpret_cast<void *>(h + PB_HANDLER_ID_BLOCK), sizeof blk) ||
        !runtime::readable(reinterpret_cast<void *>(h + PB_HANDLER_TIME), 4))
        return;
    memcpy(blk, reinterpret_cast<void *>(h + PB_HANDLER_ID_BLOCK), sizeof blk);
    memcpy(&ms, reinterpret_cast<void *>(h + PB_HANDLER_TIME), 4);
    {
        std::lock_guard<std::mutex> g(lock);
        memcpy(ghost_block, blk, sizeof blk);
        have_ghost_block = true;
    }
    if (++pb_logged <= 6)
        swofty::log("[boards] personal best committed (time word 0x%08x): player id block %s", ms, hex_dump(blk, sizeof blk).c_str());
    if (pb_armed.load(std::memory_order_relaxed))
        runtime::lap_rec_ghost(h);
}

bool hook_mid(safetyhook::MidHook &slot, uintptr_t at, safetyhook::MidHookFn fn, const char *what)
{
    auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), fn);
    if (!h) {
        swofty::log("[boards] %s: off, the hook failed: %s", what, runtime::hook_error(h.error()).c_str());
        return false;
    }
    slot = std::move(*h);
    return true;
}

void install_guard(safetyhook::MidHook &slot, const runtime::Sig &sig, safetyhook::MidHookFn fn, const char *env, const char *what)
{
    if (env_off(env, what))
        return;
    uintptr_t at = runtime::find(sig, "boards", "boards");
    if (!at) {
        swofty::log("[boards] guard: %s off, needs %s (byte signature matched nothing unique)", what, sig.what);
        return;
    }
    if (hook_mid(slot, at, fn, what))
        swofty::log("[boards] guard: %s armed at base+0x%llx: it returns for addon content", what,
                    (unsigned long long)(at - swofty::api->module_base()));
}

bool file_age_seconds(const std::wstring &path, int64_t *age)
{
    WIN32_FILE_ATTRIBUTE_DATA d{};
    if (!GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &d))
        return false;
    ULARGE_INTEGER w;
    w.LowPart = d.ftLastWriteTime.dwLowDateTime;
    w.HighPart = d.ftLastWriteTime.dwHighDateTime;
    *age = int64_t(unix_seconds()) - int64_t(w.QuadPart / 10000000ull - 11644473600ull);
    return true;
}

tb::gate::State read_gate_state(const safetyhook::Context &c, const GateHook &g, uint32_t *flags, uint64_t *user, uint8_t *session_flag, std::string *token)
{
    tb::gate::State st;
    st.s60 = uint32_t(c.rdi);
    st.s64 = uint32_t(c.rax);
    *flags = 0;
    *user = 0;
    if (c.rcx) {
        *flags = peek32(c.rcx + 0x1c8);
        *user = peek64(c.rcx + 0x1c0);
        st.presence_ok = tb::gate::presence_ok(*flags, *user);
    }
    *session_flag = 0;
    token->clear();
    uint64_t sys = g.session_global ? peek64(g.session_global) : 0;
    uint64_t blk = sys ? peek64(sys + 0x4e0) : 0;
    if (blk) {
        uint64_t tok = peek64(blk + 0x20150);
        if (runtime::readable(reinterpret_cast<void *>(blk + 0x20090), 1))
            *session_flag = *reinterpret_cast<uint8_t *>(blk + 0x20090);
        char first = 0;
        if (tok && runtime::readable(reinterpret_cast<void *>(tok), 1))
            first = *reinterpret_cast<char *>(tok);
        st.session_ok = *session_flag != 0 && first != 0;
        *token = !tok ? "null" : first ? "set" : "empty";
    } else
        *token = "no block";
    return st;
}

runtime::presence_id::Id presence_now;
bool presence_known = false;

void log_presence_user(uint64_t user)
{
    if (!user)
        return;
    uint8_t obj[0x80] = {};
    if (!runtime::readable(reinterpret_cast<void *>(user), sizeof obj))
        return;
    memcpy(obj, reinterpret_cast<void *>(user), sizeof obj);
    uint64_t vt = 0;
    runtime::presence_id::Object o;
    memcpy(&vt, obj, 8);
    memcpy(&o.at18, obj + 0x18, 8);
    memcpy(&o.at20, obj + 0x20, 8);
    memcpy(&o.at28, obj + 0x28, 8);
    runtime::presence_id::Id id = runtime::presence_id::from_presence_object(o);
    {
        std::lock_guard<std::mutex> g(lock);
        presence_now = id;
        presence_known = true;
    }
    static std::atomic<int> logged{0};
    if (!diag_on || logged.load() >= 2 || logged.fetch_add(1) >= 2)
        return;
    std::vector<uint8_t> raw(obj, obj + sizeof obj);
    std::vector<uint8_t> stream = runtime::presence_id::serialise(id);
    swofty::log("[boards] diag: presence user object 0x%llx (vtable base+0x%llx), id as the game's getter (FUN_14411e3c0) builds it: type %u, flag39 %u, flag38 %u, "
                "value0 %llu (PlatformID of a row with Platform 1), value1 and value2 come from platform service strings and are not compared; serialised id "
                "stream (%zu bytes, the description's L bytes) hex %s base64 %s; raw object %s",
                (unsigned long long)user, (unsigned long long)(vt - swofty::api->module_base()), unsigned(id.type), unsigned(id.flag39), unsigned(id.flag38),
                (unsigned long long)id.value[0], stream.size(), runtime::presence_id::hex(stream).c_str(), runtime::presence_id::base64(stream).c_str(),
                runtime::presence_id::hex(raw).c_str());
}

void gate_mid(int idx, safetyhook::Context &c)
{
    tb::Method m = tb::Method(idx + 1);
    GateHook &g = gates[idx];
    uint64_t op = c.rsi;
    uint32_t flags;
    uint64_t user;
    uint8_t session_flag;
    std::string token;
    tb::gate::State st = read_gate_state(c, g, &flags, &user, &session_flag, &token);
    tb::gate::Why why = tb::gate::first_failure(st);
    log_presence_user(user);
    std::string name;
    tb::Method seen = tb::method_at_op(game_mem(), op, &name);
    tb::Args a;
    tb::Route route = tb::Route::pass;
    tb::BoardTrack track;
    if (seen == m) {
        a = tb::decode_args(game_mem(), op, m);
        if (a.ok) {
            std::string code;
            {
                std::lock_guard<std::mutex> lk(lock);
                code = session_code;
            }
            track = tb::request_track(swofty_core::apply::tracks::base(), a.board, code);
            route = tb::route_of(m, a.board, bridge_on, track.custom || content_ours());
            if (route == tb::Route::local && !request_guard)
                route = tb::Route::pass;
        }
    }
    bool bypass = bridge_on && route != tb::Route::pass && push_hooked && found.on_result[int(m)] && why != tb::gate::Why::none;
    if (diag_on || bypass) {
        char key[160];
        snprintf(key, sizeof key, "%d/%u/%u/%d/%d/%d", idx, st.s60, st.s64, st.presence_ok, st.session_ok, bypass);
        bool fresh = false;
        {
            std::lock_guard<std::mutex> lk(lock);
            if (std::find(diag_gate_seen.begin(), diag_gate_seen.end(), key) == diag_gate_seen.end() && diag_gate_seen.size() < 48) {
                diag_gate_seen.push_back(key);
                fresh = true;
            }
        }
        if (fresh || (diag_on && ++diag_gate_logged <= 24))
            swofty::log("[boards] diag: online-state gate of %s op 0x%llx board %s: presence record %s (flags 0x%x user 0x%llx), S+0x60 = %u, S+0x64 = %u, "
                        "session token %s (flag %u); first failure: %s; %s; caller %s",
                        tb::method_name(m), (unsigned long long)op, a.ok ? a.board_text.c_str() : "(args not read)",
                        st.presence_ok ? "ok" : c.rcx ? "not signed in" : "missing", flags, (unsigned long long)user, st.s60, st.s64, token.c_str(),
                        session_flag, tb::gate::why_text(why),
                        bypass ? "this request is ours: the checks are skipped" : route == tb::Route::pass ? "stock board or bridge off: left to the game" : "left to the game",
                        backtrace_text(1, 3).c_str());
    }
    if (!bypass)
        return;
    c.rip = g.resume;
    if (++diag_bypass_logged <= 12)
        swofty::log("[boards] gate %s %s: online-state checks skipped (would have failed with %s); the request goes to the push hook", tb::method_name(m),
                    a.board_text.c_str(), tb::gate::why_text(why));
}

template <int I> void gate_cb(safetyhook::Context &c) { gate_mid(I, c); }

const safetyhook::MidHookFn gate_cbs[5] = {gate_cb<0>, gate_cb<1>, gate_cb<2>, gate_cb<3>, gate_cb<4>};

void account_error_mid(safetyhook::Context &c)
{
    if (!runtime::readable(reinterpret_cast<void *>(c.rcx + 0x1b8), 1))
        return;
    unsigned state = *reinterpret_cast<uint8_t *>(c.rcx + 0x1b8);
    if (state == 0 || state == 1 || state == 7)
        return;
    bool quiet = tb::gate::account_error_quiet(bridge_on, state);
    if (diag_on && ++diag_error_logged <= 16)
        swofty::log("[boards] diag: account-state error handler: state %u %s%s%s; backtrace %s", state, tb::gate::account_state_name(state),
                    state == 9 ? " (the Platform Client Overlay popup)" : "",
                    quiet ? "; no popup: the offline launcher's content does not need the account service" : "; the game's handler runs", backtrace_text(1, 6).c_str());
    else if (quiet && ++quiet_logged <= 4)
        swofty::log("[boards] account-state error handler: state %u %s: its popup is not shown for this content (the leaderboard and ghosts use the bridge)", state,
                    tb::gate::account_state_name(state));
    if (quiet) {
        c.rax = 0;
        c.trampoline_rsp = c.rsp;
    }
}

void session_enter_mid(safetyhook::Context &c)
{
    diag_storage = c.rcx;
    enter_ms = now_ms();
    lookup_logged = false;
    std::string code;
    {
        std::lock_guard<std::mutex> lk(lock);
        code = session_code;
    }
    if (diag_on)
        swofty::log("[boards] diag: ghost lap storage system 0x%llx onSessionEnter (handler before: 0x%llx), session track '%s', content '%s', custom track %d",
                    (unsigned long long)c.rcx, (unsigned long long)peek64(c.rcx + 0x278), code.c_str(), current_content().c_str(), session_custom.load() ? 1 : 0);
}

void session_lookup_tick()
{
    int64_t at = enter_ms.load();
    uintptr_t sys = diag_storage.load();
    if (!at || !sys || lookup_logged || now_ms() - at < 3000)
        return;
    lookup_logged = true;
    uint64_t handler = peek64(sys + 0x278);
    std::string name;
    if (handler) {
        uint64_t ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
        name = np ? tb::read_c(game_mem(), np) : std::string();
    }
    swofty::log("[boards] diag: board lookup of onSessionEnter: %s%s", handler ? "found, board " : "no board found, the ghost lap handler stays 0",
                name.c_str());
}


void finder_mid(safetyhook::Context &c)
{
    uint8_t user_online = uint8_t(c.rbx), s60 = uint8_t(c.rbp), s64 = uint8_t(c.rcx);
    uint64_t handler = c.rsi, finder = c.rdi;
    uint64_t ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
    std::string board = np ? tb::read_c(game_mem(), np) : std::string();
    tb::BoardName bn = tb::parse_board(board);
    bool ours = bn.ok && tb::ours(bn.descriptor, content_ours());
    bool open = user_online && s60 && s64;
    bool bypass = bridge_on && ours && push_hooked && !open;
    char key[96];
    snprintf(key, sizeof key, "F/%u/%u/%u/%d", user_online, s60, s64, bypass);
    bool fresh = false;
    {
        std::lock_guard<std::mutex> lk(lock);
        if (std::find(diag_gate_seen.begin(), diag_gate_seen.end(), key) == diag_gate_seen.end() && diag_gate_seen.size() < 48) {
            diag_gate_seen.push_back(key);
            fresh = true;
        }
    }
    if ((diag_on || bypass) && fresh)
        swofty::log("[boards] %srival finder gate of handler 0x%llx board %s: user online %u, S+0x60 ok %u, S+0x64 ok %u, finder state %u -> %s",
                    diag_on ? "diag: " : "", (unsigned long long)handler, board.empty() ? "?" : board.c_str(), user_online, s60, s64, peek32(finder + 0xa0),
                    open ? "online, the game's own search runs" : bypass ? "offline, this board is ours: treated as online" : "offline, left to the game (Offline state)");
    if (!bypass)
        return;
    c.rbx = (c.rbx & ~uint64_t(0xff)) | 1;
    c.rcx = (c.rcx & ~uint64_t(0xff)) | 1;
}

bool screen_ours()
{
    return bridge_on && tb::ours_session(current_content(), session_custom.load(std::memory_order_relaxed));
}

uint8_t flag_byte(uintptr_t global)
{
    uint64_t obj = global ? peek64(global) : 0;
    if (!obj || !runtime::readable(reinterpret_cast<void *>(obj + 0x18), 1))
        return 0xff;
    return *reinterpret_cast<uint8_t *>(obj + 0x18);
}

void ts_flag_mid(safetyhook::Context &c)
{
    uint8_t al = uint8_t(c.rax), joining = flag_byte(ts_global_b), blocked = flag_byte(ts_global_a);
    bool ours = screen_ours(), force = ours && !al;
    if (force) {
        c.rax = (c.rax & ~uint64_t(0xff)) | 1;
        ts_forced_ms = now_ms();
    }
    if (!diag_on && !force)
        return;
    std::string content = current_content();
    char key[160];
    snprintf(key, sizeof key, "%u/%u/%u/%d/%d/%s", al, blocked, joining, ours, session_custom.load() ? 1 : 0, content.c_str());
    {
        std::lock_guard<std::mutex> lk(lock);
        if (key == ts_seen)
            return;
        ts_seen = key;
    }
    swofty::log("[boards] %strack select can_view_leaderboard: computed %u (flag [0x1460fddb8]+0x18 = %u, [0x14604e168]+0x18 = %u), content '%s', custom track %d, "
                "ours %d -> %s",
                diag_on ? "diag: " : "", al, blocked, joining, content.c_str(), session_custom.load() ? 1 : 0, ours,
                force ? "forced visible: the View Leaderboards button and prompt are shown" : ours ? "already visible" : "left to the game");
}

void priv_mid(safetyhook::Context &c)
{
    uint32_t type = uint32_t(c.rbp);
    if (bridge_on && type != 1 && type < 12 && content_ours()) {
        char sw[8] = {};
        bool broad = type == 2 && GetEnvironmentVariableA("SWOFTY_TT_GARAGE_TYPE2", sw, sizeof sw) && sw[0] == '1';
        uint8_t win[0x300] = {};
        size_t have = 0;
        if (type == 2) {
            for (size_t n : {sizeof win, size_t(0x100)})
                if (runtime::readable(reinterpret_cast<void *>(c.r14), n)) {
                    memcpy(win, reinterpret_cast<void *>(c.r14), n);
                    have = n;
                    break;
                }
        }
        int at = type == 2 ? tb::garage_node_offset(win, have) : -1;
        static std::atomic<int> granted{0}, missed{0}, dumped{0};
        if (type == 2 && (diag_on || broad) && dumped.load() < 4 && dumped.fetch_add(1) < 4) {
            swofty::log("[boards] diag: type 2 control 0x%llx (readable window %zu bytes, leaderboard_privilege_check id 0x%x %s in it); first 0x%zx bytes follow",
                        (unsigned long long)c.r14, have, tb::GARAGE_LEADERBOARD_NODE, at >= 0 ? "found" : "not found", have);
            for (size_t o = 0; o < have; o += 32) {
                char line[160];
                int k = snprintf(line, sizeof line, "+0x%03zx", o);
                for (size_t i = o; i < o + 32 && i < have; i += 4)
                    k += snprintf(line + k, sizeof line - k, " %08x", uint32_t(win[i]) | uint32_t(win[i + 1]) << 8 | uint32_t(win[i + 2]) << 16 | uint32_t(win[i + 3]) << 24);
                swofty::log("[boards] diag:   %s", line);
            }
        }
        if (at >= 0 || broad) {
            c.rflags |= 0x40;
            if (++granted <= 8)
                swofty::log("[boards] garage monitor: privilege check type 2 of control 0x%llx granted for our content (leaderboard_privilege_check id 0x%x %s): the panel's check is answered as "
                            "passed, so car_monitor_tt_leaderboard_privilege_check enables the tab",
                            (unsigned long long)c.r14, tb::GARAGE_LEADERBOARD_NODE,
                            at >= 0 ? ("found in the control at +0x" + std::to_string(at)).c_str() : "not found in the control; granted by SWOFTY_TT_GARAGE_TYPE2=1");
            return;
        }
        if (++missed <= 6)
            swofty::log("[boards] privilege check type %u of control 0x%llx left to the game (not the leaderboard_privilege_check node)", type, (unsigned long long)c.r14);
    }
    if (type != 1) {
        if (diag_on && type < 12) {
            static std::atomic<unsigned> seen{0};
            if (!(seen.fetch_or(1u << type) & (1u << type)))
                swofty::log("[boards] diag: privilege check of type %u reached the UI privilege control 0x%llx (check free %u): left to the game; the garage panel's own check is the one "
                            "that posts car_monitor_tt_leaderboard_privilege_check",
                            type, (unsigned long long)c.r14, unsigned(uint8_t(c.r15)));
        }
        return;
    }
    uint8_t ok = uint8_t(c.r15), enabled = runtime::readable(reinterpret_cast<void *>(c.r14 + 0xab), 1) ? *reinterpret_cast<uint8_t *>(c.r14 + 0xab) : 0xff;
    bool recent = ts_forced_ms.load() && now_ms() - ts_forced_ms.load() < TS_FORCED_WINDOW_MS;
    bool skip = bridge_on && recent && screen_ours();
    char key[48];
    snprintf(key, sizeof key, "%u/%u/%d", ok, enabled, skip);
    if (skip)
        c.rflags |= 0x40;
    {
        std::lock_guard<std::mutex> lk(lock);
        if (key == priv_seen)
            return;
        priv_seen = key;
    }
    if (diag_on || skip)
        swofty::log("[boards] %sprivilege check type 1 of control 0x%llx (check free %u, control enabled %u): %s", diag_on ? "diag: " : "", (unsigned long long)c.r14, ok,
                    enabled,
                    skip ? "the Time Trial leaderboard button of our content: success raised without the platform check" : "left to the game (not our leaderboard button)");
}
uint64_t write_ready_hook(uint64_t handler)
{
    uint64_t r = h_write_ready.call<uint64_t>(handler);
    if ((r & 1) || !bridge_on || !push_hooked)
        return r;
    uint64_t ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
    std::string board = np ? tb::read_c(game_mem(), np) : std::string();
    tb::BoardName bn = tb::parse_board(board);
    if (!bn.ok || !tb::ours(bn.descriptor, content_ours()))
        return r;
    {
        std::lock_guard<std::mutex> lk(lock);
        if (ready_seen != board) {
            ready_seen = board;
            swofty::log("[boards] leaderboard write precondition of handler 0x%llx board %s: the game says offline (no session token), this board is ours: treated as ready, the lap goes to AddLeaderboardEntry",
                        (unsigned long long)handler, board.c_str());
        }
    }
    return 1;
}

void write_token_mid(safetyhook::Context &c)
{
    if ((c.rcx & 0xff) || !bridge_on || !push_hooked)
        return;
    uint64_t handler = c.rsi, ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
    std::string board = np ? tb::read_c(game_mem(), np) : std::string();
    tb::BoardName bn = tb::parse_board(board);
    if (!bn.ok || !tb::ours(bn.descriptor, content_ours()))
        return;
    c.rcx |= 1;
    static std::atomic<int> logged{0};
    if (++logged <= 6)
        swofty::log("[boards] personal best write of handler 0x%llx board %s: the session token test fails offline, this board is ours: treated as signed in", (unsigned long long)handler,
                    board.c_str());
}

uint64_t handler_ready_hook(uint64_t handler)
{
    uint64_t r = h_ready.call<uint64_t>(handler);
    static std::atomic<int> logged{0};
    static std::atomic<uint64_t> last{~0ull};
    uint64_t key = (handler << 1) ^ (r & 0xff);
    if (last.exchange(key) != key && ++logged <= 40) {
        uint64_t ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
        swofty::log("[boards] handler 0x%llx board %s: check 0x142f91900 (stored personal best usable) returned %u", (unsigned long long)handler,
                    np ? tb::read_c(game_mem(), np).c_str() : "?", unsigned(r & 0xff));
    }
    return r;
}

void garage_msg_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    uint64_t msg = peek64(c.rdx);
    uint32_t id = msg ? peek32(msg) : 0;
    const char *name = id == 0x41bdb6f5 ? "leaderboard_fail (disables the panel)" : id == 0x48e6b721 ? "exit_container" : id == 0x75f3af2d ? "(0x75f3af2d)" : nullptr;
    if (name && ++logged <= 40)
        swofty::log("[boards] garage monitor message %s reached the panel handler (0x%x); backtrace %s", name, id, backtrace_text(1, 4).c_str());
}

std::atomic<int> rearm_count{0};

void rearm_mid(safetyhook::Context &c)
{
    int n = ++rearm_count;
    if (n <= 6 || n % 200 == 0)
        swofty::log("[boards] session timer (system at handler+0x18): re-armed %d time(s) by a validator reporting activity; object 0x%llx, new limit follows at +0xbd0 = %u s", n,
                    (unsigned long long)c.rsi, peek32(c.rsi + 0xbd0));
}

void trip_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    if (++logged <= 6) {
        LARGE_INTEGER q;
        QueryPerformanceCounter(&q);
        uint64_t offs = game.qpc_offset ? peek64(game.qpc_offset) : 0;
        double freq = double(peek64(c.rsi + 0xb98) ? peek64(c.rsi + 0xb98) : 1), start = double(peek64(c.rsi + 0xb88));
        swofty::log("[boards] session timer flag check: elapsed %.2f s since the timer was armed (start 0x%llx, scale %llu), limit %u s, re-armed %d time(s) so far; the flag is being set to TRIPPED",
                    (double(int64_t(q.QuadPart + int64_t(offs)) - int64_t(start))) / freq, (unsigned long long)peek64(c.rsi + 0xb88), (unsigned long long)peek64(c.rsi + 0xb98),
                    peek32(c.rsi + 0xbd0), rearm_count.load());
    }
}

// Ownership: rbx is the command's data block, owned by the garage view model's caller for the duration of the call. The hook
// reads one byte at +8 only after readable() says so, writes that one byte back only when garage_result says to, and calls
// nothing in the game.
void garage_done_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    uint8_t value = runtime::readable(reinterpret_cast<void *>(c.rbx + 8), 1) ? *reinterpret_cast<uint8_t *>(c.rbx + 8) : 0xff;
    uint8_t result = tb::garage_result(value, bridge_on, content_ours());
    bool grant = result != value;
    if (grant)
        *reinterpret_cast<uint8_t *>(c.rbx + 8) = result;
    if (++logged <= 8)
        swofty::log("[boards] garage monitor: tt_privilege_check_done arrived with %s%s", value == 1 ? "granted" : value == 0 ? "denied" : "an unreadable value",
                    grant ? "; changed to granted for our content so car_monitor_tt_leaderboard_privilege_check enables the leaderboard tab" : "");
}

void garage_cmd_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    static std::mutex m;
    static std::set<uint32_t> seen;
    if (!bridge_on || !content_ours())
        return;
    uint64_t name = peek64(c.rdx);
    uint32_t id = name ? peek32(name) : 0;
    if (!id)
        return;
    std::lock_guard<std::mutex> lk(m);
    if (seen.insert(id).second && ++logged <= 60)
        swofty::log("[boards] garage view model command 0x%x raised%s", id, id == 0x3c3e50f5 ? " (tt_privilege_check_done: the leaderboard privilege result)" : "");
}

// Ownership: both mid-hooks sit in the panel's tab builder on the game thread. The first runs just before `cmp byte [rcx+0x18],0` and points
// rcx at a static zero byte; the second runs just before `movabs rcx,bit; and rcx,[rax]` and points rax at a static copy of the feature
// flags with the leaderboard bit set. Nothing is written to the game and nothing is called; the copy is read once through readable().
// Ownership: runs just before `cmp byte [rcx+0x80],0` in the tab builder (rcx = the registry node [panel+0x938], ebp = session type), on the
// game thread. For our content in a time trial garage rcx is pointed at a static buffer whose +0x80 byte is 1; the real byte is only read
// through readable() for the log. Nothing is written to the game and nothing is called.
void garage_tab_gate_mid(safetyhook::Context &c)
{
    static uint8_t buf[0x100] = {};
    static std::atomic<int> logged{0};
    int session = int(uint32_t(c.rbp));
    uint8_t was = runtime::readable(reinterpret_cast<void *>(c.rcx + 0x80), 1) ? *reinterpret_cast<uint8_t *>(c.rcx + 0x80) : 0xff;
    bool ours = bridge_on && content_ours();
    bool change = tb::garage_gate_byte(was, bridge_on, content_ours(), session) != was;
    if (++logged <= 12)
        swofty::log("[boards] garage monitor tab build: gate byte [[panel+0x938]+0x80] = %u, session type 0x%x, our content %d%s", unsigned(was), unsigned(session), ours ? 1 : 0,
                    change ? ": read as 1, the leaderboard block is entered" : "");
    if (!change)
        return;
    buf[0x80] = 1;
    c.rcx = reinterpret_cast<uint64_t>(buf);
}

void garage_tab_block_mid(safetyhook::Context &c)
{
    static uint8_t zero[0x20] = {};
    static std::atomic<int> logged{0};
    if (!bridge_on || !content_ours())
        return;
    uint8_t was = runtime::readable(reinterpret_cast<void *>(c.rcx + 0x18), 1) ? *reinterpret_cast<uint8_t *>(c.rcx + 0x18) : 0xff;
    c.rcx = reinterpret_cast<uint64_t>(zero) - 0x18;
    if (++logged <= 4)
        swofty::log("[boards] garage monitor tab build: leaderboard block byte was %u, read as 0 for our content", unsigned(was));
}

void garage_tab_flags_mid(safetyhook::Context &c)
{
    static thread_local uint64_t copy = 0;
    static std::atomic<int> logged{0};
    if (!bridge_on || !content_ours() || !runtime::readable(reinterpret_cast<void *>(c.rax), 8))
        return;
    uint64_t was = *reinterpret_cast<uint64_t *>(c.rax);
    copy = tb::garage_feature_flags(was, bridge_on, true);
    c.rax = reinterpret_cast<uint64_t>(&copy);
    if (++logged <= 4)
        swofty::log("[boards] garage monitor tab build: feature flags 0x%llx (leaderboard bit %s), read with the bit set for our content: the global and friends leaderboard tabs are built",
                    (unsigned long long)was, was & tb::GARAGE_LEADERBOARD_FEATURE ? "already set" : "was clear");
}

// Ownership: runs on the game thread at the start of the builder's friends tab block (alloc, ctor with kind 1, append). For our content the
// block is skipped by moving the resume address to the code after its append (0x141f726ea, where the global-only path also continues).
// Nothing is read or written and nothing is called.
void garage_friends_tab_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    if (tb::garage_append_tab(1, bridge_on, content_ours()))
        return;
    c.rip = game.friends_resume;
    if (++logged <= 4)
        swofty::log("[boards] garage monitor tab build: the friends leaderboard tab is not built for our content (only the SWOFTY leaderboard tab)");
}

// Replay capture. Ownership: pb_mid (game thread, ghost handler) only stores a timestamp. The replay system's own tick (a mid-hook at the entry of
// vslot 24 of GameReplaySystem, rcx = the replay system) starts the game's own ReplayManager::SaveFullReplay once, after a delay, when
// tb::replay_capture_allowed holds: the vtable word is compared with the known GameReplaySystem vtable, [+0x6e1] (recording) and [+0x150] (job
// queue) are read through readable(). The path builder 0x142c48860 fills a local 0x240-byte string object (vtable, capacity, pointer, inline
// buffer) exactly as the results screen does; the save call copies the path into the replay system and queues the save job on the game's
// replay thread. A watcher thread then only polls the file system.
uintptr_t replay_build_path_fn = 0, replay_save_fn = 0;
constexpr int64_t REPLAY_SAVE_DELAY_MS = 1500;
std::atomic<uint64_t> replay_system_seen{0};

std::string narrow(const wchar_t *w)
{
    std::string out;
    for (; w && *w; w++)
        out += *w < 0x80 ? char(*w) : '?';
    return out;
}

std::atomic<int64_t> replay_ui_until_ms{0};
std::atomic<int> replay_last_status{-1};

// The game's save messages: ShowSaving (0x142c44220, this = the save listener) opens the "saving" modal, ShowResult (0x142c44760, this, status
// block: int status at +0, u64 required bytes at +8) opens the saved / disk space / generic error modal and posts the result on the bus. For
// our own save they are skipped inside the suppression window (they only touch the listener's own message handle); the status is logged.
void replay_ui_start_hook(uint64_t listener)
{
    if (tb::replay_ui_suppressed(replay_ui_until_ms.load(), now_ms())) {
        swofty::log("[replay] the game's \"saving\" message is suppressed for our own replay save");
        return;
    }
    h_rui_start.call<void>(listener);
}

void replay_ui_result_hook(uint64_t listener, uint64_t status_block)
{
    if (!tb::replay_ui_suppressed(replay_ui_until_ms.load(), now_ms())) {
        h_rui_result.call<void>(listener, status_block);
        return;
    }
    int status = runtime::readable(reinterpret_cast<void *>(status_block), 16) ? int(peek32(status_block)) : -1;
    uint64_t need = runtime::readable(reinterpret_cast<void *>(status_block), 16) ? peek64(status_block + 8) : 0;
    replay_last_status = status;
    replay_ui_until_ms = 0;
    swofty::log("[replay] the game's save job finished with status %d (0 saved, 1 no disk space, 2 failed), required bytes %llu; its result message is suppressed for our own save", status,
                (unsigned long long)need);
}

// Read-only probes inside the save job, one per synchronous check that can end it with status 2. The last probe that logs names the failing
// check: component (S+0xf08 is the session's current component), selected (0 fails), stores (+0x2b8 of the two buffers, 0 fails), alloc (0 fails).
void replay_job_component_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    if (++n <= 8)
        swofty::log("[replay] save job check 1: session component host 0x%llx, its current component [+0xf08] = 0x%llx (null fails; a wrapper type without a plain child fails)", (unsigned long long)c.rax,
                    (unsigned long long)c.r14);
}

// SWOFTY_TT_REPLAY_FORCE: "self" passes a wrapper component itself to the header writer, "child" takes its first child whatever its type.
// Test switches only. Without one the probe just logs the component, its type chain and its children.
int replay_force = 0;

// A save-slot component of our own for the replay file header, built the way the championship mode builds its one (constructor arguments
// read from the call at 0x1407d5b62) but never added to the game mode. The game's string object is {vtable, capacity, pointer, 0x40 bytes};
// the last argument is an obfuscated pair whose two words differ by xor 1. The component is built once per game mode object and kept.
uintptr_t save_slot_ctor = 0;
void describe_component(const char *what, uint64_t x);
uint64_t replay_make_save_slot(uint64_t mgr, uint64_t owner);
std::atomic<int64_t> replay_saved_mark_ms{0}, replay_saved_job_ms{0};
std::atomic<uint32_t> replay_saved_lap_ms{0};
void replay_write_setup(const std::wstring &frr, bool beside_game_file = true);
std::mutex replay_dir_lock;
std::wstring replay_dir_seen;
uint64_t replay_own_owner[8], replay_own_comp[8];
std::atomic<int> replay_own_count{0};
bool replay_slot_is_ours(uint64_t comp)
{
    for (int k = 0, n = replay_own_count.load(); k < n; k++)
        if (replay_own_comp[k] == comp)
            return comp != 0;
    return false;
}
uint64_t slot_snapshot_hook(uint64_t comp, uint64_t slot, uint64_t flag);
// What the Theatre card reads (re notes: replay-card-G): the slot's track stat id, and the metadata's session type, weather and length.
constexpr size_t SLOT_INFO_TRACK = 0xb4, SLOT_INFO_SESSION = 0xb8, META_SESSION = 0x10, META_RACE_LENGTH = 0x1c, META_WEATHER = 0x1f;
constexpr uint32_t SESSION_TYPE_TIME_TRIAL = 18;
safetyhook::InlineHook h_replay_meta;
std::atomic<bool> replay_meta_ours{false}, replay_title_ours{false};
safetyhook::MidHook h_replay_title;
extern std::atomic<uint32_t> replay_saved_lap_ms;

// A Time Trial replay is titled with its lap time; the card shows the track and "Time Trial" beside it.
void replay_title_mid(safetyhook::Context &c)
{
    if (!replay_title_ours.exchange(false))
        return;
    static char title[32];
    uint32_t ms = replay_saved_lap_ms.load();
    snprintf(title, sizeof title, "%u:%02u.%03u", ms / 60000, ms / 1000 % 60, ms % 1000);
    c.rax = reinterpret_cast<uint64_t>(title);
    swofty::log("[replay] title: %s", title);
}

// The save job fills the replay's metadata from the session; a Time Trial session leaves the session type at 0, and the card then shows no
// session line. For a save through our own component the type says Time Trial.
uint64_t replay_meta_hook(uint64_t meta, uint64_t a, uint64_t b, uint64_t c, uint64_t d)
{
    uint64_t r = h_replay_meta.call<uint64_t>(meta, a, b, c, d);
    if (replay_meta_ours.exchange(false) && runtime::readable(reinterpret_cast<void *>(meta), 0x20)) {
        *reinterpret_cast<uint32_t *>(meta + META_SESSION) = SESSION_TYPE_TIME_TRIAL;
        *reinterpret_cast<int8_t *>(meta + META_RACE_LENGTH) = 0;
        *reinterpret_cast<uint8_t *>(meta + META_WEATHER) = 1;
        swofty::log("[replay] metadata 0x%llx: session type Time Trial written for the Theatre card", (unsigned long long)meta);
    }
    return r;
}
safetyhook::InlineHook h_slot_snapshot, h_slot_default, h_slot_refresh;
std::atomic<int> slot_snapshot_skip{0};

struct GameString {
    uint64_t vtable, capacity;
    char *text;
    char buf[0x40];
};

uint64_t replay_make_save_slot(uint64_t mgr, uint64_t owner)
{
    static std::mutex lock;
    std::lock_guard<std::mutex> guard(lock);
    for (int k = 0; k < replay_own_count; k++)
        if (replay_own_owner[k] == owner)
            return replay_own_comp[k];
    if (replay_own_count >= 8) {
        swofty::log("[replay] no save-slot component: eight are already built");
        return 0;
    }
    if (!save_slot_ctor || !h_slot_snapshot || !h_slot_default || !h_slot_refresh || !runtime::readable(reinterpret_cast<void *>(mgr), 0x18)) {
        swofty::log("[replay] no save-slot component: constructor %d, snapshot hook %d", save_slot_ctor ? 1 : 0, h_slot_snapshot ? 1 : 0);
        return 0;
    }
    uint64_t alloc = peek64(mgr + 0x10);
    if (!alloc || !runtime::readable(reinterpret_cast<void *>(alloc), 8)) {
        swofty::log("[replay] no save-slot component: the replay manager has no allocator");
        return 0;
    }
    auto alloc_fn = reinterpret_cast<void *(*)(uint64_t, uint64_t, uint64_t)>(peek64(peek64(alloc) + 0x18));
    void *mem = alloc_fn(alloc, 0x300, 0x10);
    if (!mem) {
        swofty::log("[replay] no save-slot component: the allocator returned nothing");
        return 0;
    }
    memset(mem, 0, 0x300);
    uint64_t string_vtable = game.string_vt;
    if (!string_vtable) {
        swofty::log("[replay] no save-slot component: the game string vtable did not resolve on this build");
        return 0;
    }
    static GameString name, type;
    name = {string_vtable, 0x40, name.buf, {}};
    type = {string_vtable, 0x40, type.buf, {}};
    strcpy(name.buf, "time_trial");
    strcpy(type.buf, "SaveSlotGameModeComponent");
    static uint64_t pair[2];
    pair[1] = 0x5357544c41500000ull | (now_ms() & 0xffff);
    pair[0] = pair[1] ^ 1;
    auto ctor = reinterpret_cast<uint64_t (*)(void *, uint64_t, uint64_t, uint64_t, uint64_t, GameString *, GameString *, uint64_t *)>(save_slot_ctor);
    swofty::log("[replay] building a save-slot component: memory 0x%llx, allocator 0x%llx, owner 0x%llx", (unsigned long long)mem, (unsigned long long)alloc, (unsigned long long)owner);
    uint64_t comp = ctor(mem, alloc, owner, alloc, alloc, &name, &type, pair);
    swofty::log("[replay] save-slot component built: 0x%llx, vtable base+0x%llx, owner [+0x250] 0x%llx, slots [+0x268..+0x270] 0x%llx..0x%llx, next slot id [+0x2c0] %u", (unsigned long long)comp,
                (unsigned long long)(peek64(comp) - swofty::api->module_base()), (unsigned long long)peek64(comp + 0x250), (unsigned long long)peek64(comp + 0x268), (unsigned long long)peek64(comp + 0x270),
                unsigned(peek32(comp + 0x2c0)));
    replay_own_owner[replay_own_count] = owner;
    replay_own_comp[replay_own_count] = comp;
    replay_own_count = replay_own_count + 1;
    return comp;
}

// The slot snapshot serialises the career state of the component's owner; a Time Trial game mode has none, and the walk has no end check.
// For our own component the snapshot is skipped (returns 0: the slot keeps no data), every other caller goes through.
uint64_t slot_snapshot_hook(uint64_t comp, uint64_t slot, uint64_t flag)
{
    bool ours = replay_slot_is_ours(comp);
    static std::atomic<int> n{0};
    if (++n <= 12)
        swofty::log("[replay] slot snapshot for component 0x%llx slot 0x%llx flag %llu: %s", (unsigned long long)comp, (unsigned long long)slot, (unsigned long long)flag, ours ? "skipped (ours)" : "passed on");
    if (ours) {
        // The Theatre card reads its track line from the slot's track stat id; a skipped snapshot leaves it unset.
        int track = runtime::direct_start::tt::track_stat();
        if (track >= 0 && track < 0xff && runtime::readable(reinterpret_cast<void *>(slot), 0xc0)) {
            *reinterpret_cast<uint8_t *>(slot + SLOT_INFO_TRACK) = uint8_t(track);
            *reinterpret_cast<uint32_t *>(slot + SLOT_INFO_SESSION) = SESSION_TYPE_TIME_TRIAL;
        }
        swofty::log("[replay] slot 0x%llx: track stat id %d written for the Theatre card", (unsigned long long)slot, track);
        return 0;
    }
    return h_slot_snapshot.call<uint64_t>(comp, slot, flag);
}

// After the temporary replay slot is deleted the manager makes a default slot and refreshes its owner; both read parts of a component that
// only an attached one has ([+0x2f0] is null in ours). For our own component they are skipped.
uint64_t slot_default_hook(uint64_t comp)
{
    if (replay_slot_is_ours(comp)) {
        swofty::log("[replay] default slot creation skipped for our own save-slot component");
        return 0;
    }
    return h_slot_default.call<uint64_t>(comp);
}
uint64_t slot_refresh_hook(uint64_t comp)
{
    if (replay_slot_is_ours(comp)) {
        swofty::log("[replay] owner refresh skipped for our own save-slot component");
        return 0;
    }
    return h_slot_refresh.call<uint64_t>(comp);
}

// The loader finds the live session by the id in the file header, then asks its game mode for a save-slot component. A Time Trial game mode
// has none, so the list in Theatre drops the file. With the switch on, our own component stands in.
void replay_load_session_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    if (++n <= 40)
        swofty::log("[replay] load: header session id 0x%x -> session host 0x%llx%s", unsigned(peek32(c.rsp + 0xa4)), (unsigned long long)c.rax, c.rax ? "" : " (no live session has that id: the load fails)");
}

void replay_load_slot_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    uint64_t mode = runtime::readable(reinterpret_cast<void *>(c.r12 + 0xf08), 8) ? peek64(c.r12 + 0xf08) : 0;
    bool log = ++n <= 40;
    if (log)
        swofty::log("[replay] load: session host 0x%llx, game mode 0x%llx, its save-slot component 0x%llx", (unsigned long long)c.r12, (unsigned long long)mode, (unsigned long long)c.rax);
    if (c.rax || replay_force != 2 || !mode)
        return;
    if (log)
        describe_component("load game mode", mode);
    uint64_t comp = replay_make_save_slot(c.rsi, mode);
    if (comp) {
        c.rax = comp;
        if (log)
            swofty::log("[replay] load: our own save-slot component 0x%llx stands in", (unsigned long long)comp);
    }
}

// A Time Trial replay file carries no session setup, so Theatre would enter a Time Trial on whatever track and car the profile last used.
// The setup is kept beside our copies as tt\\replays\\<replay file name>.json and applied through the direct start's Time Trial setup
// before the game's own load starts.
std::wstring replay_setup_path(const std::wstring &frr)
{
    size_t cut = frr.find_last_of(L"\\/");
    return runtime::swofty_root() + L"tt\\replays\\" + (cut == std::wstring::npos ? frr : frr.substr(cut + 1)) + L".json";
}

void replay_write_setup(const std::wstring &frr, bool beside_game_file)
{
    std::string box = runtime::direct_start::tt::player_list(), track = runtime::direct_start::tt::session_track(), team = runtime::direct_start::tt::player_team();
    auto quote = [](const std::string &v) {
        std::string out = "\"";
        for (char ch : v)
            if (ch != '"' && ch != '\\' && uint8_t(ch) >= 0x20)
                out += ch;
        return out + "\"";
    };
    // The file holds the whole session. The lap ended tail_ms before the end of the recording and took lap_ms, which is the window a viewer plays.
    int64_t tail = replay_saved_job_ms.load() - replay_saved_mark_ms.load();
    if (tail < 0 || tail > 60000)
        tail = REPLAY_SAVE_DELAY_MS;
    std::string json = "{\"mode\":\"time_trial\",\"box\":" + quote(box) + ",\"track\":" + quote(track) + ",\"team\":" + quote(team) + ",\"lap_ms\":" + std::to_string(replay_saved_lap_ms.load()) +
                       ",\"tail_ms\":" + std::to_string(tail) + "}";
    bool ok = runtime::write_file(beside_game_file ? replay_setup_path(frr) : frr + L".json", json);
    swofty::log("[replay] setup for %s: %s %s", narrow(frr.c_str()).c_str(), json.c_str(), ok ? "written" : "NOT written");
}

safetyhook::InlineHook h_load_start;
// The play request: a replay file, played in the live session once the recorder has run for replay_play_delay_ms in a row. Set by the
// SWOFTY_TT_REPLAY_PLAY test switch (30 s) or by a Theatre pick of one of our Time Trial replays (6 s).
std::string replay_play_path;
std::atomic<int64_t> replay_play_at{0}, replay_play_delay_ms{30000};
std::atomic<int> replay_play_stage{0};
void replay_view_arm(const std::string &path);
std::atomic<bool> in_session_load{false};
safetyhook::MidHook h_load_ready;
std::atomic<bool> replay_play_forcing{false};
// The load start asks one of two global objects whether a load may start now (virtual +0x10). It answers no on track. The probe logs the
// object, its function and the answer; during the play test a no is turned into a yes.
void replay_load_ready_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    uint64_t fn = runtime::readable(reinterpret_cast<void *>(c.rcx), 8) && runtime::readable(reinterpret_cast<void *>(peek64(c.rcx) + 0x10), 8) ? peek64(peek64(c.rcx) + 0x10) : 0;
    bool yes = (c.rax & 0xff) != 0, force = !yes && replay_play_forcing.load();
    if (++n <= 12)
        swofty::log("[replay] load start: readiness object base+0x%llx, function base+0x%llx, answer %d%s", (unsigned long long)(c.rcx - swofty::api->module_base()),
                    (unsigned long long)(fn ? fn - swofty::api->module_base() : 0), yes ? 1 : 0, force ? " (turned into yes for the play test)" : "");
    if (force)
        c.rax |= 1;
}
std::mutex replay_load_lock;
std::wstring replay_load_path;
std::string replay_load_setup;
uint64_t replay_load_mgr = 0;
int64_t replay_load_since = 0;

bool replay_load_start_hook(uint64_t mgr, const wchar_t *path)
{
    std::wstring p = path && runtime::readable(const_cast<wchar_t *>(path), 4) ? std::wstring(path) : std::wstring();
    std::string setup = p.empty() || replay_force != 2 ? std::string() : runtime::read_file(replay_setup_path(p), size_t(1) << 16);
    swofty::log("[replay] load requested on manager 0x%llx: %s; our setup file: %s", (unsigned long long)mgr, narrow(p.c_str()).c_str(), setup.empty() ? "none (the game's own load runs now)" : setup.c_str());
    if (setup.empty())
        return h_load_start.call<bool>(mgr, path);
    std::lock_guard<std::mutex> g(replay_load_lock);
    replay_load_path = p;
    replay_load_setup = setup;
    replay_load_mgr = mgr;
    replay_load_since = now_ms();
    runtime::direct_start::tt::reset_for_replay();
    return true;
}

// Flow thread, every frame: the held load waits for the Time Trial setup, then goes to the game.
void replay_load_tick()
{
    std::unique_lock<std::mutex> g(replay_load_lock);
    if (!replay_load_mgr)
        return;
    std::string why;
    ::json::Value request;
    ::json::parse(replay_load_setup, request);
    runtime::direct_start::Setup s = runtime::direct_start::tt::setup(request, why);
    int64_t waited = now_ms() - replay_load_since;
    if (s == runtime::direct_start::Setup::pending && waited < 8000)
        return;
    uint64_t mgr = replay_load_mgr;
    std::wstring path = replay_load_path;
    replay_load_mgr = 0;
    g.unlock();
    swofty::log("[replay] Time Trial setup for the Theatre pick %s after %lld ms%s%s; track now '%s', team '%s'", s == runtime::direct_start::Setup::done ? "done" : s == runtime::direct_start::Setup::failed ? "FAILED" : "timed out",
                (long long)waited, why.empty() ? "" : ": ", why.c_str(), runtime::direct_start::tt::current_track().c_str(), runtime::direct_start::tt::player_team().c_str());
    if (s != runtime::direct_start::Setup::done) {
        swofty::log("[replay] Theatre pick: nothing is loaded (the game's own load cannot play a Time Trial file)");
        return;
    }
    // The game's load is left out: Theatre goes on to start its plain Time Trial session, and the file is loaded and played there.
    // Theatre only goes on to the session once the game's own load has run; what that load put into the replay system is dropped when the
    // session starts, so the file is loaded again there.
    // The session must hold the recording's cars: the game's stream reader walks the live vehicles and does not check the stored count.
    runtime::era_press(request["box"].str().c_str());
    bool ok = h_load_start.call<bool>(mgr, path.c_str());
    swofty::log("[replay] Theatre pick: the game's own load start returned %d", ok ? 1 : 0);
    replay_play_path.clear();
    replay_play_delay_ms = 6000;
    replay_play_at = 0;
    replay_play_stage = 0;
    replay_view_arm(narrow(path.c_str()));
}

void describe_component(const char *what, uint64_t x)
{
    if (!x || !runtime::readable(reinterpret_cast<void *>(x), 0x248)) {
        swofty::log("[replay] %s 0x%llx is not readable", what, (unsigned long long)x);
        return;
    }
    uint64_t base = swofty::api->module_base();
    uint64_t vt = peek64(x), vt18 = peek64(x + 0x18), begin = peek64(x + 0x238), end = peek64(x + 0x240);
    swofty::log("[replay] %s 0x%llx: vtable base+0x%llx, type interface vtable [+0x18] base+0x%llx, children [+0x238..+0x240] 0x%llx..0x%llx (%lld)", what, (unsigned long long)x,
                (unsigned long long)(vt - base), (unsigned long long)(vt18 - base), (unsigned long long)begin, (unsigned long long)end, (long long)(end >= begin ? (end - begin) / 8 : -1));
    std::string words;
    for (int k = 0; k < 0x60; k += 8) {
        char b[40];
        snprintf(b, sizeof b, " +%02x=%llx", k, (unsigned long long)peek64(x + k));
        words += b;
    }
    swofty::log("[replay] %s first words:%s", what, words.c_str());
}

void replay_job_wrapper_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    uint64_t x = c.r14;
    bool wrapper = (c.rcx & 0xff) != 0;
    if (++n <= 8) {
        swofty::log("[replay] save job type check: component 0x%llx is %s", (unsigned long long)x, wrapper ? "a wrapper (its first plain child is used)" : "plain (used itself)");
        describe_component("component", x);
        uint64_t begin = runtime::readable(reinterpret_cast<void *>(x), 0x248) ? peek64(x + 0x238) : 0, end = begin ? peek64(x + 0x240) : 0;
        for (uint64_t at = begin, k = 0; begin && at < end && k < 12; at += 8, k++)
            if (runtime::readable(reinterpret_cast<void *>(at), 8))
                describe_component("child", peek64(at));
    }
    if (!wrapper || !tb::replay_ui_suppressed(replay_ui_until_ms.load(), now_ms()))
        return;
    if (replay_force == 2) {
        uint64_t comp = replay_make_save_slot(c.rsi, x);
        if (comp) {
            c.r14 = comp;
            c.rcx &= ~0xffull;
            replay_meta_ours = true;
            replay_title_ours = true;
            swofty::log("[replay] SWOFTY_TT_REPLAY_FORCE=slot: our own save-slot component 0x%llx goes to the header writer", (unsigned long long)comp);
        }
    }
    if (replay_force == 1) {
        c.rcx &= ~0xffull;
        swofty::log("[replay] SWOFTY_TT_REPLAY_FORCE=self: the wrapper component itself goes to the header writer");
    }
}

void replay_job_selected_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    if (++n <= 8)
        swofty::log("[replay] save job check 2: component selected 0x%llx (null fails), main module 0x%llx", (unsigned long long)c.r14, (unsigned long long)c.rdx);
}

void replay_job_stores_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    if (++n <= 8)
        swofty::log("[replay] save job check 3: buffers 0x%llx and 0x%llx, backing stores [+0x2b8] 0x%llx and 0x%llx (null fails), sample counts [+0x2d4] %u and %u", (unsigned long long)c.r12,
                    (unsigned long long)c.r13, (unsigned long long)peek64(c.r12 + 0x2b8), (unsigned long long)peek64(c.r13 + 0x2b8), unsigned(peek32(c.r12 + 0x2d4)), unsigned(peek32(c.r13 + 0x2d4)));
}

void replay_job_alloc_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    if (++n <= 8)
        swofty::log("[replay] save job check 4: allocation result 0x%llx (null fails) for %llu bytes from allocator 0x%llx", (unsigned long long)c.rax, (unsigned long long)peek64(c.rsp + 0xb8),
                    (unsigned long long)c.r12);
}

void replay_job_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    replay_saved_job_ms = now_ms();
    if (++logged <= 8)
        swofty::log("[replay] the game's save job started on manager 0x%llx (ours %d)", (unsigned long long)c.rdx, tb::replay_ui_suppressed(replay_ui_until_ms.load(), now_ms()) ? 1 : 0);
}

// Read-only mirror of the first checks of the save job. The "main" module is looked up in the list at [[mgr+0x40]+0x110]+0x28..0x30 (stride 0x10,
// module pointer at +8, its first qword points at the name) like the job does; both buffers [+0xc0] and [+0xd0] and the session
// [[mgr+0x28]+0x88]+0x260 are plain reads. Everything is logged so a failing save can be explained from the log.
bool replay_precheck(uint64_t mgr)
{
    auto name_of = [](uint64_t module) {
        uint64_t np = module ? peek64(module) : 0;
        char buf[24] = {};
        if (np && runtime::readable(reinterpret_cast<void *>(np), sizeof buf - 1))
            memcpy(buf, reinterpret_cast<void *>(np), sizeof buf - 1);
        for (char &ch : buf)
            if (ch && (ch < 0x20 || ch > 0x7e))
                ch = '?';
        return std::string(buf);
    };
    uint64_t sys = peek64(mgr + 0x40), reg = sys ? peek64(sys + 0x110) : 0;
    uint64_t begin = reg ? peek64(reg + 0x28) : 0, end = reg ? peek64(reg + 0x30) : 0;
    uint64_t count = end > begin ? (end - begin) / 16 : 0;
    uint64_t main_module = 0;
    std::string names;
    for (uint64_t k = 0; k < count && k < 24; k++) {
        uint64_t module = peek64(begin + 16 * k + 8);
        std::string n = name_of(module);
        names += (names.empty() ? "" : ", ") + n;
        if (!_stricmp(n.c_str(), "main") && !main_module)
            main_module = module;
    }
    uint64_t full = main_module ? peek64(main_module + 0xc0) : 0, key = main_module ? peek64(main_module + 0xd0) : 0;
    uint64_t holder = peek64(mgr + 0x28), session_obj = holder ? peek64(holder + 0x88) : 0, session = session_obj ? peek64(session_obj + 0x260) : 0;
    bool session_known = holder != 0;
    bool block = tb::replay_precheck_blocks(main_module != 0, full, key, session, session_known);
    swofty::log("[replay] save pre-check: %llu module(s) in the list [%s]; main module 0x%llx full buffer [+0xc0] 0x%llx key frame buffer [+0xd0] 0x%llx; session holder 0x%llx -> object 0x%llx -> 0x%llx: %s",
                (unsigned long long)count, names.c_str(), (unsigned long long)main_module, (unsigned long long)full, (unsigned long long)key, (unsigned long long)holder,
                (unsigned long long)session_obj, (unsigned long long)session, block ? "the game's save would fail (a buffer or the session is missing): not calling it" : "no missing piece found");
    return !block;
}

std::atomic<bool> replay_saving{false};
void replay_watch_file(std::wstring path, uint32_t lap_ms, uint64_t stamp);
void replay_watch(std::wstring path, uint32_t lap_ms, uint64_t stamp)
{
    replay_saving = true;
    replay_watch_file(path, lap_ms, stamp);
    replay_saving = false;
}

void replay_watch_file(std::wstring path, uint32_t lap_ms, uint64_t stamp)
{
    uint64_t last = 0;
    int stable = 0;
    DWORD attr = 0;
    for (int i = 0; i < 180; i++) {
        Sleep(1000);
        if (replay_last_status.load() > 0) {
            swofty::log("[replay] capture: the game reported the save failed (status %d); no file to copy", replay_last_status.load());
            return;
        }
        attr = GetFileAttributesW(path.c_str());
        if (attr == INVALID_FILE_ATTRIBUTES)
            continue;
        WIN32_FILE_ATTRIBUTE_DATA d{};
        if (!GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &d))
            continue;
        uint64_t size = (uint64_t(d.nFileSizeHigh) << 32) | d.nFileSizeLow;
        if (attr & FILE_ATTRIBUTE_DIRECTORY) {
            swofty::log("[replay] capture: the game's save path %s is a directory (not a file); contents are not copied", narrow(path.c_str()).c_str());
            return;
        }
        stable = size == last ? stable + 1 : 0;
        if (!tb::replay_file_settled(size, last, stable, 3)) {
            last = size;
            continue;
        }
        std::wstring root = runtime::swofty_root();
        CreateDirectoryW((root + L"tt").c_str(), nullptr);
        std::wstring dir = root + L"tt\\replays\\";
        CreateDirectoryW(dir.c_str(), nullptr);
        std::string name = tb::replay_capture_name(stamp, lap_ms);
        std::wstring dest = dir + std::wstring(name.begin(), name.end());
        uint8_t head[16] = {};
        DWORD got = 0;
        HANDLE f = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr);
        if (f != INVALID_HANDLE_VALUE) {
            ReadFile(f, head, sizeof head, &got, nullptr);
            CloseHandle(f);
        }
        int magic = tb::replay_magic_offset(head, got);
        // The file leaves the game's replay folder: Theatre lists only the laps the player adds from the boards. The setup is written
        // last, so the pair is complete once the .json is there.
        bool copied = MoveFileExW(path.c_str(), dest.c_str(), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING) != 0;
        if (!copied && CopyFileW(path.c_str(), dest.c_str(), FALSE)) {
            copied = true;
            DeleteFileW(path.c_str());
        }
        {
            size_t cut = path.find_last_of(L"\\/");
            std::lock_guard<std::mutex> g(replay_dir_lock);
            if (cut != std::wstring::npos)
                replay_dir_seen = path.substr(0, cut + 1);
        }
        if (copied)
            replay_write_setup(dest.substr(0, dest.size() - 7), false);
        swofty::log("[replay] capture: game save %s settled at %llu bytes; header %s; magic 0xe32fcaa8 %s; copy to tt/replays/%s %s", narrow(path.c_str()).c_str(), (unsigned long long)size,
                    hex_dump(head, got).c_str(), magic >= 0 ? ("found at +" + std::to_string(magic)).c_str() : "NOT found", name.c_str(), copied ? "done" : "FAILED");
        return;
    }
    swofty::log("[replay] capture: the game's save %s did not appear within 180 s", narrow(path.c_str()).c_str());
}

// The game's replay folder: where the last save went, or Documents\My Games\F1 25\replays.
std::wstring replay_game_dir()
{
    {
        std::lock_guard<std::mutex> g(replay_dir_lock);
        if (!replay_dir_seen.empty())
            return replay_dir_seen;
    }
    using GetFolder = HRESULT(WINAPI *)(HWND, int, HANDLE, DWORD, LPWSTR);
    HMODULE shell = LoadLibraryW(L"shell32.dll");
    auto get = shell ? reinterpret_cast<GetFolder>(GetProcAddress(shell, "SHGetFolderPathW")) : nullptr;
    wchar_t docs[MAX_PATH] = {};
    if (!get || get(nullptr, 5, nullptr, 0, docs) != 0 || !*docs)
        return {};
    std::wstring dir = std::wstring(docs) + L"\\My Games\\F1 25\\replays\\";
    CreateDirectoryW(dir.c_str(), nullptr);
    return dir;
}

// The launcher's "Add to game" leaves <name>.frr and <name>.frr.json in tt\replays\inbox. The replay goes into the game's replay folder,
// where Theatre lists it, and its setup beside our own under tt\replays, where a Theatre pick reads it.
void replay_inbox()
{
    std::wstring root = runtime::swofty_root(), inbox = root + L"tt\\replays\\inbox\\";
    for (;;) {
        WIN32_FIND_DATAW d{};
        HANDLE h = FindFirstFileW((inbox + L"*.frr.json").c_str(), &d);
        if (h != INVALID_HANDLE_VALUE) {
            std::vector<std::wstring> found;
            do
                found.push_back(d.cFileName);
            while (FindNextFileW(h, &d));
            FindClose(h);
            std::wstring game = found.empty() ? std::wstring() : replay_game_dir();
            for (auto &json : found) {
                std::wstring frr = json.substr(0, json.size() - 5);
                if (game.empty() || frr.find_first_of(L"\\/:") != std::wstring::npos || GetFileAttributesW((inbox + frr).c_str()) == INVALID_FILE_ATTRIBUTES)
                    continue;
                bool a = MoveFileExW((inbox + frr).c_str(), (game + frr).c_str(), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING) != 0;
                bool b = a && MoveFileExW((inbox + json).c_str(), (root + L"tt\\replays\\" + json).c_str(), MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING) != 0;
                swofty::log("[replay] inbox: %s %s into %s; its setup %s", narrow(frr.c_str()).c_str(), a ? "moved" : "NOT moved", narrow(game.c_str()).c_str(), b ? "placed" : "NOT placed");
            }
        }
        Sleep(3000);
    }
}

std::atomic<uint64_t> replay_system_ptr{0}, replay_manager_ptr{0};

bool replay_manager_ok(uint64_t m, uint64_t system)
{
    if (!m || !runtime::readable(reinterpret_cast<void *>(m + 0x6e0), 2) || !runtime::readable(reinterpret_cast<void *>(m + 0x20), 0x28) || !runtime::readable(reinterpret_cast<void *>(m + 0x118), 8) ||
        !runtime::readable(reinterpret_cast<void *>(m + 0x150), 8))
        return false;
    tb::ReplayManagerView v;
    v.vtable20 = peek64(m + 0x20), v.system40 = peek64(m + 0x40), v.lock118 = peek64(m + 0x118), v.queue150 = peek64(m + 0x150);
    v.flag6e1 = *reinterpret_cast<uint8_t *>(m + 0x6e1);
    uintptr_t base = swofty::api->module_base();
    return tb::replay_manager_plausible(v, system, base, base + 0x20000000ull, runtime::readable(reinterpret_cast<void *>(v.vtable20), 16),
                                        runtime::readable(reinterpret_cast<void *>(v.lock118), 8), runtime::readable(reinterpret_cast<void *>(v.queue150), 0x50));
}

// The manager is found by plain reads only: every qword of the system's first 0x800 bytes, and every qword of the first 0x200 bytes behind
// those pointers, that is a readable user-space pointer is tested with replay_manager_ok (it must link back to the system at +0x40).
uint64_t replay_find_manager(uint64_t system, std::string &how)
{
    auto user = [](uint64_t p) { return p > 0x10000 && p < 0x7fffffffffffull && (p & 7) == 0; };
    for (uint64_t o = 0; o < 0x800; o += 8) {
        uint64_t p = peek64(system + o);
        if (!user(p) || !runtime::readable(reinterpret_cast<void *>(p), 8))
            continue;
        if (replay_manager_ok(p, system)) {
            how = "system+0x" + std::to_string(o);
            return p;
        }
        if (!runtime::readable(reinterpret_cast<void *>(p), 0x200))
            continue;
        for (uint64_t o2 = 0; o2 < 0x200; o2 += 8) {
            uint64_t q = peek64(p + o2);
            if (user(q) && runtime::readable(reinterpret_cast<void *>(q), 8) && replay_manager_ok(q, system)) {
                how = "system+0x" + std::to_string(o) + " -> +0x" + std::to_string(o2);
                return q;
            }
        }
    }
    return 0;
}

void replay_save_entry_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    replay_system_seen = c.rcx;
    if (runtime::readable(reinterpret_cast<void *>(c.rcx + 0x40), 8)) {
        uint64_t sys = peek64(c.rcx + 0x40);
        if (is_replay_system(sys) && replay_manager_ok(c.rcx, sys)) {
            replay_manager_ptr = c.rcx;
            replay_system_ptr = sys;
        }
    }
    if (++logged <= 6)
        swofty::log("[replay] the game started ReplayManager::SaveFullReplay on 0x%llx, path %s", (unsigned long long)c.rcx,
                    runtime::readable(reinterpret_cast<void *>(c.rdx), 2) ? narrow(reinterpret_cast<const wchar_t *>(c.rdx)).c_str() : "?");
}

int64_t replay_after_ms = 0;
std::atomic<int64_t> replay_session_best_ms{0};
bool replay_capture_any = false;

std::atomic<double> replay_lap_path{0}, replay_lap_length{0};
std::atomic<bool> replay_lap_restarted{false};
extern std::atomic<int> replay_restart_wanted;
extern std::atomic<bool> replay_saving;

// Called by the lap recorder with every sample of a timed lap: 250 m before the line, a lap that is already slower than the best gives
// up its place in the recording, so the next lap's file starts a few seconds before its line.
void replay_lap_progress(double path_m, double elapsed_s)
{
    replay_lap_path = path_m;
    double length = replay_lap_length.load();
    int64_t best = replay_session_best_ms.load();
    if (!replay_capture_on || !bridge_on || length <= 0 || best <= 0 || path_m < length - 250 || path_m > length || replay_lap_restarted.load())
        return;
    if (int64_t(elapsed_s * 1000) + 2500 < best)
        return;
    replay_lap_restarted = true;
    replay_restart_wanted = 1;
}

// Called from the lap recorder (flow thread) after a lap was written: a valid lap faster than the session's best marks a capture pending.
void replay_lap_finished(int64_t lap_ms, int valid)
{
    if (!replay_capture_on || !bridge_on)
        return;
    int64_t best = replay_session_best_ms.load();
    // SWOFTY_TT_REPLAY_CAPTURE=any is a test switch: every finished lap is saved, valid or not.
    bool mark = tb::replay_lap_is_best(lap_ms, valid, best) || (replay_capture_any && lap_ms > 0);
    swofty::log("[replay] capture: lap %lld ms (valid %d), session best %lld ms: %s", (long long)lap_ms, valid, (long long)best, mark ? "marked for a replay save" : "not a new best, no save");
    double path = replay_lap_path.exchange(0);
    if (path > 1000)
        replay_lap_length = path;
    bool early = replay_lap_restarted.exchange(false);
    if (!mark) {
        if (!early)
            replay_restart_wanted = 2;
        return;
    }
    replay_session_best_ms = lap_ms;
    replay_pending_lap = uint32_t(lap_ms);
    replay_pending_ms = now_ms();
    replay_saved_lap_ms = uint32_t(lap_ms);
    replay_saved_mark_ms = replay_pending_ms.load();
}

// `sys` is the candidate replay system. The per-frame ReplayBuffer::beginFrameWrite hook has no replay system in a register (rcx is the
// buffer), so it passes 0 and the last GameReplaySystem a hook verified is used; the vtable word is checked again on every use.
// A Time Trial replay picked in Theatre is shown in the game's own replay viewer (re notes: replay-viewer-H). Theatre starts a plain Time
// Trial session; its first state, SessionEnter, is held while the file is loaded into the replay system behind the load screen, and its
// link is then pointed at the fastest lap viewer state, so the intro and the garage never run. The viewer's enter gets the lap's window,
// and its "return" link is sent to the session's Exit state, which ends in the main menu. Any step that fails falls back to playing the
// file once the car is on track.
namespace view {
constexpr uint32_t ROOT_TIME_TRIAL = 0xca472e10, NODE_SESSION_ENTER = 0x46fa8767, NODE_POST_RACE = 0xc56923a6, LINK_RETURN = 0x19306425,
                   LINK_REPLAY_FASTEST = 0x46af106b, LINK_LEAVE = 0xd63988d1, NODE_INTRO_VIDEO = 0xc8a977c8, NODE_INTRO_CAMERAS = 0xd04db8ad,
                   NODE_GARAGE = 0x4852eb9a, NODE_IN_GAME = 0xba16bc16, NODE_POST_CUTSCENE = 0xc5155560, LINK_NEXT = 0x7c9b1ec4,
                   LINK_CAMERAS_DONE = 0x0059737c, LINK_FINISHED = 0x38d5604f, LINK_ONE = 0x0002b5d6;
constexpr size_t N_HASH = 0xb0, N_STATUS = 0xb8, N_CHILDREN = 0xc0, N_RECORDS = 0xf0, N_RECORD = 0xa0, N_RECORD_TARGET = 0x98, N_LINKS = 0xaf0, N_LINK_COUNT = 0xb30,
                 N_PARENT = 0xc10;
// The fastest lap viewer state is known by its enter function, slot 8 of its vtable and of no other.
constexpr size_t VT_SLOT_ENTER = 0x40;
constexpr size_t VIEW_SYSTEM = 0xd60, VIEW_START = 0xf60, VIEW_END = 0xf64, VIEW_ABORTED = 0xed4;
enum Stage { off, armed, loading, loaded, routing, viewing, leaving, broken };
std::atomic<int> stage{off};
std::atomic<int64_t> since{0}, load_at{0};
std::atomic<bool> load_started{false};
std::atomic<float> duration{0};
std::string path;
uint32_t held_link = 0;
uintptr_t flying_lap_fn = 0;
int in_game_updates = 0;
int64_t flying_at = 0, recording_at = 0;
uint32_t pit_seen = 0xffffffff;
constexpr size_t IN_GAME_PIT_STATUS = 0xee4;
constexpr uint32_t PIT_IN_GARAGE = 9;
int64_t garage_at = 0;
uint8_t *dumped_root = nullptr;
std::vector<uint8_t *> route;
struct Patch {
    uint32_t *at;
    uint32_t old;
};
std::vector<Patch> patches;
safetyhook::MidHook h_enter;
int forced_logs = 0;

uint32_t u32(const uint8_t *p, size_t off)
{
    return *reinterpret_cast<const uint32_t *>(p + off);
}

bool node_ok(const uint8_t *n)
{
    return n && runtime::readable(n, N_PARENT + 8);
}

std::vector<uint8_t *> children(uint8_t *n)
{
    std::vector<uint8_t *> out;
    auto *cv = *reinterpret_cast<uint8_t **>(n + N_CHILDREN);
    if (!cv || !runtime::readable(cv, 0x18))
        return out;
    for (auto **p = *reinterpret_cast<uint8_t ***>(cv + 8), **e = *reinterpret_cast<uint8_t ***>(cv + 0x10); p && p < e && out.size() < 64; ++p)
        if (runtime::readable(p, 8) && node_ok(*p))
            out.push_back(*p);
    return out;
}

uint32_t link_count(const uint8_t *n)
{
    return std::min<uint32_t>(u32(n, N_LINK_COUNT), 16);
}

uint32_t *record_target(uint8_t *n, uint32_t i)
{
    return reinterpret_cast<uint32_t *>(n + N_RECORDS + size_t(i) * N_RECORD + N_RECORD_TARGET);
}

// The link of n that leads to the sibling `target`: an existing record, or record `prefer` (else the first) rewritten to it.
uint32_t link_to(uint8_t *n, uint32_t target, uint32_t prefer)
{
    uint32_t count = link_count(n);
    auto *ids = reinterpret_cast<uint32_t *>(n + N_LINKS);
    for (uint32_t i = 0; i < count; i++)
        if (*record_target(n, i) == target)
            return ids[i];
    if (!count)
        return 0;
    uint32_t pick = 0;
    for (uint32_t i = 0; i < count; i++)
        if (ids[i] == prefer)
            pick = i;
    uint32_t *at = record_target(n, pick);
    patches.push_back({at, *at});
    swofty::log("[replay] viewer: state %08x link %08x target %08x rewritten to %08x", u32(n, N_HASH), ids[pick], *at, target);
    *at = target;
    return ids[pick];
}

void restore()
{
    for (auto it = patches.rbegin(); it != patches.rend(); ++it)
        if (runtime::readable(it->at, 4))
            *it->at = it->old;
    patches.clear();
}

void dump(uint8_t *root)
{
    uintptr_t base = swofty::api->module_base();
    std::vector<std::pair<uint8_t *, int>> queue{{root, 0}};
    for (size_t i = 0; i < queue.size() && i < 256; i++) {
        auto [n, depth] = queue[i];
        std::string links, kids;
        auto *ids = reinterpret_cast<uint32_t *>(n + N_LINKS);
        for (uint32_t k = 0; k < link_count(n); k++) {
            char b[24];
            snprintf(b, sizeof b, " %08x>%08x", ids[k], *record_target(n, k));
            links += b;
        }
        for (uint8_t *c : children(n)) {
            char b[12];
            snprintf(b, sizeof b, " %08x", u32(c, N_HASH));
            kids += b;
            if (depth < 8)
                queue.push_back({c, depth + 1});
        }
        auto *parent = *reinterpret_cast<uint8_t **>(n + N_PARENT);
        swofty::log("[replay] tt node %08x parent %08x depth %d vt base+0x%llx status %u links%s children%s", u32(n, N_HASH), node_ok(parent) ? u32(parent, N_HASH) : 0, depth,
                    (unsigned long long)(peek64(reinterpret_cast<uint64_t>(n)) - base), unsigned(u32(n, N_STATUS) & 0xff), links.empty() ? " none" : links.c_str(), kids.empty() ? " none" : kids.c_str());
    }
}

// The path of states from the session's root down to the fastest lap viewer, or empty.
std::vector<uint8_t *> find_route(uint8_t *root)
{
    uintptr_t want = game.view_enter_fn;
    auto is_viewer = [&](uint8_t *n) {
        uint64_t vt = peek64(reinterpret_cast<uint64_t>(n));
        return want && vt && runtime::readable(reinterpret_cast<void *>(vt + VT_SLOT_ENTER), 8) && peek64(vt + VT_SLOT_ENTER) == want;
    };
    std::vector<std::pair<uint8_t *, int>> queue{{root, -1}};
    for (size_t i = 0; i < queue.size() && i < 256; i++) {
        uint8_t *n = queue[i].first;
        if (is_viewer(n)) {
            std::vector<uint8_t *> out;
            for (int k = int(i); k >= 0; k = queue[size_t(k)].second)
                out.insert(out.begin(), queue[size_t(k)].first);
            return out;
        }
        for (uint8_t *c : children(n))
            queue.push_back({c, int(i)});
    }
    return {};
}

void fail(const char *why)
{
    int was = stage.exchange(off);
    restore();
    route.clear();
    bool loaded_file = load_started.load();
    swofty::log("[replay] viewer: %s (stage %d); the file plays once the car is on track instead", why, was);
    replay_play_path = path;
    replay_play_stage = loaded_file ? 1 : 0;
    replay_play_at = loaded_file ? now_ms() + 3000 : 0;
}

std::atomic<int64_t> trace_until{0};

// For a while after the viewer's exit every state's status and link is logged when it changes, in any flow machine.
void trace(uint8_t *node, uint32_t link)
{
    static std::mutex lock;
    static std::map<uint8_t *, uint64_t> seen;
    static int lines = 0;
    uint64_t now = (uint64_t(u32(node, N_STATUS) & 0xff) << 32) | link;
    std::lock_guard<std::mutex> g(lock);
    auto it = seen.find(node);
    if (it != seen.end() && it->second == now)
        return;
    seen[node] = now;
    if (lines++ > 400)
        return;
    auto *parent = *reinterpret_cast<uint8_t **>(node + N_PARENT);
    swofty::log("[replay] exit trace: state %08x (parent %08x) status %u returns link %08x", u32(node, N_HASH), node_ok(parent) ? u32(parent, N_HASH) : 0, unsigned(u32(node, N_STATUS) & 0xff), link);
}

void on_link(uint8_t *node, uint32_t *link)
{
    int st = stage.load();
    if (st == off && trace_until.load() > now_ms())
        trace(node, *link);
    if (st == off)
        return;
    uint8_t *root = node;
    for (int k = 0; k < 16; k++) {
        auto *parent = *reinterpret_cast<uint8_t **>(root + N_PARENT);
        if (!node_ok(parent))
            break;
        root = parent;
    }
    int64_t now = now_ms();
    if (u32(root, N_HASH) != ROOT_TIME_TRIAL) {
        if (st == armed && now - since.load() > 180000) {
            stage = off;
            swofty::log("[replay] viewer: no Time Trial session started within three minutes; disarmed");
        }
        return;
    }
    if (root != dumped_root) {
        dumped_root = root;
        dump(root);
    }
    uint32_t hash = u32(node, N_HASH);
    if (st == broken) {
        fail("the load failed");
        return;
    }
    // A replay only reads back into a session whose cars are on track, so the session is hurried through its intro and garage to the
    // driving state, the file is loaded there, and the session is then ended into its results, which lead to the viewer.
    if (st == armed) {
        if (hash == NODE_INTRO_VIDEO)
            *link = LINK_NEXT;
        else if (hash == NODE_INTRO_CAMERAS)
            *link = LINK_CAMERAS_DONE;
        else if (hash == NODE_GARAGE) {
            if (!garage_at)
                garage_at = now;
            if (now - garage_at > 500)
                *link = LINK_NEXT;
        } else if (hash == NODE_IN_GAME) {
            garage_at = 0;
            in_game_updates = 0;
            flying_at = 0;
            pit_seen = 0xffffffff;
            load_started = false;
            since = now;
            stage = loading;
            swofty::log("[replay] viewer: the session is in its driving state; %s loads once the recorder runs", path.c_str());
        }
        return;
    }
    if (st == loading && hash == NODE_IN_GAME && !load_started.load()) {
        // The driving state starts in the garage; the game's own flying lap request puts the car on track, where the recorder starts.
        uint32_t pit = u32(node, IN_GAME_PIT_STATUS);
        if (pit != pit_seen) {
            swofty::log("[replay] viewer: pit status %u > %u", pit_seen, pit);
            pit_seen = pit;
        }
        if (++in_game_updates > 2 && pit == PIT_IN_GARAGE && flying_lap_fn && (!flying_at || now - flying_at > 3000)) {
            flying_at = now;
            swofty::log("[replay] viewer: flying lap requested");
            reinterpret_cast<void (*)()>(flying_lap_fn)();
        }
    }
    if (st == loading && hash == NODE_IN_GAME && !load_started.load()) {
        // The load job resets the replay system while it runs, and the simulation's readers crash on that if a frame runs meanwhile.
        // The load is issued here, on the flow thread, which then waits for the job, so no frame advances during it.
        uint64_t sys = replay_system_ptr.load(), mgr = replay_manager_ptr.load();
        std::string how;
        if (sys && !mgr)
            mgr = replay_find_manager(sys, how);
        bool recording = sys && runtime::readable(reinterpret_cast<void *>(sys), 0x100) && peek32(sys + 0x20) == 0;
        if (!recording)
            recording_at = 0;
        else if (!recording_at)
            recording_at = now;
        if (recording && now - recording_at >= 3000 && mgr && runtime::readable(reinterpret_cast<void *>(mgr), 0x700)) {
            auto busy = [&] { return queue_busy(peek64(mgr + 0x150)); };
            std::wstring w(path.begin(), path.end());
            load_started = true;
            swofty::log("[replay] viewer: load on manager 0x%llx from the flow thread: recording flag %u, [+0x428] 0x%llx, job queue busy %d", (unsigned long long)mgr, unsigned(peek32(mgr + 0x6e1) & 0xff),
                        (unsigned long long)peek64(mgr + 0x428), busy());
            replay_set_state(sys, 1, 1);
            replay_play_forcing = true;
            in_session_load = true;
            *reinterpret_cast<int64_t *>(mgr + 0x428) = -1;
            bool ok = h_load_start.call<bool>(mgr, w.c_str());
            replay_play_forcing = false;
            float length = 0;
            int64_t began = now_ms();
            while (ok && now_ms() - began < 8000) {
                Sleep(5);
                uint64_t list = peek64(sys + 0x98), module = list && runtime::readable(reinterpret_cast<void *>(list), 8) ? peek64(list) : 0;
                if (module && runtime::readable(reinterpret_cast<void *>(module), 0x110))
                    memcpy(&length, reinterpret_cast<void *>(module + 0xe8), 4);
                if (now_ms() - began > 300 && busy() == 0 && length > 0.5f)
                    break;
            }
            in_session_load = false;
            swofty::log("[replay] viewer: the game's load start returned %d; the flow thread waited %lld ms, %.3f s of replay", ok ? 1 : 0, (long long)(now_ms() - began), double(length));
            if (!ok || length < 0.5f) {
                fail("the load failed");
                return;
            }
            duration = length;
            load_at = now_ms();
            stage = loaded;
            return;
        }
    }
    if (st == loading) {
        if (hash == NODE_IN_GAME && load_started.load() && now - load_at.load() > 30000)
            fail("the load did not finish within 30 s");
        return;
    }
    if (st == loaded) {
        if (hash != NODE_IN_GAME)
            return;
        route = find_route(root);
        if (route.size() < 2) {
            fail("the Time Trial session has no fastest lap viewer state");
            return;
        }
        *link = LINK_FINISHED;
        forced_logs = 0;
        since = now;
        stage = routing;
        swofty::log("[replay] viewer: the driving state leaves by link %08x (finished)", *link);
        return;
    }
    if (st == routing) {
        if (now - since.load() > 20000) {
            fail("the viewer state was not entered within 20 s");
            return;
        }
        // The post-session group runs on its own from here down to the results state, whose "replay fastest lap" link leads to the viewer.
        if (hash == NODE_POST_CUTSCENE)
            *link = LINK_ONE;
        if (hash == NODE_POST_RACE) {
            if (forced_logs++ < 4)
                swofty::log("[replay] viewer: the results state returns link %08x; replay_fastest is sent instead", *link);
            *link = LINK_REPLAY_FASTEST;
        }
        return;
    }
    uint8_t *viewer = route.empty() ? nullptr : route.back();
    if (st == viewing) {
        if (node != viewer || *link != LINK_RETURN)
            return;
        stage = leaving;
        st = leaving;
        swofty::log("[replay] viewer: the viewer returned; leaving the session");
    }
    if (st == leaving && node == viewer) {
        // The viewer's own leave link ends the session (through the post-session group's last state to Exit) instead of going back to the results.
        *link = LINK_LEAVE;
        swofty::log("[replay] viewer exit: state %08x leaves the session by link %08x", hash, *link);
        // The session state only unloads once the game mode says where to go next. A Time Trial says so once, at session enter, and the
        // replay load's change of the current mode clears it again; the mode's own setter (vtable +0x1f0) says "front end" once more.
        uint64_t holder = peek64(reinterpret_cast<uint64_t>(node) + 0xca0), session = holder && runtime::readable(reinterpret_cast<void *>(holder), 0x90) ? peek64(holder + 0x88) : 0;
        uint64_t mode = session && runtime::readable(reinterpret_cast<void *>(session), 0x268) ? peek64(session + 0x260) : 0;
        uint64_t vt = mode && runtime::readable(reinterpret_cast<void *>(mode), 8) ? peek64(mode) : 0;
        bool known = game.mode_vt && vt == game.mode_vt;
        uint64_t gm = known && runtime::readable(reinterpret_cast<void *>(mode + 0xf40), 8) ? peek64(mode + 0xf40) : 0;
        uint32_t before = gm && runtime::readable(reinterpret_cast<void *>(gm + 0x1fc), 4) ? peek32(gm + 0x1fc) : 0xffffffff;
        if (known)
            reinterpret_cast<void (*)(uint64_t)>(peek64(vt + 0x1f0))(mode);
        swofty::log("[replay] viewer exit: game mode 0x%llx (vtable base+0x%llx, %s): where-to-next %u > %u", (unsigned long long)mode, (unsigned long long)(vt ? vt - swofty::api->module_base() : 0),
                    known ? "Time Trial" : "not the Time Trial mode, left alone", before, gm ? peek32(gm + 0x1fc) : 0xffffffffu);
        trace_until = now + 60000;
        stage = off;
        route.clear();
    }
}

void enter_mid(safetyhook::Context &c)
{
    if (stage.load() != routing)
        return;
    auto *state = reinterpret_cast<uint8_t *>(c.rsi);
    uint64_t host = c.rax, sys = peek64(c.rsi + VIEW_SYSTEM);
    float length = duration.load(), start = 0, end = length;
    ::json::Value setup;
    std::wstring wide(path.begin(), path.end());
    static const bool whole = [] { const char *e = getenv("SWOFTY_TT_REPLAY_WHOLE"); return e && *e == '1'; }();
    if (!whole && ::json::parse(runtime::read_file(replay_setup_path(wide), size_t(1) << 16), setup) && setup["lap_ms"].n > 0 && length > 0.5f) {
        float lap_end = length - float(setup["tail_ms"].n) / 1000.0f, lap = float(setup["lap_ms"].n) / 1000.0f;
        end = std::min(length, lap_end + 2.0f);
        start = std::max(0.0f, lap_end - lap - 2.0f);
    }
    // The viewer's update widens the window by five seconds either side.
    float from = start + 5.0f, to = end - 5.0f;
    memcpy(state + VIEW_START, &from, 4);
    memcpy(state + VIEW_END, &to, 4);
    swofty::log("[replay] viewer: enter of state %p: host flag [+0xe2e] %u, system 0x%llx byte [+0x88] %u; window %.3f..%.3f of %.3f s", state,
                runtime::readable(reinterpret_cast<void *>(host + 0xe2e), 1) ? unsigned(peek32(host + 0xe2e) & 0xff) : 999u, (unsigned long long)sys,
                runtime::readable(reinterpret_cast<void *>(sys + 0x88), 1) ? unsigned(peek32(sys + 0x88) & 0xff) : 999u, double(start), double(end), double(length));
    c.rip = game.view_resume;
    restore();
    stage = viewing;
}

// On the replay system's tick: the load the held SessionEnter waits for.
void tick(uint64_t sys)
{
    // The load moved to the flow thread (on_link); this stays for the SWOFTY_TT_REPLAY_TICKLOAD=1 comparison.
    static const bool on = [] { const char *e = getenv("SWOFTY_TT_REPLAY_TICKLOAD"); return e && *e == '1'; }();
    if (!on || stage.load() != loading)
        return;
    uint64_t mgr = replay_manager_ptr.load();
    std::string how;
    if (!mgr)
        mgr = replay_find_manager(sys, how);
    if (!mgr || !runtime::readable(reinterpret_cast<void *>(mgr), 0x700)) {
        stage = broken;
        return;
    }
    auto busy = [&] { return queue_busy(peek64(mgr + 0x150)); };
    static std::atomic<int64_t> recording_since{0};
    if (!load_started.load()) {
        if (peek32(sys + 0x20) != 0) {
            recording_since = 0;
            return;
        }
        if (!recording_since.load())
            recording_since = now_ms();
        if (now_ms() - recording_since.load() < 3000)
            return;
        recording_since = 0;
    }
    if (!load_started.exchange(true)) {
        std::wstring w(path.begin(), path.end());
        swofty::log("[replay] viewer: load on manager 0x%llx: recording flag %u, system state %u, [+0x428] 0x%llx, job queue busy %d", (unsigned long long)mgr, unsigned(peek32(mgr + 0x6e1) & 0xff),
                    unsigned(peek32(sys + 0x20)), (unsigned long long)peek64(mgr + 0x428), busy());
        replay_set_state(sys, 1, 1);
        bool ok = false;
        replay_play_forcing = true;
        in_session_load = true;
        // The load start refuses while the manager still names the replay Theatre loaded ([+0x428] is not -1).
        *reinterpret_cast<int64_t *>(mgr + 0x428) = -1;
        for (int k = 0; k < 4 && !ok; k++) {
            Sleep(50);
            ok = h_load_start.call<bool>(mgr, w.c_str());
        }
        replay_play_forcing = false;
        swofty::log("[replay] viewer: the game's load start returned %d", ok ? 1 : 0);
        if (!ok) {
            in_session_load = false;
            load_started = false;
            stage = broken;
            return;
        }
        load_at = now_ms();
        return;
    }
    uint64_t list = peek64(sys + 0x98), module = list && runtime::readable(reinterpret_cast<void *>(list), 8) ? peek64(list) : 0;
    float length = 0;
    if (module && runtime::readable(reinterpret_cast<void *>(module), 0x110))
        memcpy(&length, reinterpret_cast<void *>(module + 0xe8), 4);
    if (now_ms() - load_at.load() < 2000 || busy() != 0 || length < 0.5f)
        return;
    in_session_load = false;
    duration = length;
    int want = loading;
    if (stage.compare_exchange_strong(want, loaded))
        swofty::log("[replay] viewer: loaded after %lld ms, %.3f s of replay", (long long)(now_ms() - load_at.load()), double(length));
}

}

// While a replay loads into a session that is still entering, the game mode has no current session for a moment, and this view model's
// setup would read through the null. The setup is left for a later frame (its caller retries until it has run).
safetyhook::InlineHook h_vm_session_setup, h_info_load;

// The info block's save data goes into the game mode's containers, which the game only does in the front end; in a live session the mode
// loses its current session for a moment and readers on other threads crash. Theatre's own load in the front end already read the block,
// so the load into the live session leaves it out.
int info_load_hook(uint64_t mgr, uint64_t block, uint64_t size, uint64_t container)
{
    if (in_session_load.load()) {
        swofty::log("[replay] viewer: the info block's save data is left out of the load into the live session");
        return 0;
    }
    return h_info_load.call<int>(mgr, block, size, container);
}

void vm_session_setup_hook(uint8_t *vm)
{
    if (view::stage.load() != view::off && runtime::readable(vm, 0xb8)) {
        uint64_t a = peek64(reinterpret_cast<uint64_t>(vm) + 0xb0), b = a && runtime::readable(reinterpret_cast<void *>(a), 0x38) ? peek64(a + 0x30) : 0;
        if (b && runtime::readable(reinterpret_cast<void *>(b), 0x90) && !peek64(b + 0x88)) {
            static std::atomic<int> n{0};
            if (++n <= 4)
                swofty::log("[replay] viewer: a session view model's setup waits; the game mode has no current session yet");
            return;
        }
    }
    h_vm_session_setup.call<void>(vm);
}

void replay_view_arm(const std::string &path)
{
    view::path = path;
    if (!game.view_ready() || !h_load_start || !view::h_enter || !runtime::flow_link_listen(view::on_link)) {
        swofty::log("[replay] Theatre pick: the viewer route is not available; %s plays once the session is on track", path.c_str());
        replay_play_path = path;
        return;
    }
    view::restore();
    view::route.clear();
    view::load_started = false;
    view::since = now_ms();
    view::stage = view::armed;
    swofty::log("[replay] Theatre pick: %s opens in the replay viewer", path.c_str());
}

void replay_tick(uint64_t sys, const char *slot)
{
    static std::atomic<uint64_t> entries[4]{}, ours_entries[4]{};
    static std::atomic<int> guard_logs{0};
    int which = !strcmp(slot, "vslot24") ? 0 : !strcmp(slot, "vslot17") ? 1 : !strcmp(slot, "updateFromModule") ? 2 : 3;
    uint64_t n = ++entries[which];
    uint64_t known = replay_system_ptr.load();
    if (!sys)
        sys = known;
    bool vt_ok = is_replay_system(sys);
    if (vt_ok) {
        ours_entries[which]++;
        replay_system_ptr = sys;
    }
    if (n <= 5 || n % 20000 == 0)
        swofty::log("[replay] tick %s entry %llu (replay system 0x%llx, GameReplaySystem vtable %s, %llu such entries); pending mark %lld", slot, (unsigned long long)n, (unsigned long long)sys,
                    vt_ok ? "yes" : "no", (unsigned long long)ours_entries[which].load(), (long long)replay_pending_ms.load());
    // SWOFTY_TT_REPLAY_PLAY=<replay file path as the game sees it> is a test switch: 40 s into a session with the bridge on the file is loaded into
    // the live session through the game's own load, and 8 s later the replay system is put into full replay playback of the whole buffer
    // (mode 0, state 2, clip over the module's duration), the way the fastest lap view's update does it. Every step is logged.
    if (vt_ok && bridge_on && h_load_start)
        view::tick(sys);
    if (vt_ok && !replay_play_path.empty() && bridge_on && h_load_start) {
        std::atomic<int64_t> &at = replay_play_at;
        std::atomic<int> &stage = replay_play_stage;
        int64_t now = now_ms();
        auto state_line = [&](const char *when) {
            uint64_t list = peek64(sys + 0x98), module = list && runtime::readable(reinterpret_cast<void *>(list), 8) ? peek64(list) : 0;
            float duration = 0;
            if (module && runtime::readable(reinterpret_cast<void *>(module), 0x110))
                memcpy(&duration, reinterpret_cast<void *>(module + 0xe8), 4);
            swofty::log("[replay] play test %s: system 0x%llx state [+0x20] %u, [+0x88] %u, flags +0x42..45 %02x %02x %02x %02x, first module 0x%llx duration [+0xe8] %.3f clip [+0xfd] %u", when, (unsigned long long)sys,
                        unsigned(peek32(sys + 0x20)), unsigned(peek32(sys + 0x88)), unsigned(peek32(sys + 0x42) & 0xff), unsigned(peek32(sys + 0x43) & 0xff), unsigned(peek32(sys + 0x44) & 0xff),
                        unsigned(peek32(sys + 0x45) & 0xff), (unsigned long long)module, double(duration), module ? unsigned(peek32(module + 0xfd) & 0xff) : 0u);
            return std::pair<uint64_t, float>(module, duration);
        };
        if (stage.load() == 0 && peek32(sys + 0x20) != 0)
            at = 0;
        else if (!at.load())
            at = now + replay_play_delay_ms.load();
        else if (stage.load() == 0 && now >= at.load() && stage.exchange(1) == 0) {
            std::string how;
            uint64_t mgr = replay_manager_ptr.load();
            if (!mgr)
                mgr = replay_find_manager(sys, how);
            state_line("before the load");
            bool flag = mgr && runtime::readable(reinterpret_cast<void *>(mgr), 0x700) && (peek32(mgr + 0x6e1) & 0xff);
            swofty::log("[replay] play test: manager 0x%llx, recording flag %d, file %s", (unsigned long long)mgr, flag ? 1 : 0, replay_play_path.c_str());
            if (flag) {
                std::wstring w(replay_play_path.begin(), replay_play_path.end());
                uint64_t q38 = peek64(mgr + 0x38), active = q38 && runtime::readable(reinterpret_cast<void *>(q38 + 400), 8) ? peek64(q38 + 400) : 0;
                uint64_t q468 = peek64(mgr + 0x468), q778 = q468 && runtime::readable(reinterpret_cast<void *>(q468 + 0x778), 8) ? peek64(q468 + 0x778) : 0;
                swofty::log("[replay] play test: load start guards: job queue [+0x150] 0x%llx, [[+0x38]+400] 0x%llx (must be 0), [+0x428] 0x%llx (must be -1), [[[+0x468]+0x778]+0xa0] %u, attributes 0x%lx",
                            (unsigned long long)peek64(mgr + 0x150), (unsigned long long)active, (unsigned long long)peek64(mgr + 0x428),
                            q778 && runtime::readable(reinterpret_cast<void *>(q778 + 0xa0), 1) ? unsigned(peek32(q778 + 0xa0) & 0xff) : 999u, (unsigned long)GetFileAttributesW(w.c_str()));
                auto busy = [&] { return queue_busy(peek64(mgr + 0x150)); };
                swofty::log("[replay] play test: job queue busy %d while recording; recording is stopped (state 1) before the load", busy());
                replay_set_state(sys, 1, 1);
                bool ok = false;
                replay_play_forcing = true;
                *reinterpret_cast<int64_t *>(mgr + 0x428) = -1;
                for (int k = 0; k < 4 && !ok; k++) {
                    Sleep(50);
                    int b = busy();
                    ok = h_load_start.call<bool>(mgr, w.c_str());
                    if (true)
                        swofty::log("[replay] play test: try %d: job queue busy %d, system state %u, the game's load start returned %d", k, b, unsigned(peek32(sys + 0x20)), ok ? 1 : 0);
                }
                replay_play_forcing = false;
                if (!ok && active) {
                    *reinterpret_cast<uint64_t *>(q38 + 400) = 0;
                    ok = h_load_start.call<bool>(mgr, w.c_str());
                    *reinterpret_cast<uint64_t *>(q38 + 400) = active;
                    swofty::log("[replay] play test: with [[+0x38]+400] cleared for the call the load start returned %d", ok ? 1 : 0);
                }
            }
            at = now + 8000;
        } else if (stage.load() == 1 && now >= at.load() && stage.exchange(2) == 1) {
            auto [module, duration] = state_line("after the load");
            if (game.replay_seek && game.replay_set_state && module) {
                reinterpret_cast<void (*)(uint64_t, uint32_t)>(game.replay_seek)(sys, 0);
                replay_set_state(sys, 2, 1);
                float zero = 0;
                memcpy(reinterpret_cast<void *>(sys + 0x7c), &zero, 4);
                *reinterpret_cast<uint8_t *>(sys + 0x7a) = 1;
                *reinterpret_cast<uint8_t *>(sys + 0x42) = 0;
                *reinterpret_cast<uint8_t *>(sys + 0x43) = 1;
                *reinterpret_cast<uint8_t *>(sys + 0x44) = 0;
                *reinterpret_cast<uint8_t *>(sys + 0x45) = 0;
                *reinterpret_cast<uint8_t *>(module + 0xfd) = 1;
                memcpy(reinterpret_cast<void *>(module + 0x100), &zero, 4);
                memcpy(&duration, reinterpret_cast<void *>(module + 0xe8), 4);
                float end = duration > 0.5f ? duration : 100000.0f, start = 0;
                ::json::Value setup;
                std::wstring wide_path(replay_play_path.begin(), replay_play_path.end());
                if (::json::parse(runtime::read_file(replay_setup_path(wide_path), size_t(1) << 16), setup) && setup["lap_ms"].n > 0 && duration > 0.5f) {
                    float lap_end = duration - float(setup["tail_ms"].n) / 1000.0f, lap = float(setup["lap_ms"].n) / 1000.0f;
                    end = std::min(duration, lap_end + 2.0f);
                    start = std::max(0.0f, lap_end - lap - 2.0f);
                }
                memcpy(reinterpret_cast<void *>(sys + 0x7c), &start, 4);
                memcpy(reinterpret_cast<void *>(module + 0x100), &start, 4);
                memcpy(reinterpret_cast<void *>(module + 0x104), &end, 4);
                swofty::log("[replay] play test: mode 0 and state 2 set, clip %.3f..%.3f (module duration now %.3f)", double(start), double(end), double(duration));
            }
            at = now + 3000;
        } else if (stage.load() >= 2 && stage.load() < 6 && now >= at.load()) {
            stage++;
            at = now + 5000;
            state_line("while playing");
        }
    }
    // SWOFTY_TT_REPLAY_AFTER=<seconds> is a test switch: a save is marked that long after a session with the bridge on first ticks, and again
    // after each further interval (three times at most), without waiting for a finished lap.
    if (vt_ok && replay_after_ms > 0 && bridge_on) {
        static std::atomic<int64_t> next_at{0};
        static std::atomic<int> fired{0};
        int64_t now = now_ms(), due = next_at.load();
        if (!due)
            next_at = now + replay_after_ms;
        else if (now >= due && fired.load() < 3 && !replay_pending_ms.load() && next_at.compare_exchange_strong(due, now + replay_after_ms)) {
            fired++;
            replay_pending_lap = 1000 * uint32_t(fired.load());
            replay_pending_ms = now;
            swofty::log("[replay] SWOFTY_TT_REPLAY_AFTER: test save %d marked", fired.load());
        }
    }
    int64_t pending = replay_pending_ms.load();
    if (!vt_ok || !tb::replay_capture_due(pending, now_ms(), REPLAY_SAVE_DELAY_MS))
        return;
    uint64_t mgr = replay_manager_ptr.load();
    static std::atomic<uint64_t> searched_for{0};
    if (mgr && !replay_manager_ok(mgr, sys))
        mgr = 0;
    if (!mgr && searched_for.exchange(sys) != sys) {
        std::string how;
        mgr = replay_find_manager(sys, how);
        swofty::log("[replay] replay manager search from the GameReplaySystem 0x%llx: %s", (unsigned long long)sys,
                    mgr ? ("manager 0x" + [&] { char b[24]; snprintf(b, sizeof b, "%llx", (unsigned long long)mgr); return std::string(b); }() + " at " + how).c_str() : "no plausible manager within two pointer hops");
        if (mgr)
            replay_manager_ptr = mgr;
    }
    uint8_t rec = mgr && runtime::readable(reinterpret_cast<void *>(mgr + 0x6e1), 1) ? *reinterpret_cast<uint8_t *>(mgr + 0x6e1) : 0xff;
    uint64_t queue = mgr ? peek64(mgr + 0x150) : 0;
    uint32_t queue_busy = queue ? peek32(queue + 0x48) : 0xffffffffu;
    bool allowed = mgr != 0 && tb::replay_capture_allowed(replay_capture_on, bridge_on, vt_ok, rec == 0xff ? 0 : rec, queue);
    if (++guard_logs <= 8)
        swofty::log("[replay] capture guards at tick %s: switch %d, bridge %d, content %s, GameReplaySystem %d, manager 0x%llx, recording flag [mgr+0x6e1] %u, job queue [mgr+0x150] 0x%llx (busy word [queue+0x48] %u) -> %s",
                    slot, replay_capture_on ? 1 : 0, bridge_on ? 1 : 0, content_ours() ? "ours" : "stock", vt_ok ? 1 : 0, (unsigned long long)mgr, unsigned(rec), (unsigned long long)queue,
                    unsigned(queue_busy), allowed ? "allowed" : "refused");
    if (!allowed)
        return;
    if (!replay_pending_ms.compare_exchange_strong(pending, 0))
        return;
    sys = mgr;
    alignas(16) static uint8_t out[0x240];
    memset(out, 0, sizeof out);
    reinterpret_cast<void *(*)(uint64_t, void *)>(replay_build_path_fn)(sys, out);
    uint64_t pptr = peek64(reinterpret_cast<uint64_t>(out) + 0x10), cap = peek64(reinterpret_cast<uint64_t>(out) + 8);
    if (pptr < reinterpret_cast<uint64_t>(out) || pptr > reinterpret_cast<uint64_t>(out) + 0x40 || cap == 0 || cap > 0x100 || !runtime::readable(reinterpret_cast<void *>(pptr), 4)) {
        swofty::log("[replay] capture: the game's path builder returned an unexpected string object (ptr 0x%llx, capacity %llu); not saving", (unsigned long long)pptr, (unsigned long long)cap);
        return;
    }
    std::wstring path(reinterpret_cast<const wchar_t *>(pptr));
    if (!replay_precheck(sys))
        return;
    replay_last_status = -1;
    replay_ui_until_ms = now_ms() + 240000;
    bool ok = (reinterpret_cast<uint64_t (*)(uint64_t, const void *)>(replay_save_fn)(sys, reinterpret_cast<const void *>(pptr)) & 0xff) != 0;
    if (!ok)
        replay_ui_until_ms = 0;
    swofty::log("[replay] capture: ReplayManager::SaveFullReplay(%s) %s for the personal best of %u ms", narrow(path.c_str()).c_str(), ok ? "queued" : "refused (a save is already running or nothing is recording)",
                replay_pending_lap.load());
    if (ok) {
        uint32_t lap = replay_pending_lap.load();
        uint64_t stamp = unix_seconds();
        std::thread(replay_watch, path, lap, stamp).detach();
    }
}

void replay_tick_mid(safetyhook::Context &c) { replay_tick(c.rcx, "vslot24"); }
void replay_tick17_mid(safetyhook::Context &c) { replay_tick(c.rcx, "vslot17"); }
// The saved file holds the recording since its last restart, so the recording is restarted when what it holds cannot be part of a best
// lap: a few seconds before the line on a lap already slower than the best, or at the line after a lap that was no best. The game
// restarts its own recording the same way (start lights, rolling start), from this update.
std::atomic<int> replay_restart_wanted{0};
void replay_restart(uint64_t sys)
{
    int why = replay_restart_wanted.exchange(0);
    if (!why)
        return;
    uint64_t mgr = replay_manager_ptr.load();
    bool ok = game.replay_clear && game.replay_reset && replay_capture_on && bridge_on && view::stage.load() == view::off && !in_session_load.load() && !replay_pending_ms.load() &&
              !replay_saving.load() && sys && runtime::readable(reinterpret_cast<void *>(sys), 0xc0) && is_replay_system(sys) && (peek32(sys + 0x88) >> 8 & 0xff) != 0 &&
              peek32(sys + 0x20) == 0 && peek32(sys + 0x78) == 0 && mgr && replay_manager_ok(mgr, sys) && (peek32(mgr + 0x6e0) >> 8 & 0xff) == 1 && int32_t(peek32(mgr + 0x428)) == -1;
    uint64_t queue = ok ? peek64(mgr + 0x150) : 0;
    ok = ok && queue && runtime::readable(reinterpret_cast<void *>(queue + 0x48), 8) && peek64(queue + 0x48) == 0;
    if (!ok) {
        swofty::log("[replay] restart (%s): not now, a guard does not hold", why == 1 ? "before the line" : "at the line");
        return;
    }
    auto *cs = reinterpret_cast<CRITICAL_SECTION *>(peek64(sys + 0xb0));
    EnterCriticalSection(cs);
    reinterpret_cast<void (*)(uint64_t)>(game.replay_clear)(sys);
    reinterpret_cast<void (*)(uint64_t, uint32_t, uint8_t)>(game.replay_reset)(sys, 0, 1);
    *reinterpret_cast<uint8_t *>(sys + 0x88) = 1;
    LeaveCriticalSection(cs);
    swofty::log("[replay] restart (%s): the recording starts again from here", why == 1 ? "before the line" : "at the line");
}

void replay_update_mid(safetyhook::Context &c)
{
    replay_tick(c.rcx, "updateFromModule");
    if (replay_restart_wanted.load())
        replay_restart(c.rcx);
}
// Finds a GameReplaySystem reachable from the replay buffer by plain reads: every qword of the buffer's first 0x200 bytes that is a readable
// user-space pointer is checked for the system's vtable word, and so is every qword of the first 0x100 bytes behind such a pointer. Only
// readable() guarded 8-byte reads; no calls. Runs once per buffer object.
uint64_t replay_find_system(uint64_t buffer, std::string &how)
{
    uintptr_t want = game.replay_system_vt;
    if (!want)
        return 0;
    auto user = [](uint64_t p) { return p > 0x10000 && p < 0x7fffffffffffull && (p & 7) == 0; };
    for (uint64_t o = 0; o < 0x200; o += 8) {
        uint64_t p = peek64(buffer + o);
        if (!user(p) || !runtime::readable(reinterpret_cast<void *>(p), 8))
            continue;
        if (peek64(p) == want) {
            how = "buffer+0x" + std::to_string(o);
            return p;
        }
        if (!runtime::readable(reinterpret_cast<void *>(p), 0x100))
            continue;
        for (uint64_t o2 = 0; o2 < 0x100; o2 += 8) {
            uint64_t q = peek64(p + o2);
            if (user(q) && runtime::readable(reinterpret_cast<void *>(q), 8) && peek64(q) == want) {
                how = "buffer+0x" + std::to_string(o) + " -> +0x" + std::to_string(o2);
                return q;
            }
        }
    }
    return 0;
}

void replay_frame_mid(safetyhook::Context &c)
{
    static std::atomic<uint64_t> searched{0};
    if (!replay_system_ptr.load() && searched.exchange(c.rcx) != c.rcx && c.rcx) {
        std::string how;
        uint64_t sys = replay_find_system(c.rcx, how);
        swofty::log("[replay] beginFrameWrite buffer 0x%llx: %s", (unsigned long long)c.rcx,
                    sys ? ("GameReplaySystem 0x" + [&] { char b[24]; snprintf(b, sizeof b, "%llx", (unsigned long long)sys); return std::string(b); }() + " found at " + how).c_str()
                        : "no GameReplaySystem within two pointer hops");
        if (sys)
            replay_system_ptr = sys;
    }
    replay_tick(0, "beginFrameWrite");
}

std::atomic<uint64_t> garage_driven_panel{0}, garage_driven_count{0}, garage_built_ours{0};
uintptr_t garage_build_fn = 0;

// Read-only: logs every run of the tab builder (panel, session type, tab count before, whether our content was active) and remembers the
// panel when it ran for our content, so the Update hook does not rebuild it again.
void garage_build_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    uint64_t panel = c.rcx, inner = peek64(panel + 0x268), mgr = inner ? peek64(inner + 0x1a8) : 0;
    uint32_t session = mgr ? peek32(mgr + 4) : 0xffffffffu;
    bool ours = bridge_on && content_ours();
    if (ours)
        garage_built_ours = panel;
    if (++logged <= 12)
        swofty::log("[boards] garage monitor tab builder runs for panel 0x%llx: session type 0x%x, %llu tab(s) before, our content %d", (unsigned long long)panel, session,
                    (unsigned long long)((peek64(panel + 0x410) - peek64(panel + 0x408)) / 8), ours ? 1 : 0);
}
uintptr_t garage_priv_sub = 0;

// Ownership: rcx is the garage monitor panel, the object whose Update is running on the game thread right now. Reads are plain values
// guarded by readable(). The one call is the panel's own subscriber for car_monitor_tt_leaderboard_privilege_check with a zeroed local
// message whose bool (+0x10) is true; the subscriber only reads that byte. Done once per panel object.
void garage_update_mid(safetyhook::Context &c)
{
    uint64_t panel = c.rcx;
    if (!garage_priv_sub || !bridge_on || garage_driven_panel.load() == panel)
        return;
    uint64_t tb_ = 0, te = 0;
    if (!runtime::readable(reinterpret_cast<void *>(panel + 0x408), 16))
        return;
    tb_ = peek64(panel + 0x408), te = peek64(panel + 0x410);
    if (garage_build_fn) {
        uint64_t inner = peek64(panel + 0x268), mgr = inner ? peek64(inner + 0x1a8) : 0;
        int session = mgr ? int(peek32(mgr + 4)) : -1;
        if (tb::garage_should_rebuild(bridge_on, content_ours(), panel, garage_built_ours.load(), session, te > tb_ ? (te - tb_) / 8 : 0)) {
            garage_built_ours = panel;
            reinterpret_cast<void (*)(uint64_t)>(garage_build_fn)(panel);
            tb_ = peek64(panel + 0x408), te = peek64(panel + 0x410);
            swofty::log("[boards] garage monitor: the tab builder was run again for panel 0x%llx for our content (time trial garage): %llu tab(s) now", (unsigned long long)panel,
                        (unsigned long long)(te > tb_ ? (te - tb_) / 8 : 0));
        }
    }
    if (!tb::garage_should_drive(bridge_on, content_ours(), panel, tb_, te, garage_driven_panel.load(), garage_driven_count.load()))
        return;
    garage_driven_panel = panel;
    garage_driven_count = (te - tb_) / 8;
    alignas(16) uint8_t msg[0x40] = {};
    msg[0x10] = 1;
    reinterpret_cast<uint64_t (*)(uint64_t, void *)>(garage_priv_sub)(panel, msg);
    swofty::log("[boards] garage monitor: car_monitor_tt_leaderboard_privilege_check raised as granted on panel 0x%llx (%llu tabs) for our content: the leaderboard tab is enabled by the panel's own subscriber",
                (unsigned long long)panel, (unsigned long long)((te - tb_) / 8));
}

void commit_path_log(const char *what, safetyhook::Context &c, std::atomic<int> &logged)
{
    if (++logged > 24)
        return;
    uint64_t handler = c.rcx, ctx = peek64(handler + 0x58), np = ctx ? peek64(ctx + 0x68) : 0;
    swofty::log("[boards] commit path: %s called for handler 0x%llx board %s (args 0x%llx 0x%llx; entry state %u, download %u, +0x288 %u, +0x9c %u); backtrace %s", what,
                (unsigned long long)handler, np ? tb::read_c(game_mem(), np).c_str() : "?", (unsigned long long)c.rdx, (unsigned long long)c.r8, peek32(handler + 0x1d8),
                peek32(handler + 0x138), peek32(handler + 0x288), peek32(handler + 0x9c), backtrace_text(1, 4).c_str());
}

void save_pb_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    commit_path_log("SavePersonalBest (stores the lap's ghost blob in memory and sets +0x288)", c, n);
}

void handler_reset_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    commit_path_log("Game_GhostLapHandler_Reset (clears the handler's entry and download state)", c, n);
}

void entry_write_mid(safetyhook::Context &c)
{
    static std::atomic<int> n{0};
    commit_path_log("StartLeaderboardEntryWrite (builds the AddLeaderboardEntry request)", c, n);
}

void reader_fail_mid(safetyhook::Context &c)
{
    static std::atomic<int> logged{0};
    if (++logged > 16)
        return;
    swofty::log("[boards] leaderboard reader task cancelled or failed (this raises leaderboard_fail, which disables the garage panel): reader 0x%llx, args 0x%llx 0x%llx 0x%llx; backtrace %s",
                (unsigned long long)c.rcx, (unsigned long long)c.rdx, (unsigned long long)c.r8, (unsigned long long)c.r9, backtrace_text(1, 6).c_str());
}

void select_board_mid(safetyhook::Context &c)
{
    uint64_t kp = peek64(c.rsp + 0x38);
    std::string key = kp ? tb::read_c(game_mem(), kp) : std::string();
    uint64_t reg = c.rbx, first = reg ? peek64(reg + 0x18) : 0, last = reg ? peek64(reg + 0x20) : 0;
    size_t listed = first && last >= first ? size_t(std::min<uint64_t>((last - first) / 8, 100000)) : 0, same_descriptor = 0;
    bool found = false;
    std::string prefix = key.size() >= 5 ? key.substr(0, 5) : key, sample;
    for (size_t i = 0; i < listed; i++) {
        uint64_t board = peek64(first + 8 * i), np = board ? peek64(board + 0x68) : 0;
        std::string name = np ? tb::read_c(game_mem(), np) : std::string();
        if (!_stricmp(name.c_str(), key.c_str()))
            found = true;
        if (name.compare(0, prefix.size(), prefix) == 0) {
            same_descriptor++;
            if (sample.size() < 80 && name.size() > 5)
                sample += (sample.empty() ? "" : " ") + name.substr(5);
        }
    }
    char tag[96];
    snprintf(tag, sizeof tag, "%s/%zu/%d", key.c_str(), listed, found);
    {
        std::lock_guard<std::mutex> lk(lock);
        if (select_seen == tag)
            return;
        select_seen = tag;
    }
    swofty::log("[boards] %sleaderboard screen board lookup: key '%s', %zu board(s) listed (registry 0x%llx), %s; %zu with the prefix %s (tracks %s)",
                diag_on ? "diag: " : "", key.c_str(), listed, (unsigned long long)reg, found ? "FOUND" : "NOT in the list: the screen opens empty",
                same_descriptor, prefix.c_str(), sample.empty() ? "none" : sample.c_str());
}

std::string watch_seen;
int32_t last_state = -1;

void describe_checks(uint64_t handler, const std::string &board)
{
    static std::atomic<uint64_t> done{0};
    if (done.exchange(handler) == handler)
        return;
    uint64_t base = swofty::api->module_base();
    auto rva = [&](uint64_t a) { return a >= base && a < base + 0x20000000 ? a - base : 0; };
    uint64_t obj = peek64(handler + 0x18), vt = obj ? peek64(obj) : 0;
    std::string list;
    uint64_t b = obj ? peek64(obj + 0x38) : 0, e = obj ? peek64(obj + 0x40) : 0;
    size_t n = b && e >= b ? std::min<uint64_t>((e - b) / 16, 32) : 0;
    for (size_t i = 0; i < n; i++) {
        uint64_t v = peek64(b + 16 * i + 8), vvt = v ? peek64(v) : 0;
        char one[120];
        snprintf(one, sizeof one, "%s[object 0x%llx vtable base+0x%llx, +0x88 -> base+0x%llx]", i ? ", " : "", (unsigned long long)v, (unsigned long long)rva(vvt),
                 (unsigned long long)rva(vvt ? peek64(vvt + 0x88) : 0));
        list += one;
    }
    swofty::log("[boards] first-lap save checks of handler 0x%llx board %s: the system at handler+0x18 is object 0x%llx, vtable base+0x%llx, +0x130 check -> base+0x%llx; "
                "%zu validator(s) at +0x38: %s",
                (unsigned long long)handler, board.c_str(), (unsigned long long)obj, (unsigned long long)rva(vt), (unsigned long long)rva(vt ? peek64(vt + 0x130) : 0), n,
                list.empty() ? "none" : list.c_str());
}

void handler_watch(uint64_t handler, const std::string &board)
{
    describe_checks(handler, board);
    auto b = [&](uint64_t off) { return unsigned(peek32(handler + off) & 0xff); };
    auto w = [&](uint64_t off) { return unsigned(peek32(handler + off)); };
    uint64_t finder = peek64(handler + 0x508);
    int32_t state = int32_t(w(0x1d8));
    int finder_state = finder ? int(peek32(finder + 0xa0)) : -1;
    tb::BoardName bn = tb::parse_board(board);
    bool ours = bridge_on && bn.ok && tb::ours(bn.descriptor, content_ours());
    char force[8] = {};
    bool forced = ours && state == 4 && GetEnvironmentVariableA("SWOFTY_TT_FORCE_EMPTY", force, sizeof force) && force[0] == '1';
    if (forced) {
        *reinterpret_cast<uint32_t *>(handler + 0x1d8) = 1;
        state = 1;
    }
    std::string blockers;
    auto need = [&](bool ok, const char *what) {
        if (!ok)
            blockers += std::string(blockers.empty() ? "" : ", ") + what;
    };
    bool task_out = w(0x1d0) != 0 || w(0x1d4) != 0;
    need(b(0x65) != 0, "no new personal best flagged (+0x65)");
    need(b(0x63) != 0, "+0x63 clear");
    need(state == 1, "entry state is not 1");
    need(w(0x94) == 0 && w(0x98) == 0, "+0x94/+0x98 busy");
    need(w(0x130) == 0 && w(0x134) == 0, "+0x130/+0x134 busy");
    need(!task_out, "a leaderboard task is out (+0x1d0/+0x1d4)");
    need(b(0x66) == 0, "+0x66 set (personal best save pending)");
    need(finder_state - 2 < 0 || finder_state - 2 > 7, "the rival finder is mid-search");
    uint64_t sysobj = peek64(handler + 0x18), flag_mask = sysobj ? peek64(sysobj + 0xba8) : 0;
    unsigned flag = sysobj ? unsigned(peek32(sysobj + 0xba0) & 0xff) : 0, limit = sysobj ? peek32(sysobj + 0xbd0) : 0;
    bool check1 = sysobj && ((flag_mask & 1) == flag);
    uint64_t lst = sysobj ? peek64(sysobj + 0x38) : 0, v0 = lst ? peek64(lst + 8) : 0, p1 = v0 ? peek64(v0 + 8) : 0, p2 = p1 ? peek64(p1 + 0x1a8) : 0, p3 = p2 ? peek64(p2 + 0x30) : 0;
    float floor_s = 0;
    if (p3) {
        uint32_t m = peek32(p3 + 0x2a0) ^ peek32(p3 + 0x2a8);
        memcpy(&floor_s, &m, 4);
    }
    std::string active;
    if (sysobj && lst) {
        uint64_t end = peek64(sysobj + 0x40);
        size_t count = end >= lst ? std::min<uint64_t>((end - lst) / 16, 6) : 0;
        static const uint32_t offs[6] = {0x88, 0x40, 0x38, 0x18, 0x28, 0};
        for (size_t i = 0; i < count; i++) {
            uint64_t v = peek64(lst + 16 * i + 8);
            if (!v || !offs[i]) {
                active += std::string(i ? " " : "") + "?";
                continue;
            }
            bool on = (peek64(v + offs[i] + 8) & 1) != (peek32(v + offs[i]) & 0xff);
            active += std::string(i ? " " : "") + (on ? "1" : "0");
        }
    }
    bool a_ok = sysobj && ((peek64(sysobj + 0x170) & 1) == (peek32(sysobj + 0x168) & 0xff));
    char checks[400];
    snprintf(checks, sizeof checks,
             "check 1 (session timer flag) %s [limit %u s, flag 2 %s, system flag A %s, +0x198 object %s], validators reporting activity (0 1 2 3 4 5): %s, validator 0 minimum lap floor %.3f s%s",
             check1 ? "ok" : "TRIPPED", limit, sysobj && ((peek64(sysobj + 0xbc8) & 1) == (peek32(sysobj + 0xbc0) & 0xff)) ? "ok" : "off", a_ok ? "ok" : "off",
             sysobj && peek64(sysobj + 0x198) ? "present" : "absent", active.empty() ? "?" : active.c_str(), floor_s, p3 ? "" : " (not readable)");
    blockers += std::string(blockers.empty() ? "" : "; ") + checks;
    char t[640];
    snprintf(t, sizeof t,
             "entry state %d (0 reading, 1 entry known, 2 entry unusable, 4 no entry)%s, task out %d, download %u, flags 60..66 %u %u %u %u %u %u %u, memory pb valid +0x288 %u, "
             "+0x9c %u, +0x31c %u, pb ms %d, remote pb ms +0x300 %d, finder state %d; write blocked by: %s",
             state, forced ? " (forced by SWOFTY_TT_FORCE_EMPTY=1)" : "", task_out ? 1 : 0, w(0x138), b(0x60), b(0x61), b(0x62), b(0x63), b(0x64), b(0x65), b(0x66), w(0x288), w(0x9c),
             b(0x31c), int(w(0x1b0)), int(w(0x300)), finder_state, blockers.empty() ? "nothing visible (presence/session checks and 0x142f91900 are logged separately)" : blockers.c_str());
    std::string full = t;
    static int watch_logged = 0;
    if (watch_seen != full && watch_logged < 300) {
        watch_logged++;
        watch_seen = full;
        swofty::log("[boards] %shandler 0x%llx board %s: %s", diag_on ? "diag: " : "", (unsigned long long)handler, board.c_str(), full.c_str());
    }
}

void diag_tick()
{
    session_lookup_tick();
    uintptr_t sys = diag_storage.load();
    if (!sys)
        return;
    uint64_t handler = peek64(sys + 0x278);
    // Reading the handler, the board name and describing the handler are guarded reads; tt_bridge_tick runs this at most twice a
    // second and only while a Time Trial session is live (tb::diag_tick_on, tb::diag_tick_due).
    std::string name;
    if (handler) {
        uint64_t ctx = peek64(handler + 0x58);
        uint64_t np = ctx ? peek64(ctx + 0x68) : 0;
        name = np ? tb::read_c(game_mem(), np) : std::string();
        if (runtime::readable(reinterpret_cast<void *>(handler), 0x520))
            handler_watch(handler, name);
    }
    if (handler == diag_handler_seen)
        return;
    diag_handler_seen = handler;
    if (diag_on)
        swofty::log("[boards] diag: ghost lap handler of storage system 0x%llx is now 0x%llx%s%s", (unsigned long long)sys, (unsigned long long)handler,
                    name.empty() ? "" : ", board ", name.c_str());
}

bool section_of(const char *want, const uint8_t **begin, size_t *size)
{
    auto base = reinterpret_cast<const uint8_t *>(swofty::api->module_base());
    auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS64 *>(base + dos->e_lfanew);
    auto sec = IMAGE_FIRST_SECTION(nt);
    for (unsigned i = 0; i < nt->FileHeader.NumberOfSections; i++) {
        char nm[9] = {};
        memcpy(nm, sec[i].Name, 8);
        if (!strcmp(nm, want)) {
            *begin = base + sec[i].VirtualAddress;
            *size = sec[i].Misc.VirtualSize;
            return true;
        }
    }
    return false;
}

void install_gates()
{
    const uint8_t *buf = nullptr;
    size_t n = 0;
    if (!section_of(".xtls", &buf, &n)) {
        swofty::log("[boards] gate: off, the code section was not found");
        return;
    }
    auto matches = tb::gate::find_all(buf, n);
    std::vector<int64_t> targets;
    for (int i = 0; i < 5; i++)
        targets.push_back(found.on_result[i + 1] ? int64_t(found.on_result[i + 1] - reinterpret_cast<uintptr_t>(buf)) : -1);
    auto own = tb::gate::owners(buf, n, matches, targets);
    int armed = 0;
    std::string missing;
    for (int i = 0; i < 5; i++) {
        tb::Method m = tb::Method(i + 1);
        if (own[i] < 0) {
            missing += std::string(missing.empty() ? "" : ", ") + tb::method_name(m);
            continue;
        }
        tb::gate::Site site = tb::gate::site_of(matches[own[i]]);
        GateHook &g = gates[i];
        g.at = reinterpret_cast<uintptr_t>(buf) + site.hook;
        g.resume = reinterpret_cast<uintptr_t>(buf) + site.resume;
        g.session_global = uintptr_t(int64_t(reinterpret_cast<uintptr_t>(buf)) + tb::gate::session_global(buf, matches[own[i]]));
        if (hook_mid(g.hook, g.at, gate_cbs[i], "online-state gate")) {
            armed++;
            swofty::log("[boards] gate %s armed at base+0x%llx (OnStart template match %zu of %zu); %s", tb::method_name(m),
                        (unsigned long long)(g.at - swofty::api->module_base()), size_t(own[i]) + 1, matches.size(),
                        bridge_on ? "requests of our boards skip the online-state checks" : "diagnostics only");
        } else
            g.at = 0;
    }
    if (!missing.empty())
        swofty::log("[boards] gate: no OnStart found for %s (%zu template matches); those methods keep the game's online-state checks", missing.c_str(),
                    matches.size());
    if (!armed)
        swofty::log("[boards] gate: off, no online-state hook armed");
}

// The template's first bytes: what a result handler found from its neighbour (S_GET_TT ...) must start with.
uintptr_t on_result_entry(const runtime::Sig &sig)
{
    static const uint8_t entry[] = {0x41, 0x57, 0x41, 0x56};
    static const uint8_t body[] = {0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8};
    uintptr_t at = runtime::find(sig, "boards", "boards");
    if (!at)
        return 0;
    auto *b = reinterpret_cast<const uint8_t *>(at);
    bool ok = runtime::readable(b, 0x20) && !memcmp(b, entry, sizeof entry);
    if (ok) {
        ok = false;
        for (size_t i = 12; i + sizeof body <= 0x1c && !ok; i++)
            ok = b[i - 7] == 0x48 && b[i - 6] == 0x81 && b[i - 5] == 0xEC && !memcmp(b + i, body, sizeof body);
    }
    if (!ok)
        swofty::log("[boards] %s: the code at base+0x%llx is not a result handler; not used", sig.what, (unsigned long long)(at - swofty::api->module_base()));
    return ok ? at : 0;
}

void resolve_game()
{
    game.qpc_offset = runtime::find(S_QPC_OFFSET, "boards", "boards");
    game.friends_resume = runtime::find(S_FRIENDS_RESUME, "boards", "boards");
    game.replay_system_vt = runtime::find(S_REPLAY_SYSTEM_VT, "boards", "boards");
    game.string_vt = runtime::find(S_STRING_VT, "boards", "boards");
    game.mode_vt = runtime::find(S_MODE_VT, "boards", "boards");
    game.queue_busy = runtime::find(S_QUEUE_BUSY, "boards", "boards");
    game.replay_set_state = runtime::find(S_REPLAY_SET_STATE, "boards", "boards");
    game.replay_seek = runtime::find(S_REPLAY_SEEK, "boards", "boards");
    game.replay_clear = runtime::find(S_REPLAY_CLEAR, "boards", "boards");
    game.replay_reset = runtime::find(S_REPLAY_RESET, "boards", "boards");
    game.view_enter_fn = runtime::find(S_VIEW_ENTER_FN, "boards", "boards");
    game.view_resume = runtime::find(S_VIEW_RESUME, "boards", "boards");
    if (!game.replay_system_vt)
        swofty::log("[replay] off: the replay system vtable did not resolve on this build");
    if (!game.view_ready())
        swofty::log("[replay] Theatre viewer route off: one of its addresses did not resolve on this build");
    if (!game.replay_clear || !game.replay_reset)
        swofty::log("[replay] recording restart off: its functions did not resolve on this build");
}

void setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    bridge_on = gate.is_object() && gate["tt_bridge"].as_bool(false);
    boards_on = gate.is_object() && gate["tt_boards"].as_bool(false);
    if (!runtime::verified_build()) {
        swofty::log("[boards] off: the hook points and struct offsets are checked on the Steam and EA builds; this exe is a different build");
        return;
    }
    resolve_game();
    char gv[16] = {};
    DWORD gn = GetEnvironmentVariableA("SWOFTY_TT_GUARDS", gv, sizeof gv);
    guards_on = (gate.is_object() && gate["tt_guards"].as_bool(false)) || (gn && gn < sizeof gv && !strcmp(gv, "1"));
    request_guard = guards_on && !env_off("SWOFTY_TT_REQUEST_GUARD", "addon board write guard");
    char dv[16] = {};
    DWORD dn = GetEnvironmentVariableA("SWOFTY_TT_DIAG", dv, sizeof dv);
    diag_on = dn && dn < sizeof dv && !strcmp(dv, "1");
    {
        char rv[8] = {};
        DWORD rn = GetEnvironmentVariableA("SWOFTY_TT_REPLAY_CAPTURE", rv, sizeof rv);
        replay_capture_on = !(rn && rn < sizeof rv && !strcmp(rv, "0"));
        replay_capture_any = rn && rn < sizeof rv && !strcmp(rv, "any");
        char av[8] = {};
        DWORD an = GetEnvironmentVariableA("SWOFTY_TT_REPLAY_AFTER", av, sizeof av);
        replay_after_ms = an && an < sizeof av ? int64_t(atoi(av)) * 1000 : 0;
        char pv[512] = {};
        DWORD pn = GetEnvironmentVariableA("SWOFTY_TT_REPLAY_PLAY", pv, sizeof pv);
        if (pn && pn < sizeof pv)
            replay_play_path = pv;
        char fv[8] = {};
        DWORD fn = GetEnvironmentVariableA("SWOFTY_TT_REPLAY_FORCE", fv, sizeof fv);
        replay_force = fn && fn < sizeof fv ? (!strcmp(fv, "self") ? 1 : !strcmp(fv, "slot") ? 2 : 0) : 2;
    }
    track_watch = bridge_on || guards_on || diag_on;
    table = tb::Table(unix_seconds() * 100);
    make_dirs();

    found.on_result[int(tb::Method::get_tt)] = on_result_entry(S_GET_TT);
    found.on_result[int(tb::Method::get_friends_tt)] = on_result_entry(S_GET_FRIENDS);
    found.on_result[int(tb::Method::get_binary)] = on_result_entry(S_GET_BINARY);
    found.on_result[int(tb::Method::add_entry)] = on_result_entry(S_ADD_ENTRY);
    found.on_result[int(tb::Method::update_binary)] = on_result_entry(S_UPDATE_BINARY);
    std::string missing;
    for (auto m : {tb::Method::get_tt, tb::Method::get_friends_tt, tb::Method::get_binary, tb::Method::add_entry, tb::Method::update_binary})
        if (!found.on_result[int(m)])
            missing += std::string(missing.empty() ? "" : ", ") + tb::method_name(m);
    swofty::log("[boards] bridge %s; OnResult resolved for %s", bridge_on ? "on (tt.json tt_bridge)" : "off (tt.json has no tt_bridge)",
                missing.empty() ? "all five methods" : ("all but " + missing).c_str());

    if (diag_on)
        swofty::log("[boards] diag mode on (SWOFTY_TT_DIAG=1): logging only the online-state gate, pushes, board XML, the popup trigger and the ghost handler%s",
                    bridge_on ? "; the bridge itself is also on" : "");
    uintptr_t push = (bridge_on || request_guard || diag_on) ? runtime::find(S_PUSH, "boards", "boards") : 0;
    if (push) {
        push_hooked = hook_mid(h_push, push, push_mid, "request redirect");
        if (push_hooked)
            swofty::log("[boards] request redirect armed at base+0x%llx (%s)", (unsigned long long)(push - swofty::api->module_base()),
                        bridge_on ? "addon board requests go to the mailbox" : request_guard ? "addon board writes answered locally" : "diagnostics only");
    } else if (bridge_on || request_guard || diag_on)
        swofty::log("[boards] request redirect: off, needs %s (byte signature matched nothing unique)", S_PUSH.what);

    if (bridge_on || diag_on)
        install_gates();
    if (bridge_on || diag_on) {
        if (uintptr_t at = runtime::find(S_FINDER_GATE, "boards", "boards"))
            if (hook_mid(h_finder, at, finder_mid, "rival finder online gate"))
                swofty::log("[boards] rival finder gate armed at base+0x%llx (%s)", (unsigned long long)(at - swofty::api->module_base()),
                            bridge_on ? "our boards count as online for the rival search" : "diagnostics only");
    }
    if (bridge_on || diag_on) {
        if (uintptr_t at = runtime::find(S_TS_FLAG, "boards", "boards")) {
            uintptr_t start = at - 30;
            auto rip = [&](uintptr_t insn) { return insn + 7 + uintptr_t(int64_t(*reinterpret_cast<const int32_t *>(insn + 3))); };
            ts_global_a = rip(start + TS_FLAG_GLOBAL_A);
            ts_global_b = rip(start + TS_FLAG_GLOBAL_B);
            if (hook_mid(h_ts_flag, at, ts_flag_mid, "track select leaderboard flag"))
                swofty::log("[boards] track select leaderboard button armed at base+0x%llx (%s)", (unsigned long long)(at - swofty::api->module_base()),
                            bridge_on ? "shown for our content" : "diagnostics only");
        }
        if (bridge_on)
            if (uintptr_t at = runtime::find(S_WRITE_READY, "boards", "boards")) {
                auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&write_ready_hook));
                if (h) {
                    h_write_ready = std::move(*h);
                    swofty::log("[boards] leaderboard write precondition hooked at base+0x%llx (our boards count as ready)", (unsigned long long)(at - swofty::api->module_base()));
                } else
                    swofty::log("[boards] leaderboard write precondition: off, the inline hook failed");
            }
        if (bridge_on)
            if (uintptr_t at = runtime::find(S_WRITE_TOKEN, "boards", "boards"))
                if (hook_mid(h_write_token, at, write_token_mid, "personal best write session token test"))
                    swofty::log("[boards] personal best write token test hooked at base+0x%llx (our boards count as signed in)", (unsigned long long)(at - swofty::api->module_base()));
        if (uintptr_t at = runtime::find(S_HANDLER_READY, "boards", "boards")) {
            auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&handler_ready_hook));
            if (h) {
                h_ready = std::move(*h);
                swofty::log("[boards] handler personal best check 0x142f91900 logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
            }
        }
        if (uintptr_t at = runtime::find(S_GARAGE_MSG, "boards", "boards"))
            if (hook_mid(h_garage, at, garage_msg_mid, "garage monitor messages"))
                swofty::log("[boards] garage monitor messages logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        for (auto [sig, hook, fn, label] : {std::tuple<const runtime::Sig *, safetyhook::MidHook *, safetyhook::MidHookFn, const char *>{&S_SAVE_PB, &h_save_pb, save_pb_mid, "SavePersonalBest"},
                                            {&S_HANDLER_RESET, &h_reset, handler_reset_mid, "handler reset"},
                                            {&S_ENTRY_WRITE, &h_entry_write, entry_write_mid, "leaderboard entry write"}})
            if (uintptr_t at = runtime::find(*sig, "boards", "boards"))
                if (hook_mid(*hook, at, fn, label))
                    swofty::log("[boards] commit path: %s logged at base+0x%llx", label, (unsigned long long)(at - swofty::api->module_base()));
        for (auto [sig, hook, fn, label] : {std::tuple<const runtime::Sig *, safetyhook::MidHook *, safetyhook::MidHookFn, const char *>{&S_WATCHDOG_REARM, &h_rearm, rearm_mid, "session timer re-arm"},
                                            {&S_WATCHDOG_TRIP, &h_trip, trip_mid, "session timer flag check"}})
            if (uintptr_t at = runtime::find(*sig, "boards", "boards"))
                if (hook_mid(*hook, at, fn, label))
                    swofty::log("[boards] %s logged at base+0x%llx", label, (unsigned long long)(at - swofty::api->module_base()));
        if (bridge_on || diag_on) {
            if (uintptr_t at = runtime::find(S_GARAGE_CHECK_DONE, "boards", "boards"))
                if (hook_mid(h_garage_done, at, garage_done_mid, "garage tt privilege result"))
                    swofty::log("[boards] garage tt_privilege_check_done logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
            if (uintptr_t at = runtime::find(S_GARAGE_CMD, "boards", "boards"))
                if (hook_mid(h_garage_cmd, at, garage_cmd_mid, "garage view model commands"))
                    swofty::log("[boards] garage view model commands logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        }
        if (bridge_on)
            for (auto [sig, hook, fn, label] : {std::tuple<const runtime::Sig *, safetyhook::MidHook *, safetyhook::MidHookFn, const char *>{&S_GARAGE_TAB_BLOCK, &h_tab_block, garage_tab_block_mid, "garage leaderboard block byte"},
                                                {&S_GARAGE_TAB_FLAGS, &h_tab_flags, garage_tab_flags_mid, "garage leaderboard feature flag"},
                                                {&S_GARAGE_TAB_GATE, &h_tab_gate, garage_tab_gate_mid, "garage leaderboard gate byte"}})
                if (uintptr_t at = runtime::find(*sig, "boards", "boards"))
                    if (hook_mid(*hook, at, fn, label))
                        swofty::log("[boards] %s read as allowed for our content at base+0x%llx: the garage monitor builds its leaderboard tabs", label, (unsigned long long)(at - swofty::api->module_base()));
        if (bridge_on)
            if (uintptr_t at = game.friends_resume ? runtime::find(S_GARAGE_FRIENDS_TAB, "boards", "boards") : 0)
                if (hook_mid(h_tab_friends, at, garage_friends_tab_mid, "garage friends tab"))
                    swofty::log("[boards] garage friends tab skipped for our content at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        if (bridge_on && replay_capture_on) {
            uintptr_t path_fn = runtime::find(S_REPLAY_PATH, "boards", "boards"), save_fn = runtime::find(S_REPLAY_SAVE, "boards", "boards"), tick = runtime::find(S_REPLAY_TICK, "boards", "boards");
            if (path_fn && save_fn && tick && game.replay_system_vt && hook_mid(h_replay_tick, tick, replay_tick_mid, "replay system tick")) {
                replay_build_path_fn = path_fn;
                replay_save_fn = save_fn;
                hook_mid(h_replay_save, save_fn, replay_save_entry_mid, "replay save observer");
                if (uintptr_t j = runtime::find(S_REPLAY_JOB, "boards", "boards"))
                    hook_mid(h_replay_job, j, replay_job_mid, "replay save job observer");
                for (auto [sig, hook, fn, label] : {std::tuple<const runtime::Sig *, safetyhook::MidHook *, safetyhook::MidHookFn, const char *>{&S_REPLAY_JOB_COMPONENT, &h_job_component, replay_job_component_mid, "save job check 1"},
                                                    {&S_REPLAY_JOB_WRAPPER, &h_job_wrapper, replay_job_wrapper_mid, "save job type check"},
                                                    {&S_REPLAY_LOAD_SESSION, &h_load_session, replay_load_session_mid, "load session lookup"},
                                                    {&S_REPLAY_LOAD_SLOT, &h_load_slot, replay_load_slot_mid, "load save-slot lookup"},
                                                    {&S_REPLAY_JOB_SELECTED, &h_job_selected, replay_job_selected_mid, "save job check 2"},
                                                    {&S_REPLAY_JOB_STORES, &h_job_stores, replay_job_stores_mid, "save job check 3"},
                                                    {&S_REPLAY_JOB_ALLOC, &h_job_alloc, replay_job_alloc_mid, "save job check 4"}})
                    if (uintptr_t at = runtime::find(*sig, "boards", "boards"))
                        hook_mid(*hook, at, fn, label);
                if (replay_force == 2) {
                    save_slot_ctor = runtime::find(S_SAVE_SLOT_CTOR, "boards", "boards");
                    if (uintptr_t snap = runtime::find(S_SAVE_SLOT_SNAPSHOT, "boards", "boards"))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(snap), reinterpret_cast<void *>(&slot_snapshot_hook)))
                            h_slot_snapshot = std::move(*h);
                    if (uintptr_t a = runtime::find(S_REPLAY_LOAD_START, "boards", "boards"))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&replay_load_start_hook)))
                            h_load_start = std::move(*h);
                    swofty::log("[replay] load start hook %s", h_load_start ? "armed" : "NOT armed");
                    if (uintptr_t a = runtime::find(S_REPLAY_META, "boards", "boards"))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&replay_meta_hook)))
                            h_replay_meta = std::move(*h);
                    swofty::log("[replay] metadata hook %s", h_replay_meta ? "armed" : "NOT armed");
                    if (uintptr_t a = runtime::find(S_REPLAY_TITLE, "boards", "boards"))
                        hook_mid(h_replay_title, a, replay_title_mid, "replay title");
                    view::flying_lap_fn = runtime::find(S_FLYING_LAP, "boards", "boards");
                    if (uintptr_t a = runtime::find(S_REPLAY_INFO_LOAD, "boards", "boards"))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&info_load_hook)))
                            h_info_load = std::move(*h);
                    if (uintptr_t a = runtime::find(S_VM_SESSION_SETUP, "boards", "boards"))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&vm_session_setup_hook)))
                            h_vm_session_setup = std::move(*h);
                    if (uintptr_t a = game.view_resume ? runtime::find(S_VIEW_ENTER, "boards", "boards") : 0)
                        hook_mid(view::h_enter, a, view::enter_mid, "fastest lap viewer enter");
                    if (uintptr_t a = runtime::find(S_REPLAY_LOAD_READY, "boards", "boards"))
                        hook_mid(h_load_ready, a, replay_load_ready_mid, "load start readiness");
                    // Both hooks go on the function's jump thunk in ordinary code, never into the protector's section.
                    auto thunk = [](const runtime::Sig &sig) -> uintptr_t {
                        uintptr_t a = runtime::find(sig, "boards", "boards");
                        if (a && *reinterpret_cast<const uint8_t *>(a) != 0xE9) {
                            swofty::log("[replay] %s: base+0x%llx is not a jump thunk; not hooked", sig.what, (unsigned long long)(a - swofty::api->module_base()));
                            return 0;
                        }
                        return a;
                    };
                    if (uintptr_t a = thunk(S_SAVE_SLOT_DEFAULT))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&slot_default_hook)))
                            h_slot_default = std::move(*h);
                    if (uintptr_t a = thunk(S_SAVE_SLOT_REFRESH))
                        if (auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(a), reinterpret_cast<void *>(&slot_refresh_hook)))
                            h_slot_refresh = std::move(*h);
                    swofty::log("[replay] SWOFTY_TT_REPLAY_FORCE=slot: save-slot constructor %s, snapshot hook %s, default slot hook %s, refresh hook %s", save_slot_ctor ? "found" : "NOT found",
                                h_slot_snapshot ? "armed" : "NOT armed", h_slot_default ? "armed" : "NOT armed", h_slot_refresh ? "armed" : "NOT armed");
                }
                uintptr_t ui_start = runtime::find(S_REPLAY_UI_START, "boards", "boards"), ui_result = runtime::find(S_REPLAY_UI_RESULT, "boards", "boards");
                if (ui_start && ui_result) {
                    auto a = safetyhook::InlineHook::create(reinterpret_cast<void *>(ui_start), reinterpret_cast<void *>(&replay_ui_start_hook));
                    auto b = safetyhook::InlineHook::create(reinterpret_cast<void *>(ui_result), reinterpret_cast<void *>(&replay_ui_result_hook));
                    if (a && b) {
                        h_rui_start = std::move(*a);
                        h_rui_result = std::move(*b);
                        swofty::log("[replay] the game's save messages are hooked (suppressed only during our own save)");
                    } else
                        swofty::log("[replay] the game's save message hooks failed: our save will show the game's messages");
                }
                if (uintptr_t t17 = runtime::find(S_REPLAY_TICK17, "boards", "boards"))
                    hook_mid(h_replay_tick17, t17, replay_tick17_mid, "replay system tick (slot 17)");
                if (uintptr_t up = runtime::find(S_REPLAY_UPDATE, "boards", "boards"))
                    hook_mid(h_replay_update, up, replay_update_mid, "replay system updateFromModule");
                if (uintptr_t fr = runtime::find(S_REPLAY_FRAME, "boards", "boards"))
                    hook_mid(h_replay_frame, fr, replay_frame_mid, "replay buffer beginFrameWrite");
                std::thread(replay_inbox).detach();
                swofty::log("[replay] capture armed (SWOFTY_TT_REPLAY_CAPTURE=1): tick at base+0x%llx, save at base+0x%llx, path builder at base+0x%llx", (unsigned long long)(tick - swofty::api->module_base()),
                            (unsigned long long)(save_fn - swofty::api->module_base()), (unsigned long long)(path_fn - swofty::api->module_base()));
            } else
                swofty::log("[replay] capture: off, a signature or the tick hook failed");
        }
        if (bridge_on || diag_on)
            if (uintptr_t fn = runtime::find(S_GARAGE_BUILD, "boards", "boards"))
                if (hook_mid(h_tab_build, fn + 0x1c, garage_build_mid, "garage tab builder")) {
                    garage_build_fn = bridge_on ? fn : 0;
                    swofty::log("[boards] garage tab builder logged at base+0x%llx", (unsigned long long)(fn - swofty::api->module_base()));
                }
        if (bridge_on)
            if (uintptr_t sub = runtime::find(S_GARAGE_PRIV_SUB, "boards", "boards"))
                if (uintptr_t at = runtime::find(S_GARAGE_UPDATE, "boards", "boards"))
                    if (hook_mid(h_garage_update, at, garage_update_mid, "garage monitor panel update")) {
                        garage_priv_sub = sub;
                        swofty::log("[boards] garage monitor privilege drive armed at base+0x%llx (subscriber base+0x%llx)", (unsigned long long)(at - swofty::api->module_base()),
                                    (unsigned long long)(sub - swofty::api->module_base()));
                    }
        if (uintptr_t at = runtime::find(S_READER_FAIL, "boards", "boards"))
            if (hook_mid(h_reader_fail, at, reader_fail_mid, "leaderboard reader failure"))
                swofty::log("[boards] leaderboard reader failure logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        if (uintptr_t at = runtime::find(S_SELECT_BOARD, "boards", "boards"))
            if (hook_mid(h_select, at, select_board_mid, "leaderboard screen board lookup"))
                swofty::log("[boards] leaderboard screen board lookup logged at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        if (bridge_on)
            if (const SwoftyLang *l = runtime::lang_table()) {
                int r = l->override_set("*", "lng_global_leaderboard", "SWOFTY LEADERBOARD");
                swofty::log("[boards] leaderboard tab label: %s", r == SWOFTY_ASSET_OK ? "renamed to SWOFTY LEADERBOARD (lng_global_leaderboard override)"
                                                                                       : "the text override is unavailable, the tab keeps GLOBAL LEADERBOARD");
                int g = l->override_set("*", "lng_default_ghost", "Personal Best");
                swofty::log("[boards] ghost name tag: %s", g == SWOFTY_ASSET_OK ? "lng_default_ghost reads Personal Best (the only default ghost of a board session is the player's own best)"
                                                                                 : "the text override is unavailable, the tag keeps Default Ghost");
            }
        if (uintptr_t at = runtime::find(S_PRIV_GATE, "boards", "boards"))
            if (hook_mid(h_priv, at, priv_mid, "leaderboard privilege check"))
                swofty::log("[boards] leaderboard privilege check skip armed at base+0x%llx (%s)", (unsigned long long)(at - swofty::api->module_base()),
                            bridge_on ? "our leaderboard button needs no platform check" : "diagnostics only");
    }
    if (bridge_on || diag_on)
        if (uintptr_t at = runtime::find(S_ACCOUNT_ERROR, "boards", "boards"))
            if (hook_mid(h_account_error, at, account_error_mid, "account state error"))
                swofty::log("[boards] account-state error handler hooked at base+0x%llx (%s)", (unsigned long long)(at - swofty::api->module_base()),
                            bridge_on ? "its popups are not shown for addon content" : "diagnostics only");
    if (diag_on) {
    }
    if (bridge_on || diag_on)
        if (uintptr_t at = runtime::find(S_SESSION_ENTER, "boards", "boards"))
            if (hook_mid(h_session_enter, at, session_enter_mid, "storage system session enter"))
                swofty::log("[boards] storage system onSessionEnter hooked at base+0x%llx (handler watch)", (unsigned long long)(at - swofty::api->module_base()));

    char xv[8] = {};
    DWORD xn = GetEnvironmentVariableA("SWOFTY_TT_BOARD_XML", xv, sizeof xv);
    if (xn && xv[0] == '0') {
        swofty::log("[boards] board definitions: off (SWOFTY_TT_BOARD_XML=0)");
    } else if (bridge_on || diag_on) {
        uintptr_t xml = runtime::find(S_XML, "boards", "boards");
        if (xml && !runtime::find(S_XML_FREE, "boards", "boards")) {
            swofty::log("[boards] board definitions: off, the buffer's free site (%s) was not found, so the game's own buffer stays", S_XML_FREE.what);
            xml = 0;
        } else if (!xml)
            swofty::log("[boards] board definitions: off, needs %s (byte signature matched nothing unique)", S_XML.what);
        else if (hook_mid(h_xml, xml, xml_mid, "board definitions"))
            swofty::log("[boards] board definitions armed at base+0x%llx", (unsigned long long)(xml - swofty::api->module_base()));
    }

    bool ghost_on = boards_on && !env_off("SWOFTY_LAP_GHOST", "ghost blob");
    if (bridge_on || ghost_on) {
        uintptr_t pb = runtime::find(S_PB, "boards", "boards");
        pb_armed = ghost_on && pb;
        if (!pb)
            swofty::log("[boards] personal best hook: off, needs %s (byte signature matched nothing unique)%s", S_PB.what,
                        ghost_on ? "; lap files carry no GHST section" : "");
        else if (!hook_mid(h_pb, pb, pb_mid, "personal best hook"))
            pb_armed = false;
        else
            swofty::log("[boards] personal best hook armed at base+0x%llx (%s)", (unsigned long long)(pb - swofty::api->module_base()),
                        ghost_on ? bridge_on ? "ghost blob for the lap recorder and the player id block" : "ghost blob for the lap recorder"
                                 : "player id block");
    }

    if (!guards_on) {
        swofty::log("[boards] guards off: enable with tt_guards after the in-game confirmation");
        return;
    }
    install_guard(h_upload, S_UPLOAD, upload_mid, "SWOFTY_TT_UPLOAD_GUARD", "remote storage upload guard");
    install_guard(h_download, S_DOWNLOAD, download_mid, "SWOFTY_TT_DOWNLOAD_GUARD", "remote storage download guard");
    install_guard(h_racenet, S_RACENET, racenet_mid, "SWOFTY_TT_RACENET_GUARD", "Racenet post guard");
}

void write_request(tb::Pending &p, const std::string &content, const std::string &session_track, const std::string &ghost_b64)
{
    tb::BoardTrack track = tb::request_track(swofty_core::apply::tracks::base(), p.board, session_track);
    std::string json = tb::request_json(p.seq, p.method, p.args, content, track.key, p.board, p.board_text, ghost_b64);
    std::wstring path = outbox + runtime::wide(std::to_string(p.seq) + ".json");
    p.sent = true;
    if (runtime::write_file(path, json))
        swofty::log("[boards] request %llu %s %s written to the mailbox (%zu bytes)", (unsigned long long)p.seq, tb::method_name(p.method),
                    p.board_text.c_str(), json.size());
    else
        swofty::log("[boards] request %llu: writing the outbox file failed (error %lu); it will time out", (unsigned long long)p.seq, GetLastError());
}

void complete(const tb::Completion &done)
{
    tb::Completion c = done;
    if ((c.status < 200 || c.status > 299) && c.method != tb::Method::get_binary) {
        const char *soft = c.method == tb::Method::get_tt || c.method == tb::Method::get_friends_tt
                               ? "{\"Results\":[],\"TotalEntries\":0,\"FromIndex\":0,\"ToIndex\":0}"
                               : tb::local_body(c.method);
        swofty::log("[boards] completion %llu %s %s: the bridge failed with %d (%s); the game gets an empty answer instead of its error popup",
                    (unsigned long long)c.seq, tb::method_name(c.method), c.board_text.c_str(), c.status, tb::why_name(c.why));
        c.status = 200;
        c.body = soft;
    }
    auto resp = std::make_unique<tb::Response>();
    resp->fill(c.status, c.body);
    uintptr_t fn = found.on_result[int(c.method)];
    if (!fn || !runtime::readable(reinterpret_cast<void *>(c.op), 0x218)) {
        swofty::log("[boards] completion %llu %s: dropped (%s)", (unsigned long long)c.seq, tb::method_name(c.method),
                    fn ? "the op is no longer readable" : "no OnResult");
        return;
    }
    if (diag_on)
        swofty::log("[boards] diag: reply of %llu %s %s (status %d): %s", (unsigned long long)c.seq, tb::method_name(c.method), c.board_text.c_str(), c.status,
                    c.body.substr(0, 1800).c_str());
    uint64_t saved = 0;
    {
        std::lock_guard<std::mutex> g(lock);
        auto it = op_vtables.find(c.seq);
        if (it != op_vtables.end()) {
            saved = it->second;
            op_vtables.erase(it);
        }
    }
    if (saved && peek64(c.op) != saved) {
        swofty::log("[boards] completion %llu %s %s: dropped, the op was released or reused while the request was out (vtable 0x%llx, was 0x%llx)",
                    (unsigned long long)c.seq, tb::method_name(c.method), c.board_text.c_str(), (unsigned long long)peek64(c.op), (unsigned long long)saved);
        return;
    }
    reinterpret_cast<OnResult_t>(fn)(reinterpret_cast<void *>(c.op), resp->raw);
    if (runtime::readable(reinterpret_cast<void *>(c.op + 0x1f8), 8)) {
        uint64_t handle = peek64(c.op + 0x1f8);
        if (handle) {
            if (++handle_logged <= 8)
                swofty::log("[boards] completion %llu: the op's OS operation handle (op+0x1f8) is 0x%llx; no HTTP request was made for it, so it is cleared before the game destroys the op",
                            (unsigned long long)c.seq, (unsigned long long)handle);
            *reinterpret_cast<uint64_t *>(c.op + 0x1f8) = 0;
        }
    }
    swofty::log("[boards] completion %llu %s %s: status %d, %zu body bytes, %s after %lld ms (OnResult returned)", (unsigned long long)c.seq,
                tb::method_name(c.method), c.board_text.c_str(), c.status, c.body.size(), tb::why_name(c.why), (long long)c.waited_ms);
    responses.push_back(std::move(resp));
    if (responses.size() > 16)
        responses.pop_front();
}

}

namespace {

std::string read_session_track()
{
    std::string code = runtime::lower(runtime::direct_start::tt::session_track());
    session_custom = tb::board_track(swofty_core::apply::tracks::base(), code).custom;
    std::lock_guard<std::mutex> g(lock);
    session_code = code;
    return code;
}

}

namespace runtime {

void tt_bridge_track_watch(bool session_live)
{
    bool time_trial = session_live && swofty_core::apply::moments::session_mode_track().first == "time_trial";
    bool on = tb::track_watch_on(bridge_on, guards_on, diag_on, session_live, time_trial);
    if (track_session.exchange(on) == on)
        return;
    if (on) {
        track_checked_ms = now_ms();
        std::string code = read_session_track();
        swofty::log("[boards] track watch on: Time Trial session entered on track '%s'; the track is read twice a second until the session ends", code.c_str());
        return;
    }
    std::string code;
    {
        std::lock_guard<std::mutex> g(lock);
        code = session_code;
    }
    swofty::log("[boards] track watch off: the session ended; the last track '%s' is kept", code.c_str());
}

void tt_bridge_tick()
{
    replay_load_tick();
    int64_t t = now_ms();
    bool time_trial_live = track_session.load(std::memory_order_relaxed);
    if (tb::diag_tick_due(tb::diag_tick_on(bridge_on, diag_on, time_trial_live), t, diag_checked_ms.load())) {
        diag_checked_ms = t;
        diag_tick();
    }
    if (tb::track_watch_due(track_watch.load(std::memory_order_relaxed) && time_trial_live, t, track_checked_ms.load())) {
        track_checked_ms = t;
        read_session_track();
    }
    if (!pending_count.load(std::memory_order_relaxed))
        return;
    std::vector<tb::Completion> due;
    {
        std::lock_guard<std::mutex> g(lock);
        std::string content, track, ghost;
        auto unsent = table.unsent();
        if (!unsent.empty()) {
            content = current_content();
            if (content.empty())
                content = "game/f1_2025";
            track = session_code;
            if (have_ghost_block)
                ghost = tb::base64(ghost_block, sizeof ghost_block);
            for (auto *p : unsent)
                write_request(*p, content, track, ghost);
        }
        for (auto *p : table.waiting()) {
            std::wstring path = inbox + wide(std::to_string(p->seq) + ".json");
            int64_t age = 0;
            if (!file_age_seconds(path, &age))
                continue;
            if (age > tb::STALE_SECONDS) {
                DeleteFileW(path.c_str());
                continue;
            }
            std::string text = read_file(path, size_t(8) << 20);
            if (text.empty())
                continue;
            auto answer = tb::parse_answer(text, p->seq);
            DeleteFileW(path.c_str());
            if (!answer) {
                swofty::log("[boards] answer %llu is not a valid {seq, status, body} object; the request gets a 502", (unsigned long long)p->seq);
                p->answered = true;
                p->status = 502;
                p->body = tb::error_body("The answer was not valid");
                continue;
            }
            p->answered = true;
            p->status = answer->status;
            p->body = std::move(answer->body);
        }
        due = table.take_due(now_ms());
        pending_count = int(table.size());
    }
    for (auto &c : due)
        complete(c);
}

bool tt_bridge_pb_armed() { return pb_armed.load(); }
void tt_bridge_lap_finished(int64_t lap_ms, int valid) { replay_lap_finished(lap_ms, valid); }
void tt_bridge_lap_progress(double path_m, double elapsed_s) { replay_lap_progress(path_m, elapsed_s); }

tt_bridge::BoardTrack tt_bridge_board_track(const std::string &code, int stat_hint)
{
    const auto &base = swofty_core::apply::tracks::base();
    tt_bridge::BoardTrack t = tt_bridge::board_track(base, code, stat_hint);
    if (!t.known && !unknown_logged.exchange(true))
        swofty::log("[laps] track base: %s; the board key is the track code '%s' (logged once)",
                    !base.ready ? "not ready" : code.empty() ? "the session's track is not known" : "no track with this code", t.key.c_str());
    return t;
}

std::string tt_bridge_last_board()
{
    std::lock_guard<std::mutex> g(lock);
    return last_board;
}

void tt_bridge_session_enter()
{
    std::lock_guard<std::mutex> g(lock);
    last_board.clear();
    replay_session_best_ms = 0;
    replay_pending_ms = 0;
}

bool tt_bridge_ours_session()
{
    return bridge_on && content_ours();
}

bool tt_bridge_presence_id(runtime::presence_id::Id &out)
{
    std::lock_guard<std::mutex> g(lock);
    if (!presence_known)
        return false;
    out = presence_now;
    return true;
}

bool tt_bridge_ghost_id(uint8_t out[48])
{
    std::lock_guard<std::mutex> g(lock);
    if (!have_ghost_block)
        return false;
    memcpy(out, ghost_block, sizeof ghost_block);
    return true;
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial boards bridge", setup);
}
