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
//    site_Game_LeaderboardXml_Loaded the buffer pointer (rax) and its byte length ([rsp+0x70]) are replaced by a longer
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
// Every hook point is found by byte scan, gated on the KB build. SWOFTY_TT_UPLOAD_GUARD=0, SWOFTY_TT_DOWNLOAD_GUARD=0,
// SWOFTY_TT_RACENET_GUARD=0 and SWOFTY_TT_REQUEST_GUARD=0 turn the guards off.

namespace {

namespace tb = runtime::tt_bridge;

const runtime::Sig S_XML{"site_Game_LeaderboardXml_Loaded", "48 8B 40 58 48 89 44 24 78 48 8B 4C 24 70", 0x30daa2a,
                         runtime::Sig::addr, 0, 4};
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
std::string last_board;
std::atomic<int> pending_count{0};
std::deque<std::unique_ptr<tb::Response>> responses;
std::deque<std::string> xml_buffers;
uint8_t ghost_block[tb::ID_BLOCK] = {};
bool have_ghost_block = false;
std::wstring outbox, inbox;
std::atomic<int> logged_requests{0}, logged_unreadable{0}, logged_passes{0}, xml_logged{0}, pb_logged{0};
std::atomic<int> hits_upload{0}, hits_download{0}, hits_racenet{0};
std::vector<std::string> seen_methods;

safetyhook::MidHook h_xml, h_push, h_upload, h_download, h_racenet, h_pb;

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

void xml_mid(safetyhook::Context &c)
{
    if (!bridge_on)
        return;
    uint64_t len = 0;
    uint64_t at_len = c.rsp + 0x70;
    if (!runtime::readable(reinterpret_cast<void *>(at_len), 8))
        return;
    memcpy(&len, reinterpret_cast<void *>(at_len), 8);
    int n = ++xml_logged;
    if (len < 16 || len > (size_t(32) << 20) || !runtime::readable(reinterpret_cast<void *>(c.rax), len)) {
        swofty::log("[boards] board definitions: the buffer at the load site is not readable (rax 0x%llx, length %llu); left alone",
                    (unsigned long long)c.rax, (unsigned long long)len);
        return;
    }
    std::string text(reinterpret_cast<const char *>(c.rax), size_t(len));
    std::vector<int> descriptors;
    size_t dropped = 0;
    for (int64_t id : swofty_core::apply::ids::store().ids_of(swofty_core::apply::content::LEADERBOARD_IDS)) {
        if (id >= tb::FIRST_ADDON_DESCRIPTOR && id <= tb::LAST_NAMED_DESCRIPTOR)
            descriptors.push_back(int(id));
        else if (id > tb::LAST_NAMED_DESCRIPTOR)
            dropped++;
    }
    tb::XmlReport rep;
    std::string out = tb::extend_xml(text, descriptors, rep);
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
        swofty::log("[boards] board definitions: left alone (%s); %zu addon descriptor(s), %zu stock track(s)%s",
                    rep.why.c_str(), rep.descriptors, rep.tracks, dropped ? ", descriptors above 99 have no name form" : "");
        return;
    }
    xml_buffers.push_back(std::move(out));
    if (xml_buffers.size() > 4)
        xml_buffers.pop_front();
    const std::string &keep = xml_buffers.back();
    uint64_t new_len = keep.size();
    memcpy(reinterpret_cast<void *>(at_len), &new_len, 8);
    c.rax = reinterpret_cast<uint64_t>(keep.c_str());
    swofty::log("[boards] board definitions: appended %zu board(s) for %zu addon descriptor(s) over %zu track(s) (ids %lld..%lld, %zu already "
                "defined%s), buffer %zu -> %zu bytes",
                rep.added, rep.descriptors, rep.tracks, (long long)rep.first_id, (long long)rep.last_id, rep.existing,
                rep.capped ? ", capped" : "", text.size(), keep.size());
}

void log_unreadable(uint64_t op, tb::Method m, const std::string &name)
{
    if (++logged_unreadable > 4)
        return;
    uint8_t raw[0x30] = {};
    uint64_t base = op + tb::ARGS_AT_OP + tb::board_slot(m);
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
    {
        std::lock_guard<std::mutex> g(lock);
        last_board = a.board_text;
    }
    tb::Route route = tb::route_of(m, a.board, bridge_on);
    if (route == tb::Route::local && !request_guard)
        route = tb::Route::pass;
    if (route == tb::Route::pass || !found.on_result[int(m)]) {
        if (route != tb::Route::pass)
            swofty::log("[boards] request %s %s: off, needs OnResult of the method (signature did not resolve); passed through", name.c_str(),
                        a.board_text.c_str());
        else if (++logged_passes <= 3)
            swofty::log("[boards] request %s %s passes (%s board, string form %s)", name.c_str(), a.board_text.c_str(),
                        a.board.addon() ? "addon" : "stock", tb::form_name(a.form));
        return;
    }
    uint64_t seq;
    {
        std::lock_guard<std::mutex> g(lock);
        seq = table.add(op, m, a, now_ms(), route == tb::Route::local);
        pending_count = int(table.size());
    }
    c.rax = 0;
    c.trampoline_rsp = c.rsp;
    if (++logged_requests <= 24)
        swofty::log("[boards] request %s %s redirected (seq %llu, %s, string form %s)", name.c_str(), a.board_text.c_str(),
                    (unsigned long long)seq, route == tb::Route::local ? "answered locally" : "to the mailbox", tb::form_name(a.form));
}

void guard_entry(safetyhook::Context &c, const char *what, std::atomic<int> &hits)
{
    std::string content = current_content();
    if (!tb::addon_content(content))
        return;
    c.rax = 0;
    c.trampoline_rsp = c.rsp;
    tb::Say say = tb::say(++hits);
    if (say != tb::Say::none)
        swofty::log("[boards] guard: %s skipped for addon content %s%s", what, content.c_str(),
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

    uintptr_t push = (bridge_on || request_guard) ? runtime::find(S_PUSH, "boards", "boards") : 0;
    if (push) {
        push_hooked = hook_mid(h_push, push, push_mid, "request redirect");
        if (push_hooked)
            swofty::log("[boards] request redirect armed at base+0x%llx (%s)", (unsigned long long)(push - swofty::api->module_base()),
                        bridge_on ? "addon board requests go to the mailbox" : "addon board writes answered locally");
    } else if (bridge_on || request_guard)
        swofty::log("[boards] request redirect: off, needs %s (byte signature matched nothing unique)", S_PUSH.what);

    if (bridge_on) {
        uintptr_t xml = runtime::find(S_XML, "boards", "boards");
        if (!xml)
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

void write_request(tb::Pending &p, const std::string &content, const std::string &track, const std::string &ghost_b64)
{
    std::string json = tb::request_json(p.seq, p.method, p.args, content, track, p.board, p.board_text, ghost_b64);
    std::wstring path = outbox + runtime::wide(std::to_string(p.seq) + ".json");
    p.sent = true;
    if (runtime::write_file(path, json))
        swofty::log("[boards] request %llu %s %s written to the mailbox (%zu bytes)", (unsigned long long)p.seq, tb::method_name(p.method),
                    p.board_text.c_str(), json.size());
    else
        swofty::log("[boards] request %llu: writing the outbox file failed (error %lu); it will time out", (unsigned long long)p.seq, GetLastError());
}

void complete(const tb::Completion &c)
{
    auto resp = std::make_unique<tb::Response>();
    resp->fill(c.status, c.body);
    uintptr_t fn = found.on_result[int(c.method)];
    if (!fn || !runtime::readable(reinterpret_cast<void *>(c.op), 0x218)) {
        swofty::log("[boards] completion %llu %s: dropped (%s)", (unsigned long long)c.seq, tb::method_name(c.method),
                    fn ? "the op is no longer readable" : "no OnResult");
        return;
    }
    reinterpret_cast<OnResult_t>(fn)(reinterpret_cast<void *>(c.op), resp->raw);
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
            track = runtime::lower(direct_start::tt::current_track());
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
