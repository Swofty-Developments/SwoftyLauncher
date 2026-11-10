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
#include <cstdint>
#include <cstring>
#include <deque>
#include <map>
#include <memory>
#include <mutex>
#include <string>
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
safetyhook::MidHook h_finder, h_ts_flag, h_priv, h_select, h_write_token, h_reader_fail, h_xml, h_push, h_upload, h_download, h_racenet, h_pb, h_session_enter, h_account_error;

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

void handler_watch(uint64_t handler, const std::string &board)
{
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
