#include "tt_bridge_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "apply/content/lower.hpp"
#include "apply/ids.hpp"
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
const runtime::Sig S_GET_TT{"Game_F1os_OnResult_GetTTLeaderboard",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 28 01 00 00 48 89 D7 48 89 CE E8 ?? ?? ?? ?? 4C 8B A7 "
        "A0 00 00 00 8B AF 9C 00 00 00 44 0F B6 B7 98 00 00 00 48 8B 5E 08 48 8B 03 BA A8 00 00 00 41 B8 10 "
        "00 00 00 48 89 D9 FF 50 18 49 89 C7 48 85 C0 74 16 4C 89 64 24 20 4C 89 F9 48 89 DA 45 89 F0 41 89 "
        "E9 E8 ?? ?? ?? ?? 4C 8B B6 A8 01 00 00 4C 89 BE A8 01 00 00 4D 85 F6 74 20 4C 8B BE A0 01 00 00 4D "
        "85 FF 74 14 4C 89 F1 E8 ?? ?? ?? ?? 49 8B 07 4C 89 F9 4C 89 F2 FF 50 38 48 89 9E A0 01 00 00 48 8D "
        "47 08 80 BF 88 00 00 00 00 48 8D 0D EB 0F C9 03 ",
        0x3021580};
const runtime::Sig S_GET_FRIENDS{"Game_F1os_OnResult_GetFriendsTTLeaderboard",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 28 01 00 00 48 89 D7 48 89 CE E8 ?? ?? ?? ?? 4C 8B A7 "
        "A0 00 00 00 8B AF 9C 00 00 00 44 0F B6 B7 98 00 00 00 48 8B 5E 08 48 8B 03 BA A8 00 00 00 41 B8 10 "
        "00 00 00 48 89 D9 FF 50 18 49 89 C7 48 85 C0 74 16 4C 89 64 24 20 4C 89 F9 48 89 DA 45 89 F0 41 89 "
        "E9 E8 ?? ?? ?? ?? 4C 8B B6 A8 01 00 00 4C 89 BE A8 01 00 00 4D 85 F6 74 20 4C 8B BE A0 01 00 00 4D "
        "85 FF 74 14 4C 89 F1 E8 ?? ?? ?? ?? 49 8B 07 4C 89 F9 4C 89 F2 FF 50 38 48 89 9E A0 01 00 00 48 8D "
        "47 08 80 BF 88 00 00 00 00 48 8D 0D 8B B2 C9 03 ",
        0x30172e0};
const runtime::Sig S_GET_BINARY{"Game_F1os_OnResult_GetBinaryDataForUser",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 18 01 00 00 48 89 D7 48 89 CE E8 ?? ?? ?? ?? 4C 8B A7 "
        "A0 00 00 00 8B AF 9C 00 00 00 44 0F B6 B7 98 00 00 00 48 8B 5E 08 48 8B 03 BA A8 00 00 00 41 B8 10 "
        "00 00 00 48 89 D9 FF 50 18 49 89 C7 48 85 C0 74 16 4C 89 64 24 20 4C 89 F9 48 89 DA 45 89 F0 41 89 "
        "E9 E8 ?? ?? ?? ?? 4C 8B B6 A8 01 00 00 4C 89 BE A8 01 00 00 4D 85 F6 74 20 4C 8B BE A0 01 00 00 4D "
        "85 FF 74 14 4C 89 F1 E8 ?? ?? ?? ?? 49 8B 07 4C 89 F9 4C 89 F2 FF 50 38 48 89 9E A0 01 00 00 48 8D "
        "47 08 80 BF 88 00 00 00 00 48 8D 0D EB 40 CB 03 ",
        0x2ffe480};
const runtime::Sig S_ADD_ENTRY{"Game_F1os_OnResult_AddLeaderboardEntry",
        "41 57 41 56 41 54 56 57 55 53 48 81 EC 00 01 00 00 48 89 D7 48 89 CE E8 ?? ?? ?? ?? 4C 8B A7 A0 00 "
        "00 00 8B AF 9C 00 00 00 44 0F B6 B7 98 00 00 00 48 8B 5E 08 48 8B 03 BA A8 00 00 00 41 B8 10 00 00 "
        "00 48 89 D9 FF 50 18 49 89 C7 48 85 C0 74 16 4C 89 64 24 20 4C 89 F9 48 89 DA 45 89 F0 41 89 E9 E8 "
        "?? ?? ?? ?? 4C 8B B6 A8 01 00 00 4C 89 BE A8 01 00 00 4D 85 F6 74 20 4C 8B BE A0 01 00 00 4D 85 FF "
        "74 14 4C 89 F1 E8 ?? ?? ?? ?? 49 8B 07 4C 89 F9 4C 89 F2 FF 50 38 48 89 9E A0 01 00 00 48 8D 47 08 "
        "80 BF 88 00 00 00 00 48 8D 0D 6D 46 CA 03 ",
        0x300df00};
const runtime::Sig S_UPDATE_BINARY{"Game_F1os_OnResult_UpdateLeaderboardEntryBinaryData",
        "41 57 41 56 41 54 56 57 55 53 48 81 EC 00 01 00 00 48 89 D7 48 89 CE E8 ?? ?? ?? ?? 4C 8B A7 A0 00 "
        "00 00 8B AF 9C 00 00 00 44 0F B6 B7 98 00 00 00 48 8B 5E 08 48 8B 03 BA A8 00 00 00 41 B8 10 00 00 "
        "00 48 89 D9 FF 50 18 49 89 C7 48 85 C0 74 16 4C 89 64 24 20 4C 89 F9 48 89 DA 45 89 F0 41 89 E9 E8 "
        "?? ?? ?? ?? 4C 8B B6 A8 01 00 00 4C 89 BE A8 01 00 00 4D 85 F6 74 20 4C 8B BE A0 01 00 00 4D 85 FF "
        "74 14 4C 89 F1 E8 ?? ?? ?? ?? 49 8B 07 4C 89 F9 4C 89 F2 FF 50 38 48 89 9E A0 01 00 00 48 8D 47 08 "
        "80 BF 88 00 00 00 00 48 8D 0D 9D B8 CA 03 ",
        0x3006cd0};

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
constexpr const char *GARAGE_TABS_PATTERN = "80 79 18 00 0F 85 ?? ?? ?? ?? 48 B9 00 00 00 00 20 00 00 00 48 23 08 0F 84 ?? ?? ?? ??";
const runtime::Sig S_GARAGE_TAB_BLOCK{"site_Game_GarageMonitorPanel_BuildTabs_leaderboard_block_141f720ab", GARAGE_TABS_PATTERN, 0x1f720ab, runtime::Sig::addr, 0, 0};
const runtime::Sig S_GARAGE_TAB_FLAGS{"site_Game_GarageMonitorPanel_BuildTabs_leaderboard_feature_141f720b5", "48 B9 00 00 00 00 20 00 00 00 48 23 08 0F 84 ?? ?? ?? ??", 0x1f720b5};
const runtime::Sig S_GARAGE_TAB_GATE{"site_Game_GarageMonitorPanel_BuildTabs_gate_byte_141f7208e", "80 B9 80 00 00 00 00 0F 84 ?? ?? ?? ?? 83 FD 12 0F 85 ?? ?? ?? ??", 0x1f7208e};
const runtime::Sig S_REPLAY_TICK{"Ne_ReplaySystem_ResetSlot24_1445d57e0", "56 48 83 EC 20 80 B9 89 00 00 00 00 0F 84 90 00 00 00 48 89 CE 80 3D", 0x45d57e0};
const runtime::Sig S_REPLAY_TICK17{"Ne_ReplaySystem_ResetSlot17_1445d5b90", "56 48 83 EC 20 80 B9 89 00 00 00 00 74 5F 48 89 CE 80 3D 1C 5F A7 01 01 75 6C", 0x45d5b90};
const runtime::Sig S_REPLAY_UPDATE{"Game_ReplaySystem_UpdateFromModule_1445d6040",
        "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC D8 00 00 00 C5 78 29 AC 24 C0 00 00 00 C5 78 29 A4 24 B0 00 00 00 C5 78 29", 0x45d6040};
const runtime::Sig S_REPLAY_FRAME{"Game_ReplayBuffer_BeginFrameWrite_1445c4b20", "41 56 56 57 53 48 83 EC 68 C5 F8 29 74 24 50 C5 F9 6F F1 48 89 CE 80 79 51 00 74 30", 0x45c4b20};
const runtime::Sig S_REPLAY_SAVE{"Game_ReplayManager_SaveFullReplay_142c46880", "41 56 56 57 53 48 81 EC 38 02 00 00 80 B9 E1 06 00 00 00 74 46 48 89 CE 48 83 B9 50 01 00 00 00", 0x2c46880};
const runtime::Sig S_REPLAY_JOB{"Game_Replay_SaveFullReplay_job_142c46a00", "41 57 41 56 41 55 41 54 56 57 55 53 B8 08 14 00 00 E8 BA 2D 04 02 48 29", 0x2c46a00};
const runtime::Sig S_REPLAY_JOB_COMPONENT{"site_Game_Replay_SaveFullReplay_session_component_142c46c02",
        "48 8B 46 28 48 8B 80 88 00 00 00 48 85 C0 0F 84 ?? ?? ?? ?? 48 8B 80 60 02 00 00 48 89 84 24 98 00 00 00 48 85 C0 0F 84 ?? ?? ?? ?? 48 89 4C 24 68 48 8B 84 24 98 00 00 00 4C 8B B0 08 0F 00 00 49 8D 4E 18",
        0x2c46c02, runtime::Sig::addr, 0, 64};
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
const runtime::Sig S_GARAGE_PRIV_SUB{"Game_GarageMonitorPanel_OnTtLeaderboardPrivilege_thunk_141f79690", "E9 9B 42 6A 0A", 0x1f79690};
const runtime::Sig S_PRIV_GATE{"site_Game_UIUIPrivilegeCheckControl_HandleSignal_Result",
        "41 80 BE AB 00 00 00 00 0F 84 8C 00 00 00 45 84 FF 0F 84 8D 00 00 00", 0x3822cdf, runtime::Sig::addr, 0, 8};
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
std::atomic<bool> session_custom{false}, track_watch{false};
std::atomic<int64_t> track_checked_ms{0};
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
safetyhook::MidHook h_garage_done, h_garage_cmd, h_garage_update, h_tab_block, h_tab_flags, h_tab_friends, h_tab_build, h_tab_gate, h_replay_tick, h_replay_tick17, h_replay_update, h_replay_frame, h_replay_save, h_replay_job, h_job_component, h_job_selected, h_job_stores, h_job_alloc, h_finder, h_ts_flag, h_priv, h_select, h_write_token, h_reader_fail, h_rearm, h_trip, h_save_pb, h_reset, h_entry_write, h_xml, h_push, h_upload, h_download, h_racenet, h_pb, h_session_enter, h_account_error;

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
    bool quiet = bridge_on && (state == 2 || state == 8 || state == 9 || state == 10);
    if (diag_on && ++diag_error_logged <= 16)
        swofty::log("[boards] diag: account-state error handler: state %u %s%s%s; backtrace %s", state, tb::gate::account_state_name(state),
                    state == 9 ? " (the Platform Client Overlay popup)" : "", quiet ? "; no popup: the offline launcher's content does not need the account service" : "",
                    backtrace_text(1, 6).c_str());
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
        uint64_t base = swofty::api->module_base(), offs = peek64(base + 0x1461089f8 - 0x140000000);
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
    c.rip = swofty::api->module_base() + 0x1f726ea;
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
constexpr uint64_t GAME_REPLAY_SYSTEM_VTABLE_RVA = 0x1457cf220 - 0x140000000;
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

void replay_watch(std::wstring path, uint32_t lap_ms, uint64_t stamp)
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
        bool copied = CopyFileW(path.c_str(), dest.c_str(), FALSE) != 0;
        swofty::log("[replay] capture: game save %s settled at %llu bytes; header %s; magic 0xe32fcaa8 %s; copy to tt/replays/%s %s", narrow(path.c_str()).c_str(), (unsigned long long)size,
                    hex_dump(head, got).c_str(), magic >= 0 ? ("found at +" + std::to_string(magic)).c_str() : "NOT found", name.c_str(), copied ? "done" : "FAILED");
        return;
    }
    swofty::log("[replay] capture: the game's save %s did not appear within 180 s", narrow(path.c_str()).c_str());
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
        if (runtime::readable(reinterpret_cast<void *>(sys), 8) && peek64(sys) == swofty::api->module_base() + GAME_REPLAY_SYSTEM_VTABLE_RVA && replay_manager_ok(c.rcx, sys)) {
            replay_manager_ptr = c.rcx;
            replay_system_ptr = sys;
        }
    }
    if (++logged <= 6)
        swofty::log("[replay] the game started ReplayManager::SaveFullReplay on 0x%llx, path %s", (unsigned long long)c.rcx,
                    runtime::readable(reinterpret_cast<void *>(c.rdx), 2) ? narrow(reinterpret_cast<const wchar_t *>(c.rdx)).c_str() : "?");
}

std::atomic<int64_t> replay_session_best_ms{0};
bool replay_capture_any = false;

// Called from the lap recorder (flow thread) after a lap was written: a valid lap faster than the session's best marks a capture pending.
void replay_lap_finished(int64_t lap_ms, int valid)
{
    if (!replay_capture_on || !bridge_on)
        return;
    int64_t best = replay_session_best_ms.load();
    // SWOFTY_TT_REPLAY_CAPTURE=any is a test switch: every finished lap is saved, valid or not.
    bool mark = tb::replay_lap_is_best(lap_ms, valid, best) || (replay_capture_any && lap_ms > 0);
    swofty::log("[replay] capture: lap %lld ms (valid %d), session best %lld ms: %s", (long long)lap_ms, valid, (long long)best, mark ? "marked for a replay save" : "not a new best, no save");
    if (!mark)
        return;
    replay_session_best_ms = lap_ms;
    replay_pending_lap = uint32_t(lap_ms);
    replay_pending_ms = now_ms();
}

// `sys` is the candidate replay system. The per-frame ReplayBuffer::beginFrameWrite hook has no replay system in a register (rcx is the
// buffer), so it passes 0 and the last GameReplaySystem a hook verified is used; the vtable word is checked again on every use.
void replay_tick(uint64_t sys, const char *slot)
{
    static std::atomic<uint64_t> entries[4]{}, ours_entries[4]{};
    static std::atomic<int> guard_logs{0};
    int which = !strcmp(slot, "vslot24") ? 0 : !strcmp(slot, "vslot17") ? 1 : !strcmp(slot, "updateFromModule") ? 2 : 3;
    uint64_t n = ++entries[which];
    uintptr_t base = swofty::api->module_base();
    uint64_t known = replay_system_ptr.load();
    if (!sys)
        sys = known;
    bool vt_ok = sys && runtime::readable(reinterpret_cast<void *>(sys), 8) && peek64(sys) == base + GAME_REPLAY_SYSTEM_VTABLE_RVA;
    if (vt_ok) {
        ours_entries[which]++;
        replay_system_ptr = sys;
    }
    if (n <= 5 || n % 20000 == 0)
        swofty::log("[replay] tick %s entry %llu (replay system 0x%llx, GameReplaySystem vtable %s, %llu such entries); pending mark %lld", slot, (unsigned long long)n, (unsigned long long)sys,
                    vt_ok ? "yes" : "no", (unsigned long long)ours_entries[which].load(), (long long)replay_pending_ms.load());
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
    bool allowed = mgr != 0 && tb::replay_capture_allowed(replay_capture_on, bridge_on, content_ours(), vt_ok, rec == 0xff ? 0 : rec, queue);
    if (++guard_logs <= 8)
        swofty::log("[replay] capture guards at tick %s: switch %d, bridge %d, our content %d, GameReplaySystem %d, manager 0x%llx, recording flag [mgr+0x6e1] %u, job queue [mgr+0x150] 0x%llx (busy word [queue+0x48] %u) -> %s",
                    slot, replay_capture_on ? 1 : 0, bridge_on ? 1 : 0, content_ours() ? 1 : 0, vt_ok ? 1 : 0, (unsigned long long)mgr, unsigned(rec), (unsigned long long)queue,
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
void replay_update_mid(safetyhook::Context &c) { replay_tick(c.rcx, "updateFromModule"); }
// Finds a GameReplaySystem reachable from the replay buffer by plain reads: every qword of the buffer's first 0x200 bytes that is a readable
// user-space pointer is checked for the system's vtable word, and so is every qword of the first 0x100 bytes behind such a pointer. Only
// readable() guarded 8-byte reads; no calls. Runs once per buffer object.
uint64_t replay_find_system(uint64_t buffer, std::string &how)
{
    uintptr_t want = swofty::api->module_base() + GAME_REPLAY_SYSTEM_VTABLE_RVA;
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

void setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    bridge_on = gate.is_object() && gate["tt_bridge"].as_bool(false);
    boards_on = gate.is_object() && gate["tt_boards"].as_bool(false);
    if (!runtime::same_build()) {
        swofty::log("[boards] off: the hook points are this build's byte signatures; this exe is a different build");
        return;
    }
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
        replay_capture_on = rn && rn < sizeof rv && (!strcmp(rv, "1") || !strcmp(rv, "any"));
        replay_capture_any = rn && rn < sizeof rv && !strcmp(rv, "any");
    }
    track_watch = bridge_on || guards_on || diag_on;
    table = tb::Table(unix_seconds() * 100);
    make_dirs();

    found.on_result[int(tb::Method::get_tt)] = runtime::find(S_GET_TT, "boards", "boards");
    found.on_result[int(tb::Method::get_friends_tt)] = runtime::find(S_GET_FRIENDS, "boards", "boards");
    found.on_result[int(tb::Method::get_binary)] = runtime::find(S_GET_BINARY, "boards", "boards");
    found.on_result[int(tb::Method::add_entry)] = runtime::find(S_ADD_ENTRY, "boards", "boards");
    found.on_result[int(tb::Method::update_binary)] = runtime::find(S_UPDATE_BINARY, "boards", "boards");
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
            if (uintptr_t at = runtime::find(S_GARAGE_FRIENDS_TAB, "boards", "boards"))
                if (hook_mid(h_tab_friends, at, garage_friends_tab_mid, "garage friends tab"))
                    swofty::log("[boards] garage friends tab skipped for our content at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
        if (bridge_on && replay_capture_on) {
            uintptr_t path_fn = runtime::find(S_REPLAY_PATH, "boards", "boards"), save_fn = runtime::find(S_REPLAY_SAVE, "boards", "boards"), tick = runtime::find(S_REPLAY_TICK, "boards", "boards");
            if (path_fn && save_fn && tick && hook_mid(h_replay_tick, tick, replay_tick_mid, "replay system tick")) {
                replay_build_path_fn = path_fn;
                replay_save_fn = save_fn;
                hook_mid(h_replay_save, save_fn, replay_save_entry_mid, "replay save observer");
                if (uintptr_t j = runtime::find(S_REPLAY_JOB, "boards", "boards"))
                    hook_mid(h_replay_job, j, replay_job_mid, "replay save job observer");
                for (auto [sig, hook, fn, label] : {std::tuple<const runtime::Sig *, safetyhook::MidHook *, safetyhook::MidHookFn, const char *>{&S_REPLAY_JOB_COMPONENT, &h_job_component, replay_job_component_mid, "save job check 1"},
                                                    {&S_REPLAY_JOB_SELECTED, &h_job_selected, replay_job_selected_mid, "save job check 2"},
                                                    {&S_REPLAY_JOB_STORES, &h_job_stores, replay_job_stores_mid, "save job check 3"},
                                                    {&S_REPLAY_JOB_ALLOC, &h_job_alloc, replay_job_alloc_mid, "save job check 4"}})
                    if (uintptr_t at = runtime::find(*sig, "boards", "boards"))
                        hook_mid(*hook, at, fn, label);
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

namespace runtime {

void tt_bridge_tick()
{
    if (diag_on || bridge_on)
        diag_tick();
    if (track_watch.load(std::memory_order_relaxed)) {
        int64_t t = now_ms();
        if (t - track_checked_ms.load() >= 500) {
            track_checked_ms = t;
            std::string code = runtime::lower(direct_start::tt::session_track());
            session_custom = tb::board_track(swofty_core::apply::tracks::base(), code).custom;
            std::lock_guard<std::mutex> g(lock);
            session_code = std::move(code);
        }
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
