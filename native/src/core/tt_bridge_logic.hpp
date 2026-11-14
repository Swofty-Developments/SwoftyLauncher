#pragma once

#include "apply/facts.hpp"
#include "apply/tracks/base.hpp"

#include <swofty/json.hpp>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <vector>

namespace runtime::tt_bridge {

namespace json = swofty::json;

constexpr int64_t TIMEOUT_MS = 8000;
constexpr size_t MAX_PENDING = 32;
constexpr int FIRST_ADDON_DESCRIPTOR = 6, LAST_NAMED_DESCRIPTOR = 99;
constexpr size_t MAX_XML_BOARDS = 6000;
constexpr int MAX_TRACK_NUMBER = 999;
constexpr size_t ID_BLOCK = 0x30;

enum class Say { none, line, last };

inline Say say(int occurrence, int first = 6)
{
    if (occurrence < 1 || occurrence > first)
        return Say::none;
    return occurrence == first ? Say::last : Say::line;
}

inline bool switch_off(const char *value)
{
    if (!value)
        return false;
    return (value[0] == '0' && !value[1]) || !strcmp(value, "off");
}

// Board names

struct BoardName {
    bool ok = false;
    int descriptor = -1, track = -1;
    char weather = 0;
    bool addon() const { return ok && descriptor >= FIRST_ADDON_DESCRIPTOR; }
};

inline BoardName parse_board(std::string_view s)
{
    BoardName b;
    if (s.size() < 8 || s.size() > 12 || s.substr(0, 3) != "LBV")
        return b;
    char w = s.back();
    if (w != 'D' && w != 'W')
        return b;
    auto digits = [&](size_t from, size_t to, int &out) {
        if (to <= from)
            return false;
        out = 0;
        for (size_t i = from; i < to; i++) {
            if (s[i] < '0' || s[i] > '9')
                return false;
            out = out * 10 + (s[i] - '0');
        }
        return true;
    };
    if (!digits(3, 5, b.descriptor) || !digits(5, s.size() - 1, b.track) || s.size() - 1 - 5 < 2)
        return b = BoardName{};
    b.weather = w;
    b.ok = true;
    return b;
}

inline std::string board_name(int descriptor, int track, char weather)
{
    char b[24];
    snprintf(b, sizeof b, "LBV%02d%02d%c", descriptor, track, weather);
    return b;
}

inline const char *weather_word(char w) { return w == 'W' ? "wet" : "dry"; }

// Methods

enum class Method { none, get_tt, get_friends_tt, get_binary, add_entry, update_binary };

inline const char *method_name(Method m)
{
    switch (m) {
    case Method::get_tt: return "GetTTLeaderboard";
    case Method::get_friends_tt: return "GetFriendsTTLeaderboard";
    case Method::get_binary: return "GetBinaryDataForUser";
    case Method::add_entry: return "AddLeaderboardEntry";
    case Method::update_binary: return "UpdateLeaderboardEntryBinaryData";
    case Method::none: break;
    }
    return "";
}

inline Method method_of(std::string_view name)
{
    for (Method m : {Method::get_tt, Method::get_friends_tt, Method::get_binary, Method::add_entry, Method::update_binary})
        if (name == method_name(m))
            return m;
    return Method::none;
}

enum class Kind { str, i32, i64 };

struct Field {
    const char *key;
    uint32_t offset;
    Kind kind;
};

// op+0x128 holds a pointer to the request block (Game_F1os_Create_*: puVar7[0x25]); the fields are relative to the block.
inline constexpr uint32_t ARGS_AT_OP = 0x128;
inline constexpr uint32_t OP_SERVICE = 0x110, OP_METHOD = 0x118, OP_ARGS_TEXT = 0x168;

inline const std::vector<Field> &fields_of(Method m)
{
    static const std::vector<Field> get_tt = {{"LeaderboardID", 8, Kind::str}, {"Version", 0x30, Kind::i32},
                                              {"RangeType", 0x34, Kind::i32},  {"FromIndex", 0x38, Kind::i64},
                                              {"ToIndex", 0x40, Kind::i64},    {"TopResultCount", 0x48, Kind::i32},
                                              {"CrossPlay", 0x4c, Kind::i32}};
    static const std::vector<Field> friends = {{"LeaderboardID", 8, Kind::str},
                                               {"Version", 0x30, Kind::i32},
                                               {"CommaSeparatedFriends", 0x38, Kind::str},
                                               {"CommaSeparatedEAFriends", 0x60, Kind::str},
                                               {"RangeType", 0x88, Kind::i32},
                                               {"FromIndex", 0x90, Kind::i64},
                                               {"ToIndex", 0x98, Kind::i64},
                                               {"TopResultCount", 0xa0, Kind::i32},
                                               {"CrossPlay", 0xa4, Kind::i32}};
    static const std::vector<Field> binary = {{"PlatformID", 8, Kind::str},
                                              {"Platform", 0x30, Kind::i32},
                                              {"LeaderboardID", 0x48, Kind::str},
                                              {"Version", 0x70, Kind::i32}};
    static const std::vector<Field> add = {{"LeaderboardID", 8, Kind::str}, {"Version", 0x30, Kind::i32},
                                           {"Score", 0x38, Kind::i64},      {"Date", 0x40, Kind::i64},
                                           {"BinaryDataHash", 0x70, Kind::str}, {"ResultStatus", 0xb8, Kind::i64},
                                           {"JSONData", 0xc0, Kind::str},   {"CrossPlay", 0xec, Kind::i32}};
    static const std::vector<Field> update = {{"Version", 8, Kind::i32}, {"LeaderboardID", 0x10, Kind::str}};
    static const std::vector<Field> none;
    switch (m) {
    case Method::get_tt: return get_tt;
    case Method::get_friends_tt: return friends;
    case Method::get_binary: return binary;
    case Method::add_entry: return add;
    case Method::update_binary: return update;
    case Method::none: break;
    }
    return none;
}

inline uint32_t board_slot(Method m)
{
    for (auto &f : fields_of(m))
        if (!strcmp(f.key, "LeaderboardID"))
            return f.offset;
    return 0;
}

// Checked memory

struct Mem {
    bool (*fn)(void *ctx, uint64_t addr, void *out, size_t n) = nullptr;
    void *ctx = nullptr;
    bool read(uint64_t addr, void *out, size_t n) const { return fn && addr && fn(ctx, addr, out, n); }
    template <class T> bool get(uint64_t addr, T &v) const { return read(addr, &v, sizeof v); }
};

constexpr size_t MAX_STRING = 96;

inline bool printable(const char *p, size_t n)
{
    for (size_t i = 0; i < n; i++)
        if (uint8_t(p[i]) < 0x20 || uint8_t(p[i]) > 0x7e)
            return false;
    return n > 0;
}

// A game string at an address: the NeStr object (+0 djb2 hash, +4 refcount, +6 24-bit length, chars from +9), or a C
// string, reached through the slot, its second, third or fourth word, or held inline.
enum class Form { none, ne_str, c_str, inline_c };

struct StrForm {
    Form form = Form::none;
    uint32_t word = 0;
    bool ok() const { return form != Form::none; }
};

inline std::string read_c(const Mem &m, uint64_t at)
{
    char buf[MAX_STRING + 1] = {};
    size_t n = 0;
    for (; n < MAX_STRING; n++) {
        char c;
        if (!m.read(at + n, &c, 1))
            return {};
        if (!c)
            break;
        buf[n] = c;
    }
    if (n == MAX_STRING || !printable(buf, n))
        return {};
    return std::string(buf, n);
}

inline std::string read_ne(const Mem &m, uint64_t p)
{
    uint8_t h[9];
    if (!p || !m.read(p, h, sizeof h))
        return {};
    size_t len = size_t(h[6]) | size_t(h[7]) << 8 | size_t(h[8]) << 16;
    if (len == 0 || len >= MAX_STRING)
        return {};
    std::string s(len, '\0');
    if (!m.read(p + 9, s.data(), len) || !printable(s.data(), len))
        return {};
    return s;
}

inline std::string read_with(const Mem &m, uint64_t slot, StrForm f)
{
    if (!f.ok())
        return {};
    uint64_t at = slot + f.word * 8;
    if (f.form == Form::inline_c)
        return read_c(m, at);
    uint64_t p = 0;
    if (!m.get(at, p))
        return {};
    return f.form == Form::ne_str ? read_ne(m, p) : read_c(m, p);
}

template <class Accept>
inline StrForm probe_form(const Mem &m, uint64_t slot, Accept accept, std::string *value = nullptr)
{
    for (Form form : {Form::ne_str, Form::c_str, Form::inline_c})
        for (uint32_t word = 0; word < 4; word++) {
            StrForm f{form, word};
            std::string s = read_with(m, slot, f);
            if (!s.empty() && accept(s)) {
                if (value)
                    *value = s;
                return f;
            }
        }
    return {};
}

inline const char *form_name(StrForm f)
{
    static char b[32];
    const char *k = f.form == Form::ne_str ? "NeStr*" : f.form == Form::c_str ? "char*" : f.form == Form::inline_c ? "inline" : "none";
    snprintf(b, sizeof b, "%s@word%u", k, f.word);
    return b;
}

struct Args {
    bool ok = false;
    Method method = Method::none;
    std::string board_text;
    BoardName board;
    StrForm form;
    json::Value values = json::Value::object();
};

inline Args decode_args(const Mem &m, uint64_t op, Method method)
{
    Args a;
    a.method = method;
    uint32_t slot = board_slot(method);
    if (!slot)
        return a;
    uint64_t base = 0;
    if (!m.get(op + ARGS_AT_OP, base) || !base)
        return a;
    a.form = probe_form(m, base + slot, [](const std::string &s) { return parse_board(s).ok; }, &a.board_text);
    if (!a.form.ok())
        return a;
    a.board = parse_board(a.board_text);
    for (auto &f : fields_of(method)) {
        uint64_t at = base + f.offset;
        if (f.kind == Kind::str) {
            std::string s = read_with(m, at, a.form);
            if (!s.empty())
                a.values[f.key] = json::Value(s);
        } else if (f.kind == Kind::i32) {
            int32_t v;
            if (m.get(at, v))
                a.values[f.key] = json::Value(int64_t(v));
        } else {
            int64_t v;
            if (m.get(at, v))
                a.values[f.key] = json::Value(v);
        }
    }
    a.ok = true;
    return a;
}

inline Method method_at_op(const Mem &m, uint64_t op, std::string *name = nullptr)
{
    StrForm f = probe_form(m, op + OP_METHOD, [](const std::string &s) { return method_of(s) != Method::none; }, name);
    if (f.ok())
        return method_of(*name);
    f = probe_form(m, op + OP_METHOD, [](const std::string &s) { return s.size() >= 3; }, name);
    return Method::none;
}

// Every F1os op is built by one template, so its method name has one string form. Once a Time Trial method was read in a form
// (`known`), a request is told by one string read in that form: a name that is not a Time Trial method passes without the probe of
// the twelve forms. A read that gives no string probes as method_at_op does. The name of a request that is not a Time Trial
// method is only looked for when `name` is given.
inline Method method_with_form(const Mem &m, uint64_t op, StrForm &known, std::string *name)
{
    if (known.ok()) {
        std::string s = read_with(m, op + OP_METHOD, known);
        if (!s.empty()) {
            Method got = method_of(s);
            if (name)
                *name = std::move(s);
            return got;
        }
    }
    std::string s;
    StrForm f = probe_form(m, op + OP_METHOD, [](const std::string &v) { return method_of(v) != Method::none; }, &s);
    if (f.ok()) {
        known = f;
        Method got = method_of(s);
        if (name)
            *name = std::move(s);
        return got;
    }
    if (name)
        probe_form(m, op + OP_METHOD, [](const std::string &v) { return v.size() >= 3; }, name);
    return Method::none;
}

inline uint32_t pack_form(StrForm f) { return uint32_t(f.form) << 8 | (f.word & 0xff); }
inline StrForm unpack_form(uint32_t v) { return {Form(v >> 8 & 0xff), v & 0xff}; }

// Board track keys

struct BoardTrack {
    std::string key, addon, name, kind;
    int stat_id = -1;
    bool custom = false, known = false;
};

inline std::string lower_ascii(std::string s)
{
    for (char &c : s)
        if (c >= 'A' && c <= 'Z')
            c = char(c - 'A' + 'a');
    return s;
}

// The key a board carries for a track: the code for a stock track, `<addon id>:<code>` for a track an addon adds or
// fills. stat_hint picks between rows that share a code. A code the base does not list, or a base that is not ready,
// gives the lowercased code, custom false and known false.
inline BoardTrack board_track(const swofty_core::apply::tracks::Base &base, std::string_view code, int stat_hint = -1)
{
    namespace at = swofty_core::apply::tracks;
    BoardTrack out;
    out.key = lower_ascii(std::string(code));
    if (!base.ready || code.empty())
        return out;
    const at::BaseTrack *row = nullptr;
    std::string lowered = out.key;
    for (const at::BaseTrack &t : base.tracks)
        if (lower_ascii(t.code) == lowered && (!row || (stat_hint >= 0 && t.stat_id == stat_hint && row->stat_id != stat_hint)))
            row = &t;
    if (!row)
        return out;
    out.known = true;
    out.stat_id = row->stat_id;
    out.name = row->name;
    if (row->kind == at::Kind::stock)
        return out;
    out.custom = true;
    out.addon = row->addon;
    out.kind = at::kind_name(row->kind);
    out.key = lower_ascii(row->addon + ":" + row->code);
    return out;
}

// The track a request is about: the board name's track number through the base when the base has that stat id, else the
// session's current track.
inline BoardTrack request_track(const swofty_core::apply::tracks::Base &base, const BoardName &board, std::string_view session_code)
{
    if (board.ok && base.ready)
        if (const auto *t = base.by_stat_id(board.track))
            return board_track(base, t->code, board.track);
    return board_track(base, session_code);
}

// The session's track comes from the game's current-track getter, which costs 100 ms or more per call. The watch runs only while a
// Time Trial session is live: the bridge, the guards or the diagnostics need the track, and the session entered as a Time Trial and
// has not ended. In the front end the last track read is kept.
inline bool track_watch_on(bool bridge_on, bool guards_on, bool diag_on, bool session_live, bool time_trial)
{
    return (bridge_on || guards_on || diag_on) && session_live && time_trial;
}

inline bool track_watch_due(bool on, int64_t now_ms, int64_t checked_ms, int64_t period_ms = 500)
{
    return on && now_ms - checked_ms >= period_ms;
}

// A live session's track does not change: another track or car from the Time Trial pause menu loads the session again, and that
// load enters a new session, which turns the watch off and on and reads the track again. The track is read at the session enter;
// while a read gives no track it is read again twice a second, at most TRACK_READS times per session, then not until the next one.
constexpr int TRACK_READS = 10;

inline bool track_read_due(bool on, bool known, int reads, int64_t now_ms, int64_t checked_ms, int max_reads = TRACK_READS,
                           int64_t period_ms = 500)
{
    return !known && reads < max_reads && track_watch_due(on, now_ms, checked_ms, period_ms);
}

struct TrackReads {
    std::atomic<bool> known{false};
    std::atomic<int> reads{0};
    std::atomic<int64_t> checked_ms{0};

    template <class Read> std::string enter(int64_t now_ms, Read &&read)
    {
        checked_ms = now_ms;
        reads = 1;
        std::string code = read();
        known = !code.empty();
        return code;
    }

    // The flow tick runs on several threads: one claims a due read and the others skip it. The read's number, or 0 without one.
    template <class Read> int tick(bool on, int64_t now_ms, Read &&read, std::string &code)
    {
        int64_t was = checked_ms.load();
        if (!track_read_due(on, known.load(), reads.load(), now_ms, was) || !checked_ms.compare_exchange_strong(was, now_ms))
            return 0;
        int n = ++reads;
        code = read();
        known = !code.empty();
        return n;
    }
};

// The diagnostics tick reads the ghost lap handler out of the storage system and describes it. A fresh guarded read (a VirtualQuery)
// inside the game's heap costs milliseconds on some PCs (about 10 ms per tick on Windows), and the storage system's onSessionEnter runs
// for every session kind. The tick therefore runs on the handler watch's own thread, never on a game thread, every WATCH_PERIOD_MS and
// only while the track watch says a Time Trial session is live, for the bridge or the diagnostics. One thread also keeps one warm
// VirtualQuery cache: a game flow thread came back to the tick only every few seconds, after its cached answers had expired.
constexpr uint32_t WATCH_PERIOD_MS = 500;

inline bool diag_tick_on(bool bridge_on, bool diag_on, bool time_trial_live)
{
    return (bridge_on || diag_on) && time_trial_live;
}

// The handler watch describes the ghost lap handler: its own block (HANDLER_BLOCK bytes, one readability check, copied once) and the
// save checks behind it (a pointer walk of about 40 reads). With SWOFTY_TT_DIAG=1 the full description runs on every diagnostics tick.
// Otherwise it runs when the handler changes, when the handler fields that the watch line prints change (the key), and at most once
// every HANDLER_REFRESH_MS between those, so a change in the save checks alone still reaches the log at a lower rate.
constexpr size_t HANDLER_BLOCK = 0x520;
constexpr int64_t HANDLER_REFRESH_MS = 10000;

struct HandlerField {
    uint16_t at;
    uint8_t size;
};

inline constexpr HandlerField HANDLER_KEY_FIELDS[] = {{0x18, 8},  {0x60, 7},  {0x94, 8},  {0x9c, 4},  {0x130, 8}, {0x138, 4},
                                                      {0x1b0, 4}, {0x1d0, 12}, {0x288, 4}, {0x300, 4}, {0x31c, 1}, {0x508, 8}};

inline uint64_t handler_key(const uint8_t *block, size_t n, int32_t finder_state)
{
    uint64_t h = 0xcbf29ce484222325ull;
    auto mix = [&](uint8_t b) { h = (h ^ b) * 0x100000001b3ull; };
    for (const HandlerField &f : HANDLER_KEY_FIELDS)
        for (size_t i = 0; i < f.size; i++)
            mix(block && f.at + i < n ? block[f.at + i] : 0);
    for (int i = 0; i < 4; i++)
        mix(uint8_t(uint32_t(finder_state) >> (8 * i)));
    return h;
}

struct HandlerWatch {
    uint64_t handler = 0, key = 0;
    int64_t full_ms = 0;
};

inline bool handler_watch_full(bool diag_on, uint64_t handler, uint64_t key, int64_t now_ms, const HandlerWatch &seen, int64_t refresh_ms = HANDLER_REFRESH_MS)
{
    if (!handler)
        return false;
    return diag_on || handler != seen.handler || key != seen.key || now_ms - seen.full_ms >= refresh_ms;
}

// The save checks behind handler+0x18 (the pointer walk) are read again only with SWOFTY_TT_DIAG=1, for a new handler, or once
// SAVE_CHECKS_MS has passed since the last walk. Otherwise the watch line keeps the last walk's text and needs only the block copy.
constexpr int64_t SAVE_CHECKS_MS = 5000;

struct SaveChecks {
    uint64_t handler = 0;
    int64_t read_ms = 0;
};

inline bool save_checks_due(bool diag_on, uint64_t handler, int64_t now_ms, const SaveChecks &seen, int64_t period_ms = SAVE_CHECKS_MS)
{
    if (!handler)
        return false;
    return diag_on || handler != seen.handler || now_ms - seen.read_ms >= period_ms;
}

// The game looks a Time Trial board up by its key ('LBV<descriptor><track><D|W>') in the board list (the registry's begin +0x18
// and end +0x20, 8-byte board pointers, each board's C-string name at +0x68): once at the ghost lap storage system's
// onSessionEnter and again when the leaderboard view selects its board. scan_boards walks that list the same way, with the
// case of the name ignored, and counts the boards that share the key's first five characters (its descriptor) for the log.
constexpr uint64_t BOARD_LIST_BEGIN = 0x18, BOARD_LIST_END = 0x20, BOARD_NAME = 0x68;
constexpr size_t BOARD_LIST_MAX = 100000;

struct BoardScan {
    size_t listed = 0, same_prefix = 0;
    bool found = false;
    std::string sample;
};

inline bool same_board_name(const std::string &a, const std::string &b)
{
    if (a.size() != b.size())
        return false;
    for (size_t i = 0; i < a.size(); i++) {
        char x = a[i] >= 'a' && a[i] <= 'z' ? char(a[i] - 32) : a[i], y = b[i] >= 'a' && b[i] <= 'z' ? char(b[i] - 32) : b[i];
        if (x != y)
            return false;
    }
    return true;
}

inline BoardScan scan_boards(const Mem &m, uint64_t first, uint64_t last, const std::string &key)
{
    BoardScan out;
    if (!first || last < first)
        return out;
    out.listed = size_t(std::min<uint64_t>((last - first) / 8, BOARD_LIST_MAX));
    std::string prefix = key.size() >= 5 ? key.substr(0, 5) : key;
    for (size_t i = 0; i < out.listed; i++) {
        uint64_t board = 0, np = 0;
        std::string name;
        if (m.get(first + 8 * i, board) && board && m.get(board + BOARD_NAME, np) && np)
            name = read_c(m, np);
        if (!key.empty() && same_board_name(name, key))
            out.found = true;
        if (name.compare(0, prefix.size(), prefix) == 0) {
            out.same_prefix++;
            if (out.sample.size() < 80 && name.size() > 5)
                out.sample += (out.sample.empty() ? "" : " ") + name.substr(5);
        }
    }
    return out;
}

// The garage monitor's leaderboard tab of our content shows the session's board. A leaderboard view whose lookup finds no board
// skips its board work and never makes its child list model (+0x9a0), and activating that tab reads the null child. So the core
// forces the tab for our content (builds it, raises car_monitor_tt_leaderboard_privilege_check, grants its type 2 privilege check,
// turns a denied tt_privilege_check_done into granted) only while the session's latest lookup did not miss the board. Before any
// lookup of the session the forcing stands as it was; after a lookup that missed, the tab is left to the game, which builds no
// leaderboard tab for a Time Trial without a board.
enum class BoardListed : uint8_t { unknown, listed, missing };

inline BoardListed board_listed(bool found) { return found ? BoardListed::listed : BoardListed::missing; }

inline bool garage_tab_allowed(bool bridge_on, bool ours_content, BoardListed listed)
{
    return bridge_on && ours_content && listed != BoardListed::missing;
}

// One rule for "this board is ours": an addon's descriptor or a custom track. The request redirect and the guards use it.
// The garage monitor's `tt_privilege_check_done` command carries one byte, the privilege result. A denied result (0) becomes
// granted (1) for our content with the bridge on; a granted result, an unreadable value (0xff) and any other value stay.
inline uint8_t garage_result(uint8_t value, bool bridge_on, bool ours_content)
{
    return value == 0 && bridge_on && ours_content ? 1 : value;
}

// The garage's leaderboard privilege control is the UI node `leaderboard_privilege_check`, whose name id is 0x57172c5d. The id is a
// plain 32-bit value stored in the control object; the predicate looks for it as an aligned dword in a copied byte window.
// Returns the byte offset of the first match, or -1 for no match or a window too short to hold one.
constexpr uint32_t GARAGE_LEADERBOARD_NODE = 0x57172c5d;

inline int garage_node_offset(const uint8_t *bytes, size_t n, uint32_t want = GARAGE_LEADERBOARD_NODE)
{
    if (!bytes)
        return -1;
    for (size_t i = 0; i + 4 <= n; i += 4) {
        uint32_t v = uint32_t(bytes[i]) | uint32_t(bytes[i + 1]) << 8 | uint32_t(bytes[i + 2]) << 16 | uint32_t(bytes[i + 3]) << 24;
        if (v == want)
            return int(i);
    }
    return -1;
}

// The garage monitor, the track select's leaderboard button, the leaderboard privilege checks and the session timer matter only in
// Time Trial (its front end and its sessions). Their hooks return at their first line in a game mode known to be another one (a
// career, a Grand Prix, story, F1 World); an unknown mode (the main menu, a mode not listed, no read yet) and the diagnostics keep them.
inline bool tt_hooks_idle(swofty_core::apply::facts::ModeKind kind, bool diag_on)
{
    using K = swofty_core::apply::facts::ModeKind;
    return !diag_on && (kind == K::career || kind == K::other);
}

// An online-state gate skips the game's checks only for a request of ours whose checks fail, so the op is read (its method, board
// and route) only when the checks fail and the bridge can answer it, or for the diagnostics.
inline bool gate_reads_op(bool diag_on, bool bridge_on, bool push_hooked, bool on_result, bool checks_fail)
{
    return diag_on || (bridge_on && push_hooked && on_result && checks_fail);
}

// The garage monitor panel's update runs every frame. A panel that was driven, or that was seen with content that is not ours
// while the content generation (bumped when the session's track is read and at a session's enter and exit) has not moved, returns
// at once.
inline bool garage_update_idle(uint64_t panel, uint64_t driven_panel, uint64_t idle_panel, uint32_t idle_gen, uint32_t gen)
{
    return !panel || panel == driven_panel || (panel == idle_panel && idle_gen == gen);
}

// Driving the garage monitor's `car_monitor_tt_leaderboard_privilege_check` ourselves: allowed once per panel object, only for our content with
// the bridge on, and only when the panel's tab list (begin/end pointers) is a plausible non-empty list of 8-byte entries.
inline bool garage_should_drive(bool bridge_on, bool ours_content, uint64_t panel, uint64_t tabs_begin, uint64_t tabs_end, uint64_t driven_panel, uint64_t driven_count = 0)
{
    if (!bridge_on || !ours_content || !panel || !tabs_begin || tabs_end <= tabs_begin)
        return false;
    uint64_t span = tabs_end - tabs_begin;
    return span % 8 == 0 && span / 8 <= 64 && (panel != driven_panel || span / 8 != driven_count);
}

// The garage monitor builds its leaderboard tabs (global and friends, class ctor 0x142fc7280) in a time trial garage only when the
// leaderboard block byte ([[0x1460fddb8]+0x18]) is 0 and the feature flag bit 37 of Ne_Get_FeatureFlags() is set. Offline one of
// them is not. For our content with the bridge on the build sees a 0 block byte and the flags with bit 37 set; nothing else changes.
constexpr uint64_t GARAGE_LEADERBOARD_FEATURE = 0x2000000000ull;

inline uint64_t garage_feature_flags(uint64_t flags, bool bridge_on, bool ours_content)
{
    return bridge_on && ours_content ? flags | GARAGE_LEADERBOARD_FEATURE : flags;
}

// Replay capture (SWOFTY_TT_REPLAY_CAPTURE=1). A personal best marks a capture pending; the game's own save (ReplayManager::SaveFullReplay) is
// started once, a short delay later, from the replay system's own tick.
constexpr uint32_t REPLAY_MAGIC = 0xe32fcaa8u;

// Replays are captured only for Time Trial personal bests, so the replay system hooks act only in a live Time Trial session; a Grand
// Prix, a career race or the front end keeps the game's replay system to itself.
inline bool replay_session_on(bool session_live, bool time_trial)
{
    return session_live && time_trial;
}

inline bool replay_capture_due(int64_t pending_ms, int64_t now_ms, int64_t delay_ms)
{
    return pending_ms > 0 && now_ms >= pending_ms + delay_ms;
}

// Byte offset of the replay header magic inside the first `n` bytes of a file (little endian dword at an aligned offset), or -1.
inline int replay_magic_offset(const uint8_t *bytes, size_t n)
{
    if (!bytes)
        return -1;
    for (size_t i = 0; i + 4 <= n && i < 16; i += 4) {
        uint32_t v = uint32_t(bytes[i]) | uint32_t(bytes[i + 1]) << 8 | uint32_t(bytes[i + 2]) << 16 | uint32_t(bytes[i + 3]) << 24;
        if (v == REPLAY_MAGIC)
            return int(i);
    }
    return -1;
}

// The replay loader's save-slot lookup of a Time Trial file, whose game mode has no save-slot component of its own. Our component stands in
// only while the load start hook is armed (it applies the replay's setup and presses its box before the game's own load runs), and never for
// a Theatre pick that came without its setup file: the game's own load of a Time Trial file starts a session without the player's car.
// The answer is why nothing stands in, or null when our component does.
inline const char *replay_stand_in_refusal(bool load_start_armed, int64_t bare_pick_ms, int64_t now_ms, int64_t window_ms = 10000)
{
    if (!load_start_armed)
        return "the load start hook is not armed, so the replay's setup could not be applied";
    if (bare_pick_ms > 0 && now_ms >= bare_pick_ms && now_ms - bare_pick_ms < window_ms)
        return "the Theatre pick has no setup file (tt/replays/<file>.json), and the game's own load would start a Time Trial without the player's car";
    return nullptr;
}

inline std::string replay_capture_name(uint64_t unix_seconds, uint32_t lap_ms)
{
    return "pb_" + std::to_string(unix_seconds) + "_" + std::to_string(lap_ms) + ".replay";
}

// The replay MANAGER (the object SaveFullReplay and its path builder are methods of; the results screen holds it) is not the GameReplaySystem:
// it holds the system at +0x40, an embedded directory provider with a vtable at +0x20, a lock pointer at +0x118, a job queue pointer at +0x150
// and a bool at +0x6e1. All values are plain reads; the call is allowed only when every one fits.
struct ReplayManagerView {
    uint64_t vtable20 = 0, system40 = 0, lock118 = 0, queue150 = 0;
    uint8_t flag6e1 = 0xff;
};

inline bool replay_manager_plausible(const ReplayManagerView &m, uint64_t system, uint64_t module_lo, uint64_t module_hi, bool vtable_readable, bool lock_readable, bool queue_readable)
{
    return system != 0 && m.system40 == system && m.vtable20 >= module_lo && m.vtable20 < module_hi && vtable_readable && m.flag6e1 <= 1 && m.lock118 > 0x10000 &&
           m.lock118 < 0x7fffffffffffull && lock_readable && m.queue150 > 0x10000 && m.queue150 < 0x7fffffffffffull && queue_readable;
}

// Our own save runs with the game's save messages suppressed (the "saving" modal and the success or error result) for a bounded window.
inline bool replay_ui_suppressed(int64_t until_ms, int64_t now_ms) { return until_ms > 0 && now_ms < until_ms; }

// Pre-check mirroring the first conditions of the game's save job (Game_Replay_SaveFullReplay 0x142c46a00): it needs the "main" module with both
// of its replay buffers (+0xc0 full, +0xd0 key frames) and a session object ([[mgr+0x28]+0x88]+0x260). The call is skipped only when the module
// was found and one of them is missing; an undecoded module list never blocks (the job then decides).
inline bool replay_precheck_blocks(bool main_found, uint64_t full_buffer, uint64_t key_buffer, uint64_t session_ptr, bool session_known)
{
    if (session_known && session_ptr == 0)
        return true;
    return main_found && (full_buffer == 0 || key_buffer == 0);
}

// A lap is a new session best only when the game itself reported it valid (1) and it is faster than the session's best so far (0 = none
// yet). A lap of unknown validity (-1, timed by our own clock when the game gave no lap result) is no best: the clock can run over a lap
// restart or a teleport and time a "lap" of a few seconds.
inline bool replay_lap_is_best(int64_t lap_ms, int valid, int64_t session_best_ms)
{
    return lap_ms > 0 && valid == 1 && (session_best_ms <= 0 || lap_ms < session_best_ms);
}

// What a finished lap does to the recording: `mark` saves the replay, `new_best` makes the lap the session's best, and `restart` restarts
// the recording at the line. Only a new best lowers the session best; the test switch `capture_any` saves every lap without touching it.
// A lap that is no best (slower, invalid or of unknown validity) cannot be part of a saved best, so the recording restarts at its line,
// unless it already restarted before the line on that lap.
struct ReplayLapOutcome {
    bool mark = false, new_best = false, restart = false;
};

inline ReplayLapOutcome replay_lap_outcome(int64_t lap_ms, int valid, int64_t session_best_ms, bool capture_any, bool restarted_early)
{
    ReplayLapOutcome o;
    o.new_best = replay_lap_is_best(lap_ms, valid, session_best_ms);
    o.mark = o.new_best || (capture_any && lap_ms > 0);
    o.restart = !o.mark && !restarted_early;
    return o;
}

// Why the replay recording restarts: before the line of a lap already slower than the best; at the line after a finished lap that is no
// best; or at the line where timing starts after an out-lap or a dropped lap (the car was moved or reset, stood still, or the lap was too
// short), where the recording holds no part of a lap.
enum ReplayRestart : int { restart_none, restart_before_line, restart_after_lap, restart_timing_starts };

inline const char *replay_restart_when(int why)
{
    switch (why) {
    case restart_before_line: return "before the line";
    case restart_after_lap: return "at the line";
    case restart_timing_starts: return "at the line, timing starts";
    }
    return "";
}

// The values the recording restart reads, each at the width the game itself reads it: [sys+0x78] is the byte Ne_Replay_SetState checks
// before it changes state (the reset body writes the u16 at +0x79 = 1, so a dword read at +0x78 is not 0 after any reset);
// [queue+0x48] is the dword SaveFullReplay reads as "busy"; [mgr+0x428] is the qword the load start compares with -1.
struct ReplayRestartView {
    bool functions = false, capture = false, viewer_idle = false, loading = false, pending = false, saving = false, system_readable = false,
         system_vtable = false, manager_ok = false, queue_readable = false;
    uint8_t active89 = 0, external78 = 0, recording6e1 = 0;
    uint32_t state20 = 0, queue_busy48 = 0;
    int64_t loaded428 = 0;
    uint64_t sys = 0, mgr = 0, queue = 0;
};

// The first guard of the restart that does not hold, as "<name> <value>"; empty when all hold.
inline std::string replay_restart_blocker(const ReplayRestartView &v)
{
    char b[112];
    auto say = [&](const char *name, unsigned long long value) {
        snprintf(b, sizeof b, "%s 0x%llx", name, value);
        return std::string(b);
    };
    if (!v.functions)
        return "the reset functions did not resolve";
    if (!v.capture)
        return "replay capture off";
    if (!v.viewer_idle)
        return "the replay viewer is in use";
    if (v.loading)
        return "a replay is being loaded into the session";
    if (v.pending)
        return "a replay save is marked";
    if (v.saving)
        return "our replay save is still being copied";
    if (!v.sys || !v.system_readable)
        return say("replay system", v.sys);
    if (!v.system_vtable)
        return say("replay system without the GameReplaySystem vtable", v.sys);
    if (v.active89 == 0)
        return say("[sys+0x89] (recording active)", v.active89);
    if (v.state20 != 0)
        return say("[sys+0x20] (state, 0 = Record)", v.state20);
    if (v.external78 != 0)
        return say("[sys+0x78] (external control)", v.external78);
    if (!v.mgr || !v.manager_ok)
        return say("replay manager", v.mgr);
    if (v.recording6e1 != 1)
        return say("[mgr+0x6e1] (recording flag)", v.recording6e1);
    if (v.loaded428 != -1)
        return say("[mgr+0x428] (loaded replay, -1 = none)", static_cast<unsigned long long>(v.loaded428));
    if (!v.queue || !v.queue_readable)
        return say("job queue [mgr+0x150]", v.queue);
    if (v.queue_busy48 != 0)
        return say("[queue+0x48] (job queue busy)", v.queue_busy48);
    return {};
}

// The save may be started only with the bridge on, with a replay system whose vtable is the game's and a recording enabled. The content is
// not a condition: every lap the boards accept, stock or addon, gets its replay.
inline bool replay_capture_allowed(bool env_on, bool bridge_on, bool vtable_ok, uint8_t recording_flag, uint64_t job_queue)
{
    return env_on && bridge_on && vtable_ok && recording_flag == 1 && job_queue != 0;
}

// The game finished writing when the size is non-zero and unchanged across `stable_needed` polls.
inline bool replay_file_settled(uint64_t size, uint64_t last_size, int stable_polls, int stable_needed)
{
    return size > 0 && size == last_size && stable_polls >= stable_needed;
}

// The garage monitor panel's tab builder may have run before our content was known, leaving the panel without the leaderboard tab. For our
// content in a time trial garage (session type 0x12) the builder is run again once per panel object, unless it already ran for it with our content.
constexpr int GARAGE_TT_SESSION = 0x12;

inline bool garage_should_rebuild(bool bridge_on, bool ours_content, uint64_t panel, uint64_t built_ours_panel, int session_type, uint64_t tab_count)
{
    return bridge_on && ours_content && panel && panel != built_ours_panel && session_type == GARAGE_TT_SESSION && tab_count > 0 && tab_count <= 64;
}

// The builder reaches the leaderboard block in a time trial garage only when the panel's registry bool [[panel+0x938]+0x80] is non-zero
// (checked at 0x141f7208e). For our content in a session of type 0x12 it reads as 1.
inline uint8_t garage_gate_byte(uint8_t value, bool bridge_on, bool ours_content, int session_type)
{
    return bridge_on && ours_content && session_type == GARAGE_TT_SESSION ? 1 : value;
}

// Which leaderboard tabs the garage builder appends: our content gets only the global one (index 0); everything else keeps both.
inline bool garage_append_tab(int leaderboard_kind, bool bridge_on, bool ours_content)
{
    return leaderboard_kind == 0 || !(bridge_on && ours_content);
}

inline uint8_t garage_block_byte(uint8_t block, bool bridge_on, bool ours_content)
{
    return bridge_on && ours_content ? 0 : block;
}

inline bool ours(int descriptor, bool track_custom) { return descriptor >= FIRST_ADDON_DESCRIPTOR || track_custom; }

// Whether a board is the bridge's, the same answer for the request hooks and the ghost handler's hooks (write precondition,
// session token test, rival finder). The board's own track counts as well as the session's content: the session's track is read
// only once the session is entered, and a custom track's board is in use from the loading screen on.
inline bool board_ours(const BoardName &b, bool board_track_custom, bool content_ours)
{
    return b.ok && ours(b.descriptor, board_track_custom || content_ours);
}

// Routing

enum class Route { pass, mailbox, local };

inline Route route_of(Method method, const BoardName &b, bool bridge_on, bool track_custom = false)
{
    if (method == Method::none || !b.ok || !ours(b.descriptor, track_custom))
        return Route::pass;
    if (bridge_on)
        return Route::mailbox;
    return method == Method::add_entry || method == Method::update_binary ? Route::local : Route::pass;
}

inline const char *local_body(Method method) { return method == Method::add_entry ? "{\"Rank\":0}" : "{}"; }

// Guards

inline bool addon_content(std::string_view content)
{
    return !content.empty() && content.substr(0, 5) != "game/";
}

inline bool ours_session(std::string_view content, bool track_custom)
{
    return ours(addon_content(content) ? FIRST_ADDON_DESCRIPTOR : 0, track_custom);
}

// Mailbox

constexpr int64_t STALE_SECONDS = 60;

inline std::string request_json(uint64_t seq, Method method, const json::Value &args, std::string_view content,
                                std::string_view track, const BoardName &board, std::string_view board_text,
                                std::string_view ghost_id_b64)
{
    json::Value v = json::Value::object();
    v["seq"] = json::Value(int64_t(seq));
    v["method"] = json::Value(std::string(method_name(method)));
    v["args"] = args;
    json::Value b = json::Value::object();
    b["content"] = json::Value(std::string(content));
    b["track"] = json::Value(std::string(track));
    b["weather"] = json::Value(std::string(weather_word(board.weather)));
    b["game_board"] = json::Value(std::string(board_text));
    v["board"] = b;
    json::Value p = json::Value::object();
    if (!ghost_id_b64.empty())
        p["ghost_id"] = json::Value(std::string(ghost_id_b64));
    v["player"] = p;
    return json::dump(v);
}

inline std::string base64(const uint8_t *p, size_t n)
{
    static const char T[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string o;
    for (size_t i = 0; i < n; i += 3) {
        uint32_t v = uint32_t(p[i]) << 16 | (i + 1 < n ? uint32_t(p[i + 1]) << 8 : 0) | (i + 2 < n ? p[i + 2] : 0);
        o += T[v >> 18 & 63];
        o += T[v >> 12 & 63];
        o += i + 1 < n ? T[v >> 6 & 63] : '=';
        o += i + 2 < n ? T[v & 63] : '=';
    }
    return o;
}

struct Answer {
    uint64_t seq = 0;
    int status = 0;
    std::string body;
};

inline std::optional<Answer> parse_answer(std::string_view text, uint64_t want_seq)
{
    json::Value v = json::parse(text);
    if (!v.is_object() || !v["seq"].is_int() || !v["status"].is_int() || !v["body"].is_string())
        return std::nullopt;
    Answer a;
    a.seq = uint64_t(v["seq"].as_int());
    a.status = int(v["status"].as_int());
    a.body = std::string(v["body"].as_string());
    if (a.seq != want_seq || a.status < 100 || a.status > 599)
        return std::nullopt;
    return a;
}

inline std::string error_body(std::string_view message)
{
    json::Value v = json::Value::object();
    v["Message"] = json::Value(std::string(message));
    v["Data"] = json::Value();
    return json::dump(v);
}

// Pending operations

struct Pending {
    uint64_t seq = 0, op = 0;
    Method method = Method::none;
    BoardName board;
    std::string board_text;
    json::Value args = json::Value::object();
    int64_t start_ms = 0, deadline_ms = 0;
    bool sent = false, answered = false, local = false;
    int status = 0;
    std::string body;
};

struct Completion {
    uint64_t seq = 0, op = 0;
    Method method = Method::none;
    std::string board_text;
    int status = 0;
    std::string body;
    enum Why { answered, timed_out, local, evicted } why = answered;
    int64_t waited_ms = 0;
};

inline const char *why_name(Completion::Why w)
{
    return w == Completion::answered ? "answered" : w == Completion::timed_out ? "timed out" : w == Completion::local ? "answered locally" : "dropped";
}

class Table {
public:
    explicit Table(uint64_t first_seq = 1) : next_(first_seq) {}

    uint64_t add(uint64_t op, Method method, const Args &args, int64_t now_ms, bool local)
    {
        if (items_.size() >= MAX_PENDING)
            for (auto &p : items_)
                if (!p.answered) {
                    p.answered = true;
                    p.status = 503;
                    p.body = error_body("too many pending requests");
                    p.sent = true;
                    break;
                }
        Pending p;
        p.seq = next_++;
        p.op = op;
        p.method = method;
        p.board = args.board;
        p.board_text = args.board_text;
        p.args = args.values;
        p.start_ms = now_ms;
        p.deadline_ms = now_ms + TIMEOUT_MS;
        p.local = local;
        if (local) {
            p.answered = true;
            p.sent = true;
            p.status = 200;
            p.body = local_body(method);
        }
        items_.push_back(std::move(p));
        return items_.back().seq;
    }

    std::vector<Pending *> unsent()
    {
        std::vector<Pending *> out;
        for (auto &p : items_)
            if (!p.sent)
                out.push_back(&p);
        return out;
    }

    std::vector<Pending *> waiting()
    {
        std::vector<Pending *> out;
        for (auto &p : items_)
            if (p.sent && !p.answered)
                out.push_back(&p);
        return out;
    }

    bool answer(uint64_t seq, int status, std::string body)
    {
        for (auto &p : items_)
            if (p.seq == seq && !p.answered) {
                p.answered = true;
                p.status = status;
                p.body = std::move(body);
                return true;
            }
        return false;
    }

    std::vector<Completion> take_due(int64_t now_ms)
    {
        std::vector<Completion> out;
        for (auto it = items_.begin(); it != items_.end();) {
            bool timed = !it->answered && now_ms >= it->deadline_ms;
            if (it->answered || timed) {
                Completion c;
                c.seq = it->seq;
                c.op = it->op;
                c.method = it->method;
                c.board_text = it->board_text;
                c.waited_ms = now_ms - it->start_ms;
                if (timed) {
                    c.status = 504;
                    c.body = error_body("The request timed out");
                    c.why = Completion::timed_out;
                } else {
                    c.status = it->status;
                    c.body = it->body;
                    c.why = it->local ? Completion::local : Completion::answered;
                }
                out.push_back(std::move(c));
                it = items_.erase(it);
            } else
                ++it;
        }
        return out;
    }

    size_t size() const { return items_.size(); }
    void clear() { items_.clear(); }

private:
    uint64_t next_;
    std::vector<Pending> items_;
};

// The synthetic response the method's OnResult reads: +0x88 u8 0, +0x98 u8 0 (success), +0x9c u32 status, +0xa0 -> body
// object {char *data, u64 unknown, u64 length}.

constexpr size_t RESPONSE_SIZE = 0xb0;

struct Response {
    alignas(16) uint8_t raw[RESPONSE_SIZE] = {};
    uint64_t body_obj[3] = {};
    std::string data;

    void fill(int status, std::string text)
    {
        memset(raw, 0, sizeof raw);
        data = std::move(text);
        body_obj[0] = reinterpret_cast<uint64_t>(data.c_str());
        body_obj[1] = data.size();
        body_obj[2] = data.size();
        raw[0x88] = 0;
        raw[0x98] = 0;
        uint32_t s = uint32_t(status);
        memcpy(raw + 0x9c, &s, 4);
        uint64_t p = reinterpret_cast<uint64_t>(&body_obj[0]);
        memcpy(raw + 0xa0, &p, 8);
    }
};

// Board definitions XML

struct XmlElement {
    size_t begin = 0, open_end = 0, end = 0;
    bool self_closing = false;
    std::string id, name;
    int64_t id_value = -1;
};

struct XmlReport {
    bool changed = false;
    std::string why;
    std::string template_name;
    size_t added = 0, descriptors = 0, tracks = 0, existing = 0, capped = 0;
    size_t custom_tracks = 0, custom_added = 0, stock_descriptors = 0;
    int64_t first_id = -1, last_id = -1;
};

struct Attr {
    std::string name;
    size_t value_begin = 0, value_end = 0;
};

inline bool xml_space(char c) { return c == ' ' || c == '\t' || c == '\r' || c == '\n'; }

inline std::vector<Attr> attributes(const std::string &x, size_t from, size_t to)
{
    std::vector<Attr> out;
    size_t i = from;
    while (i < to) {
        while (i < to && (xml_space(x[i]) || x[i] == '/'))
            i++;
        size_t ns = i;
        while (i < to && !xml_space(x[i]) && x[i] != '=' && x[i] != '/' && x[i] != '>')
            i++;
        if (i == ns)
            break;
        Attr a;
        a.name = x.substr(ns, i - ns);
        while (i < to && xml_space(x[i]))
            i++;
        if (i >= to || x[i] != '=')
            continue;
        i++;
        while (i < to && xml_space(x[i]))
            i++;
        if (i >= to || (x[i] != '"' && x[i] != '\''))
            break;
        char q = x[i++];
        a.value_begin = i;
        while (i < to && x[i] != q)
            i++;
        if (i >= to)
            break;
        a.value_end = i++;
        out.push_back(std::move(a));
    }
    return out;
}

inline size_t tag_end(const std::string &x, size_t from)
{
    char quote = 0;
    for (size_t i = from; i < x.size(); i++) {
        if (quote) {
            if (x[i] == quote)
                quote = 0;
        } else if (x[i] == '"' || x[i] == '\'')
            quote = x[i];
        else if (x[i] == '>')
            return i;
    }
    return std::string::npos;
}

inline std::vector<XmlElement> leaderboard_elements(const std::string &x)
{
    std::vector<XmlElement> out;
    static const std::string OPEN = "<leaderboard";
    size_t at = 0;
    while ((at = x.find(OPEN, at)) != std::string::npos) {
        size_t after = at + OPEN.size();
        if (after >= x.size() || !(xml_space(x[after]) || x[after] == '>' || x[after] == '/')) {
            at = after;
            continue;
        }
        size_t gt = tag_end(x, after);
        if (gt == std::string::npos)
            break;
        XmlElement e;
        e.begin = at;
        e.open_end = gt + 1;
        e.self_closing = x[gt - 1] == '/';
        if (e.self_closing)
            e.end = gt + 1;
        else {
            size_t close = x.find("</leaderboard", gt);
            if (close == std::string::npos)
                break;
            size_t cg = x.find('>', close);
            if (cg == std::string::npos)
                break;
            e.end = cg + 1;
        }
        for (auto &a : attributes(x, after, e.self_closing ? gt - 1 : gt)) {
            if (a.name == "id")
                e.id = x.substr(a.value_begin, a.value_end - a.value_begin);
            else if (a.name == "name")
                e.name = x.substr(a.value_begin, a.value_end - a.value_begin);
        }
        char *stop = nullptr;
        long long id = strtoll(e.id.c_str(), &stop, 10);
        if (!e.id.empty() && stop && !*stop)
            e.id_value = id;
        out.push_back(std::move(e));
        at = out.back().end;
    }
    return out;
}

inline std::string clone_element(const std::string &x, const XmlElement &t, int64_t id, const std::string &name)
{
    std::string text = x.substr(t.begin, t.end - t.begin);
    size_t open_len = t.open_end - t.begin;
    size_t after = std::string("<leaderboard").size();
    size_t gt = open_len - 1;
    std::string opening = text.substr(0, open_len);
    auto attrs = attributes(opening, after, t.self_closing ? gt - 1 : gt);
    for (size_t i = attrs.size(); i-- > 0;) {
        const Attr &a = attrs[i];
        std::string value = opening.substr(a.value_begin, a.value_end - a.value_begin);
        std::string repl;
        if (a.name == "id")
            repl = std::to_string(id);
        else if (value == t.name)
            repl = name;
        else
            continue;
        opening.replace(a.value_begin, a.value_end - a.value_begin, repl);
    }
    return opening + text.substr(open_len);
}

// Clones the first LBV element of the buffer once per board a descriptor and track call for, ids above every id present,
// after the last leaderboard element: every addon descriptor over the stock tracks and the custom tracks' stat ids, and
// every stock descriptor the buffer uses over the custom stat ids. With fill_stock_descriptors every game descriptor (0 to
// 5) also gets the boards of the tracks the buffer names that it lacks: an addon season that plays under a game descriptor
// (classic cars use 1) has no board for a track the stock game never ran that descriptor on. A name the buffer defines is
// never added again.
inline std::string extend_xml(const std::string &x, const std::vector<int> &descriptors, const std::vector<int> &custom_stat_ids,
                              XmlReport &rep, bool fill_stock_descriptors = false)
{
    rep = XmlReport{};
    auto elements = leaderboard_elements(x);
    if (elements.empty()) {
        rep.why = "no <leaderboard> element in the buffer";
        return x;
    }
    const XmlElement *tmpl = nullptr;
    int64_t max_id = -1;
    std::set<int> tracks, stock_descriptors;
    std::set<std::string> names;
    for (auto &e : elements) {
        max_id = std::max(max_id, e.id_value);
        names.insert(e.name);
        BoardName b = parse_board(e.name);
        if (b.ok) {
            tracks.insert(b.track);
            if (b.descriptor < FIRST_ADDON_DESCRIPTOR)
                stock_descriptors.insert(b.descriptor);
            if (!tmpl && e.id_value >= 0)
                tmpl = &e;
        }
    }
    if (!tmpl) {
        rep.why = "no stock LBV element to use as the template";
        return x;
    }
    rep.template_name = tmpl->name;
    std::set<int> want;
    for (int d : descriptors)
        if (d >= FIRST_ADDON_DESCRIPTOR && d <= LAST_NAMED_DESCRIPTOR)
            want.insert(d);
    std::set<int> custom;
    for (int t : custom_stat_ids)
        if (t >= 0 && t <= MAX_TRACK_NUMBER)
            custom.insert(t);
    rep.descriptors = want.size();
    rep.tracks = tracks.size();
    rep.custom_tracks = custom.size();
    rep.stock_descriptors = custom.empty() ? 0 : stock_descriptors.size();
    if (want.empty() && custom.empty() && !fill_stock_descriptors) {
        rep.why = "no addon descriptor ids are assigned yet and no custom tracks";
        return x;
    }
    std::set<int> addon_tracks = tracks;
    addon_tracks.insert(custom.begin(), custom.end());
    const XmlElement &last = elements.back();
    size_t ls = x.rfind('\n', tmpl->begin);
    ls = ls == std::string::npos ? 0 : ls + 1;
    std::string indent;
    for (size_t i = ls; i < tmpl->begin && (x[i] == ' ' || x[i] == '\t'); i++)
        indent += x[i];
    std::string eol = x.find("\r\n") != std::string::npos ? "\r\n" : "\n";
    std::string add;
    int64_t id = max_id;
    auto add_board = [&](int d, int t, char w) {
        std::string name = board_name(d, t, w);
        if (names.count(name)) {
            rep.existing++;
            return;
        }
        if (rep.added >= MAX_XML_BOARDS) {
            rep.capped++;
            return;
        }
        id++;
        if (rep.first_id < 0)
            rep.first_id = id;
        rep.last_id = id;
        add += eol + indent + clone_element(x, *tmpl, id, name);
        names.insert(name);
        rep.added++;
        if (custom.count(t))
            rep.custom_added++;
    };
    for (int d : want)
        for (int t : addon_tracks)
            for (char w : {'D', 'W'})
                add_board(d, t, w);
    for (int d : stock_descriptors)
        for (int t : custom)
            for (char w : {'D', 'W'})
                add_board(d, t, w);
    if (fill_stock_descriptors)
        for (int d = 0; d < FIRST_ADDON_DESCRIPTOR; d++)
            for (int t : tracks)
                for (char w : {'D', 'W'})
                    add_board(d, t, w);
    if (!rep.added) {
        rep.why = "every addon and custom track board is already defined";
        return x;
    }
    rep.changed = true;
    std::string out = x;
    out.insert(last.end, add);
    return out;
}

// Online-state gate of the F1os method starts (FINDINGS "Third pass"). Every F1os op's OnStart opens with the same
// template: the signed-in presence record, the platform state words S+0x60 / S+0x64 and the session token block are each
// compared, and a failing one finishes the op with an AccountServiceOperation state (8 login error, 9 platform overlay
// disabled, 10 no presence, 4/5/7 no session) instead of reaching Game_F1os_RequestRing_Push. The template is found by
// bytes; the five Time Trial methods are told apart by which OnResult the function takes the address of.

namespace gate {

constexpr const char *TEMPLATE =
    "48 8B 8E 00 01 00 00 48 8B 49 40 48 8B 89 40 01 00 00 48 8B 89 98 01 00 00 48 85 C9 0F 84 ?? ?? ?? ?? 8B 91 C8 01 00 00 F7 D2 F6 C2 03 "
    "0F 85 ?? ?? ?? ?? 48 83 B9 C0 01 00 00 00 0F 84 ?? ?? ?? ?? 83 FF 02 0F 85 ?? ?? ?? ?? 83 F8 02 0F 85 ?? ?? ?? ?? 48 8B 05 ?? ?? ?? ?? "
    "48 8B 80 E0 04 00 00 80 B8 90 00 02 00 00 0F 84 ?? ?? ?? ?? 48 8B 80 50 01 02 00 48 85 C0 0F 84 ?? ?? ?? ?? 80 38 00 0F 84 ?? ?? ?? ?? "
    "80 7E 32 00";

constexpr size_t TEMPLATE_LEN = 139;
constexpr size_t HOOK_AT = 25;
constexpr size_t SESSION_GLOBAL_AT = 83;
constexpr size_t RESUME_AT = TEMPLATE_LEN - 4;
constexpr size_t MAX_FUNCTION = 0x4000;

struct Pattern {
    std::array<uint8_t, TEMPLATE_LEN> byte{};
    std::array<bool, TEMPLATE_LEN> any{};
    bool ok = false;
};

inline Pattern pattern()
{
    Pattern p;
    std::string_view t(TEMPLATE);
    size_t n = 0, i = 0;
    while (i < t.size()) {
        if (t[i] == ' ') {
            i++;
            continue;
        }
        if (n >= TEMPLATE_LEN)
            return p;
        if (t[i] == '?') {
            p.any[n] = true;
            i += 2;
        } else {
            p.byte[n] = uint8_t(std::stoi(std::string(t.substr(i, 2)), nullptr, 16));
            i += 2;
        }
        n++;
    }
    p.ok = n == TEMPLATE_LEN;
    return p;
}

inline bool matches_at(const Pattern &p, const uint8_t *at)
{
    for (size_t i = 0; i < TEMPLATE_LEN; i++)
        if (!p.any[i] && at[i] != p.byte[i])
            return false;
    return true;
}

inline std::vector<size_t> find_all(const uint8_t *buf, size_t n)
{
    std::vector<size_t> out;
    Pattern p = pattern();
    if (!p.ok || n < TEMPLATE_LEN)
        return out;
    const uint8_t first = p.byte[0];
    for (size_t i = 0; i + TEMPLATE_LEN <= n; i++) {
        const uint8_t *q = static_cast<const uint8_t *>(memchr(buf + i, first, n - TEMPLATE_LEN + 1 - i));
        if (!q)
            break;
        i = size_t(q - buf);
        if (matches_at(p, q))
            out.push_back(i);
    }
    return out;
}

inline bool lea_to(const uint8_t *buf, size_t from, size_t to, int64_t target)
{
    for (size_t i = from; i + 7 <= to; i++) {
        if ((buf[i] == 0x48 || buf[i] == 0x4c) && buf[i + 1] == 0x8d && (buf[i + 2] & 0xc7) == 0x05) {
            int32_t rel;
            memcpy(&rel, buf + i + 3, 4);
            if (int64_t(i) + 7 + rel == target)
                return true;
        }
    }
    return false;
}

struct Site {
    size_t match = 0, hook = 0, resume = 0;
};

inline int64_t session_global(const uint8_t *buf, size_t match)
{
    int32_t rel;
    memcpy(&rel, buf + match + SESSION_GLOBAL_AT + 3, 4);
    return int64_t(match + SESSION_GLOBAL_AT) + 7 + rel;
}

inline Site site_of(size_t match) { return {match, match + HOOK_AT, match + RESUME_AT}; }

// For each target (an OnResult as an offset into buf) the match of the function that takes its address, 0-based index
// into `matches`, or -1 when none (or more than one) does.
inline std::vector<int> owners(const uint8_t *buf, size_t n, const std::vector<size_t> &matches, const std::vector<int64_t> &targets)
{
    std::vector<int> out(targets.size(), -1);
    for (size_t t = 0; t < targets.size(); t++) {
        if (targets[t] < 0)
            continue;
        int hit = -1;
        bool many = false;
        for (size_t m = 0; m < matches.size(); m++) {
            size_t from = matches[m] + TEMPLATE_LEN;
            size_t to = std::min(m + 1 < matches.size() ? matches[m + 1] : n, matches[m] + MAX_FUNCTION);
            if (to > n)
                to = n;
            if (from < to && lea_to(buf, from, to, targets[t])) {
                many = many || hit >= 0;
                hit = int(m);
            }
        }
        out[t] = many ? -1 : hit;
    }
    return out;
}

enum class Why { none, presence = 10, login = 8, overlay = 9, session = 4 };

struct State {
    uint32_t s60 = 0, s64 = 0;
    bool presence_ok = false;
    bool session_ok = false;
};

inline Why first_failure(const State &s)
{
    if (!s.presence_ok)
        return Why::presence;
    if (s.s60 != 2)
        return Why::login;
    if (s.s64 != 2)
        return Why::overlay;
    if (!s.session_ok)
        return Why::session;
    return Why::none;
}

inline const char *why_text(Why w)
{
    switch (w) {
    case Why::presence: return "10 no signed-in presence record";
    case Why::login: return "8 EA account login error (S+0x60 != 2)";
    case Why::overlay: return "9 platform client overlay disabled (S+0x64 != 2)";
    case Why::session: return "4/5/7 no session token";
    default: return "none, the op would be sent";
    }
}

inline bool presence_ok(uint32_t flags, uint64_t user) { return (~flags & 3u) == 0 && user != 0; }

inline const char *account_state_name(unsigned v)
{
    static const char *const names[] = {"eIdle", "eInProgress", "eServerNotAccessible", "ePrivilegesNotGranted", "eServerDownForMaintenance",
                                        "eAccountBanned", "eAccountSuspened", "eAccountLoggedIn", "eEAAccountLoginError",
                                        "eEAAccountSteamOverlayDisabled", "eCurrentSessionHostile"};
    return v < sizeof names / sizeof *names ? names[v] : "unknown";
}

// The account-state error handler raises a popup route only for 8 (ea_account_login_error) and 9 (the Platform Client
// Overlay popup); with the bridge on those two, and 2 (server not accessible, the offline copies' state), are not shown.
// 10 (a hostile session, seen on the EA app copy) runs the stock handler: it sets #ui_menu_data.online_services.error_code,
// which is how the online-services screen (Time Trial's "checking privileges") learns the check failed and leaves.
inline bool account_error_quiet(bool bridge_on, unsigned state) { return bridge_on && (state == 2 || state == 8 || state == 9); }

}

}
