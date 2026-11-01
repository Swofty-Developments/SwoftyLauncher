// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: the bridge between the launcher page and the running game.
//
// Mailbox in the addon's storage folder, <game>/swofty/addons/swofty.career-editor/
// (protocol/bridge.md has the formats):
//   inbox/<seq>.json   edit batches from the launcher (it writes a temp file, then renames)
//   outbox/<seq>.json  one ack per batch: per-op ok / before / after / error
//   snapshot.json      status plus every registered section, rewritten when it changes
//   journal.jsonl      one line per applied op
//
// Batches, sections and the snapshot body run on the flow thread from the events library's frame
// callback; a watcher thread reads viewer.json and lists the inbox, and a writer thread writes
// snapshot.json, so the flow thread does no file IO while no batch is waiting. Batches run in
// seq order, one per frame, and only while the gate is open: ce::safe() for CE_OP ops,
// ce::in_career() for CE_OP_ANYTIME ops. A blocked batch stays queued and the snapshot says why.
// Every op and section call is wrapped in an SEH guard: a fault is logged, answered as an error
// and the op or section is switched off for the rest of the game session.

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <mutex>
#include <thread>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <set>
#include <string>
#include <string_view>
#include <vector>

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#else
#include <ctime>
#include <unistd.h>
#endif

#include <swofty/mixin.hpp>
#include <swofty/storage.hpp>

#include "bridge.hpp"
#include "section_schedule.hpp"

namespace st = swofty::storage;

namespace ce {

// ---- time ---------------------------------------------------------------------------------------

uint64_t now_ms()
{
#if defined(_WIN32)
    return GetTickCount64();
#else
    timespec t{};
    clock_gettime(CLOCK_MONOTONIC, &t);
    return uint64_t(t.tv_sec) * 1000 + uint64_t(t.tv_nsec / 1000000);
#endif
}

uint64_t wall_ms()
{
#if defined(_WIN32)
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    uint64_t t = (uint64_t(ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
    return t / 10000 - 11644473600000ull;
#else
    timespec t{};
    clock_gettime(CLOCK_REALTIME, &t);
    return uint64_t(t.tv_sec) * 1000 + uint64_t(t.tv_nsec / 1000000);
#endif
}

namespace {

unsigned long pid()
{
#if defined(_WIN32)
    return GetCurrentProcessId();
#else
    return (unsigned long)getpid();
#endif
}

} // namespace

// ---- Writer -------------------------------------------------------------------------------------

namespace {

size_t utf8_len(const unsigned char *p, size_t n)
{
    unsigned char c = p[0];
    size_t len = c >= 0xf0 && c <= 0xf4 ? 4 : c >= 0xe0 ? 3 : c >= 0xc2 && c < 0xe0 ? 2 : 0;
    if (!len || len > n)
        return 0;
    for (size_t i = 1; i < len; ++i)
        if ((p[i] & 0xc0) != 0x80)
            return 0;
    if (len == 3 && ((c == 0xe0 && p[1] < 0xa0) || (c == 0xed && p[1] >= 0xa0)))
        return 0;
    if (len == 4 && ((c == 0xf0 && p[1] < 0x90) || (c == 0xf4 && p[1] >= 0x90)))
        return 0;
    return len;
}

void quote(std::string &out, std::string_view s)
{
    out += '"';
    const auto *p = reinterpret_cast<const unsigned char *>(s.data());
    size_t n = s.size();
    for (size_t i = 0; i < n;) {
        unsigned char c = p[i];
        if (c >= 0x80) {
            size_t len = utf8_len(p + i, n - i);
            if (len) {
                out.append(reinterpret_cast<const char *>(p + i), len);
                i += len;
            } else {
                out += "\xef\xbf\xbd";
                ++i;
            }
            continue;
        }
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20 || c == 0x7f) {
                char b[8];
                std::snprintf(b, sizeof b, "\\u%04x", c);
                out += b;
            } else {
                out += char(c);
            }
        }
        ++i;
    }
    out += '"';
}

} // namespace

void Writer::sep()
{
    if (after_key_) {
        after_key_ = false;
        return;
    }
    if (!first_.empty()) {
        if (!first_.back())
            out_ += ',';
        first_.back() = false;
    }
}

void Writer::begin_object()
{
    sep();
    out_ += '{';
    first_.push_back(true);
}

void Writer::end_object()
{
    out_ += '}';
    if (!first_.empty())
        first_.pop_back();
}

void Writer::begin_array()
{
    sep();
    out_ += '[';
    first_.push_back(true);
}

void Writer::end_array()
{
    out_ += ']';
    if (!first_.empty())
        first_.pop_back();
}

void Writer::key(const char *k)
{
    sep();
    quote(out_, k ? k : "");
    out_ += ':';
    after_key_ = true;
}

void Writer::str(const std::string &v)
{
    sep();
    quote(out_, v);
}

void Writer::num(double v)
{
    sep();
    if (!std::isfinite(v)) {
        out_ += "null";
        return;
    }
    char b[40];
    if (v == std::floor(v) && std::fabs(v) < 9007199254740992.0)
        std::snprintf(b, sizeof b, "%lld", (long long)v);
    else
        std::snprintf(b, sizeof b, "%.17g", v);
    if (!std::strchr(b, 'e') && std::strchr(b, '.')) {
        char t[40];
        for (int p = 6; p <= 17; ++p) {
            std::snprintf(t, sizeof t, "%.*g", p, v);
            if (std::strtod(t, nullptr) == v) {
                std::memcpy(b, t, sizeof t);
                break;
            }
        }
    }
    out_ += b;
}

void Writer::integer(int64_t v)
{
    sep();
    out_ += std::to_string(v);
}

void Writer::boolean(bool v)
{
    sep();
    out_ += v ? "true" : "false";
}

void Writer::null()
{
    sep();
    out_ += "null";
}

// ---- logging ------------------------------------------------------------------------------------

void log(const char *fmt, ...)
{
    char buf[1536];
    va_list ap;
    va_start(ap, fmt);
    std::vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    swofty::log("[career-editor] %s", buf);
}

// ---- registries ---------------------------------------------------------------------------------

namespace {

struct OpEntry {
    std::string name;
    OpFn fn;
    bool anytime;
    std::string quarantined;
};

struct SectionEntry {
    std::string name;
    SectionFn fn;
    std::string quarantined;
    std::string text;
    Json value;
    bool rendered = false;
    uint64_t slow_logged = 0;
    bool big_logged = false;
    sched::Slot slot;
};

std::vector<OpEntry> &op_table()
{
    static std::vector<OpEntry> v;
    return v;
}

std::vector<SectionEntry> &section_table()
{
    static std::vector<SectionEntry> v;
    return v;
}

std::vector<IdleFn> &idle_table()
{
    static std::vector<IdleFn> v;
    return v;
}

std::vector<std::string> &duplicates()
{
    static std::vector<std::string> v;
    return v;
}

void add_op(const char *name, OpFn fn, bool anytime)
{
    if (!name || !fn)
        return;
    for (auto &e : op_table())
        if (e.name == name) {
            duplicates().push_back(std::string("op ") + name);
            return;
        }
    op_table().push_back({name, fn, anytime, {}});
}

OpEntry *find_op(const std::string &name)
{
    for (auto &e : op_table())
        if (e.name == name)
            return &e;
    return nullptr;
}

} // namespace

void register_op(const char *name, OpFn fn)
{
    add_op(name, fn, false);
}

void register_op_anytime(const char *name, OpFn fn)
{
    add_op(name, fn, true);
}

void register_section(const char *name, SectionFn fn)
{
    if (!name || !fn)
        return;
    for (auto &e : section_table())
        if (e.name == name) {
            duplicates().push_back(std::string("section ") + name);
            return;
        }
    SectionEntry e;
    e.name = name;
    e.fn = fn;
    section_table().push_back(std::move(e));
}

// ---- JSON helpers -------------------------------------------------------------------------------

namespace {

void emit(Writer &w, const Json &v, int depth = 0)
{
    if (depth > 32) {
        w.null();
        return;
    }
    switch (v.type) {
    case Json::Null: w.null(); break;
    case Json::Bool: w.boolean(v.b); break;
    case Json::Num: w.num(v.n); break;
    case Json::Str: w.str(v.s); break;
    case Json::Arr:
        w.begin_array();
        for (auto &x : v.a)
            emit(w, x, depth + 1);
        w.end_array();
        break;
    case Json::Obj:
        w.begin_object();
        for (auto &[k, x] : v.o) {
            w.key(k.c_str());
            emit(w, x, depth + 1);
        }
        w.end_object();
        break;
    }
}

void emit_raw(Writer &w, const std::string &raw)
{
    if (raw.empty()) {
        w.null();
        return;
    }
    Json v;
    if (json::parse(raw, v))
        emit(w, v);
    else
        w.str(raw);
}

std::string clip(std::string s, size_t n = 300)
{
    if (s.size() > n) {
        s.resize(n);
        s += "...";
    }
    return s;
}

std::string to_text(const Json &v)
{
    Writer w;
    emit(w, v);
    return w.text();
}

// ---- fault guard --------------------------------------------------------------------------------

#if defined(_WIN32)
int fault_filter(unsigned long code)
{
    switch (code) {
    case EXCEPTION_ACCESS_VIOLATION:
    case EXCEPTION_IN_PAGE_ERROR:
    case EXCEPTION_INT_DIVIDE_BY_ZERO:
    case EXCEPTION_ILLEGAL_INSTRUCTION:
    case EXCEPTION_PRIV_INSTRUCTION:
    case EXCEPTION_DATATYPE_MISALIGNMENT:
    case EXCEPTION_ARRAY_BOUNDS_EXCEEDED:
        return EXCEPTION_EXECUTE_HANDLER;
    default:
        return EXCEPTION_CONTINUE_SEARCH;
    }
}

bool guarded(void (*fn)(void *), void *ctx, unsigned long *code)
{
    __try {
        fn(ctx);
        return true;
    } __except (fault_filter(*code = GetExceptionCode())) {
        return false;
    }
}
#else
bool guarded(void (*fn)(void *), void *ctx, unsigned long *code)
{
    *code = 0;
    fn(ctx);
    return true;
}
#endif

struct OpCall {
    OpFn fn;
    const Json *args;
    OpResult out;
};

void run_op_body(void *p)
{
    auto *c = static_cast<OpCall *>(p);
    try {
        c->out = c->fn(*c->args);
    } catch (const std::exception &e) {
        c->out = OpResult{};
        c->out.error = std::string("exception: ") + e.what();
    } catch (...) {
        c->out = OpResult{};
        c->out.error = "exception";
    }
}

struct SectionCall {
    SectionFn fn;
    Writer w;
    std::string error;
};

void run_section_body(void *p)
{
    auto *c = static_cast<SectionCall *>(p);
    try {
        c->fn(c->w);
    } catch (const std::exception &e) {
        c->error = std::string("exception: ") + e.what();
    } catch (...) {
        c->error = "exception";
    }
}

// ---- mailbox ------------------------------------------------------------------------------------

constexpr uint64_t kStatePollMs = 250;
constexpr uint64_t kInboxPollMs = 500;
constexpr uint64_t kInboxRelistMs = 10000;
constexpr uint64_t kSnapshotMs = 1000;
constexpr uint64_t kHeartbeatMs = 5000;
constexpr uint64_t kViewerFreshMs = 15000;
constexpr auto kSectionGap = std::chrono::milliseconds(8);
constexpr size_t kMaxOps = 256;

using Clock = std::chrono::steady_clock;

struct Batch {
    uint64_t seq = 0;
    bool parsed = false;
    bool bad = false;
    std::string error;
    Json doc;
    bool anytime = true;
};

struct Bridge {
    bool up = false;
    bool storage = false;
    uint64_t boot = 0;
    uint64_t last_state = 0;
    uint64_t last_snapshot = 0;
    uint64_t last_write = 0;
    uint64_t sections_at = 0;
    std::string sections_text;
    std::string sections_json;
    bool sections_dirty = true;
    bool sections_live = false;
    bool force_render = false;
    bool pass_active = false;
    bool pass_rendered = false;
    bool was_live = false;
    bool was_viewer = false;
    const char *last_phase = "";
    sched::Clock sched_clock;
    size_t pass_index = 0;
    Clock::time_point next_section{};
    uint64_t viewer_at = 0;
    uint64_t renders = 0;
    uint64_t pass_ms = 0;
    uint64_t pass_max = 0;
    std::string pass_slowest;
    uint64_t perf_logged = 0;
    std::map<uint64_t, Batch> pending;
    std::set<uint64_t> done;
    uint64_t blocked_seq = 0;
    std::string blocked_why;
    uint64_t blocked_logged_seq = 0;
    std::string blocked_logged_why;
    uint64_t last_ack = 0;
    uint64_t applied = 0;
    uint64_t failed = 0;
    std::string last_body;
};

Bridge g;

struct Watcher {
    std::mutex lock;
    std::string names;
    std::atomic<bool> changed{false};
    std::atomic<uint64_t> viewer_at{0};
    std::atomic<bool> game_in_front{false};
    bool seeded = false;
#if defined(_WIN32)
    std::wstring inbox_dir;
    FILETIME inbox_mtime{};
    bool doorbell = false;
#endif
};

Watcher &watcher()
{
    static Watcher *w = new Watcher;
    return *w;
}

#if defined(_WIN32)
void init_doorbell()
{
    wchar_t path[MAX_PATH * 2];
    DWORD n = GetModuleFileNameW(nullptr, path, DWORD(sizeof path / sizeof path[0]));
    if (!n || n >= sizeof path / sizeof path[0])
        return;
    std::wstring dir(path, n);
    size_t cut = dir.find_last_of(L"\\/");
    if (cut == std::wstring::npos)
        return;
    Watcher &w = watcher();
    w.inbox_dir = dir.substr(0, cut) + L"\\swofty\\addons\\swofty.career-editor\\inbox";
    WIN32_FILE_ATTRIBUTE_DATA a{};
    w.doorbell = GetFileAttributesExW(w.inbox_dir.c_str(), GetFileExInfoStandard, &a) &&
                 (a.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY);
}

bool doorbell_rang()
{
    Watcher &w = watcher();
    if (!w.doorbell)
        return false;
    WIN32_FILE_ATTRIBUTE_DATA a{};
    if (!GetFileAttributesExW(w.inbox_dir.c_str(), GetFileExInfoStandard, &a))
        return true;
    bool changed = CompareFileTime(&a.ftLastWriteTime, &w.inbox_mtime) != 0;
    w.inbox_mtime = a.ftLastWriteTime;
    return changed;
}
#else
void init_doorbell() {}
#endif

std::string inbox_path(uint64_t seq)
{
    return "inbox/" + std::to_string(seq) + ".json";
}

std::string outbox_path(uint64_t seq)
{
    return "outbox/" + std::to_string(seq) + ".json";
}

bool parse_seq(std::string_view name, uint64_t &seq)
{
    if (name.size() < 6 || name.substr(name.size() - 5) != ".json")
        return false;
    std::string_view digits = name.substr(0, name.size() - 5);
    if (digits.empty() || digits.size() > 19)
        return false;
    uint64_t v = 0;
    for (char c : digits) {
        if (c < '0' || c > '9')
            return false;
        v = v * 10 + uint64_t(c - '0');
    }
    seq = v;
    return true;
}

bool read_inbox(std::string &names)
{
    std::vector<char> buf(64 * 1024);
    size_t len = 0;
    st::Result r = st::list("inbox", buf.data(), buf.size(), &len);
    if (r == st::Result::invalid && len > buf.size()) {
        buf.resize(len + 1024);
        r = st::list("inbox", buf.data(), buf.size(), &len);
    }
    if (r != st::Result::ok)
        return false;
    names.assign(buf.data(), std::min(len, buf.size()));
    return true;
}

void merge_inbox(std::string_view all)
{
    std::set<uint64_t> seen;
    size_t at = 0;
    while (at < all.size()) {
        size_t nl = all.find('\n', at);
        std::string_view name = all.substr(at, nl == std::string_view::npos ? std::string_view::npos : nl - at);
        at = nl == std::string_view::npos ? all.size() : nl + 1;
        uint64_t seq;
        if (!parse_seq(name, seq))
            continue;
        seen.insert(seq);
        if (g.done.count(seq) || g.pending.count(seq))
            continue;
        uint64_t sz = 0;
        if (st::size(outbox_path(seq), &sz) == st::Result::ok) {
            log("batch %llu was already acked; dropping its inbox file", (unsigned long long)seq);
            st::remove(inbox_path(seq));
            g.done.insert(seq);
            continue;
        }
        Batch b;
        b.seq = seq;
        g.pending.emplace(seq, std::move(b));
        g.force_render = true;
        log("batch %llu queued", (unsigned long long)seq);
    }
    for (auto it = g.pending.begin(); it != g.pending.end();) {
        if (!seen.count(it->first)) {
            log("batch %llu was withdrawn by the launcher before it ran", (unsigned long long)it->first);
            it = g.pending.erase(it);
        } else {
            ++it;
        }
    }
}

bool game_in_front()
{
#if defined(_WIN32)
    DWORD owner = 0;
    HWND h = GetForegroundWindow();
    return h && GetWindowThreadProcessId(h, &owner) && owner == GetCurrentProcessId();
#else
    return false;
#endif
}

void poll_viewer(Watcher &w)
{
    w.game_in_front = game_in_front();
    std::string text;
    uint64_t at = 0;
    if (st::read("viewer.json", text) == st::Result::ok)
        for (char c : text) {
            if (c < '0' || c > '9')
                break;
            at = at * 10 + uint64_t(c - '0');
        }
    else if (!w.seeded)
        w.seeded = true, st::write("viewer.json", "0");
    w.viewer_at = at;
}

void watch()
{
    Watcher &w = watcher();
    uint64_t listed = now_ms();
    for (;;) {
        std::this_thread::sleep_for(std::chrono::milliseconds(kInboxPollMs));
        poll_viewer(w);
        uint64_t now = now_ms();
#if defined(_WIN32)
        bool relist = !w.doorbell || doorbell_rang() || now - listed >= kInboxRelistMs;
#else
        bool relist = true;
#endif
        std::string names;
        if (!relist || !read_inbox(names))
            continue;
        listed = now;
        std::lock_guard<std::mutex> l(w.lock);
        if (names != w.names) {
            w.names = std::move(names);
            w.changed = true;
        }
    }
}

void take_inbox()
{
    Watcher &w = watcher();
    if (!w.changed.load(std::memory_order_relaxed) || !w.changed.exchange(false))
        return;
    std::string names;
    {
        std::lock_guard<std::mutex> l(w.lock);
        names = w.names;
    }
    merge_inbox(names);
}

void parse_batch(Batch &b)
{
    b.parsed = true;
    std::string text;
    if (st::read(inbox_path(b.seq), text) != st::Result::ok) {
        b.bad = true;
        b.error = "the batch file could not be read";
        return;
    }
    if (!json::parse(text, b.doc) || b.doc.type != Json::Obj) {
        b.bad = true;
        b.error = "the batch is not a JSON object";
        return;
    }
    const Json &ops = b.doc["ops"];
    if (ops.type != Json::Arr) {
        b.bad = true;
        b.error = "the batch has no ops array";
        return;
    }
    if (ops.a.size() > kMaxOps) {
        b.bad = true;
        b.error = "the batch has more than 256 ops";
        return;
    }
    b.anytime = true;
    for (auto &o : ops.a) {
        std::string name = o["op"].str();
        if (name == "bridge.ping")
            continue;
        OpEntry *e = find_op(name);
        if (e && !e->anytime)
            b.anytime = false;
    }
}

Json args_with_expect(const Json &op)
{
    Json args = op["args"];
    if (args.type == Json::Null)
        args.type = Json::Obj;
    const Json &expect = op["expect"];
    if (args.type == Json::Obj && expect.type != Json::Null && args["expect"].type == Json::Null)
        args.o.emplace_back("expect", expect);
    return args;
}

OpResult run_one(const std::string &name, const Json &args, bool &ran)
{
    ran = false;
    OpResult r;
    if (name == "bridge.ping") {
        Writer w;
        w.begin_object();
        w.key("boot");
        w.integer(int64_t(g.boot));
        w.key("mode");
        w.str(state::mode_name(mode()));
        w.key("safe");
        w.boolean(safe());
        w.end_object();
        r.ok = true;
        r.before = w.text();
        r.after = w.text();
        ran = true;
        return r;
    }
    OpEntry *e = find_op(name);
    if (!e) {
        r.error = "unknown op '" + name + "' (not in this build)";
        return r;
    }
    if (!e->quarantined.empty()) {
        r.error = "op switched off after a fault this session: " + e->quarantined;
        return r;
    }
    std::string why = state::blocked(e->anytime);
    if (!why.empty()) {
        r.error = "blocked: " + why;
        return r;
    }
    OpCall call{e->fn, &args, {}};
    unsigned long code = 0;
    if (!guarded(run_op_body, &call, &code)) {
        char b[96];
        std::snprintf(b, sizeof b, "fault 0x%08lx inside the op", code);
        e->quarantined = b;
        log("op %s FAULTED (0x%08lx); it is switched off until the game restarts", name.c_str(), code);
        r.error = b;
        ran = true;
        return r;
    }
    ran = true;
    return call.out;
}

void run_batch(Batch &b)
{
    uint64_t t0 = now_ms();
    const Json &ops = b.doc["ops"];
    bool stop_on_error = b.doc["stop_on_error"].type == Json::Bool && b.doc["stop_on_error"].b;
    std::string session = b.doc["session"].str();
    std::string label = b.doc["label"].str();

    Writer ack;
    ack.begin_object();
    ack.key("seq");
    ack.integer(int64_t(b.seq));
    ack.key("boot");
    ack.integer(int64_t(g.boot));
    ack.key("session");
    ack.str(session);
    ack.key("t");
    ack.integer(int64_t(wall_ms()));
    ack.key("mode");
    ack.str(state::mode_name(mode()));
    ack.key("results");
    ack.begin_array();

    bool all_ok = true;
    size_t n_ok = 0;
    for (size_t i = 0; i < ops.a.size(); ++i) {
        const Json &o = ops.a[i];
        std::string name = o["op"].str();
        Json args = args_with_expect(o);
        bool ran = false;
        OpResult r;
        if (name.empty())
            r.error = "op name missing";
        else if (stop_on_error && !all_ok)
            r.error = "skipped: an earlier op in the batch failed";
        else
            r = run_one(name, args, ran);
        if (r.ok && r.error.empty())
            ++n_ok;
        else
            all_ok = false;

        std::string args_text = to_text(args);
        log("op %llu.%zu %s args=%s -> %s%s%s before=%s after=%s", (unsigned long long)b.seq, i, name.c_str(),
            clip(args_text, 200).c_str(), r.ok ? "ok" : "FAILED", r.error.empty() ? "" : ": ", r.error.c_str(),
            clip(r.before, 160).c_str(), clip(r.after, 160).c_str());

        ack.begin_object();
        ack.key("i");
        ack.integer(int64_t(i));
        ack.key("op");
        ack.str(name);
        ack.key("ok");
        ack.boolean(r.ok && r.error.empty());
        ack.key("ran");
        ack.boolean(ran);
        ack.key("args");
        emit(ack, args);
        ack.key("error");
        if (r.error.empty())
            ack.null();
        else
            ack.str(r.error);
        ack.key("before");
        emit_raw(ack, r.before);
        ack.key("after");
        emit_raw(ack, r.after);
        ack.end_object();

        if (r.ok && r.error.empty() && name != "bridge.ping") {
            Writer j;
            j.begin_object();
            j.key("t");
            j.integer(int64_t(wall_ms()));
            j.key("seq");
            j.integer(int64_t(b.seq));
            j.key("i");
            j.integer(int64_t(i));
            j.key("session");
            j.str(session);
            j.key("label");
            j.str(label);
            j.key("mode");
            j.str(state::mode_name(mode()));
            j.key("op");
            j.str(name);
            j.key("args");
            emit(j, args);
            j.key("before");
            emit_raw(j, r.before);
            j.key("after");
            emit_raw(j, r.after);
            j.end_object();
            st::append("journal.jsonl", j.text() + "\n");
            ++g.applied;
        } else if (!(r.ok && r.error.empty())) {
            ++g.failed;
        }
    }
    ack.end_array();
    ack.key("ok");
    ack.boolean(all_ok);
    ack.key("ms");
    ack.integer(int64_t(now_ms() - t0));
    ack.end_object();

    st::write(outbox_path(b.seq), ack.text());
    st::remove(inbox_path(b.seq));
    g.last_ack = b.seq;
    g.force_render = true;
    log("batch %llu done: %zu/%zu ops ok in %llu ms", (unsigned long long)b.seq, n_ok, ops.a.size(),
        (unsigned long long)(now_ms() - t0));
}

void reject_batch(Batch &b)
{
    Writer ack;
    ack.begin_object();
    ack.key("seq");
    ack.integer(int64_t(b.seq));
    ack.key("boot");
    ack.integer(int64_t(g.boot));
    ack.key("t");
    ack.integer(int64_t(wall_ms()));
    ack.key("ok");
    ack.boolean(false);
    ack.key("error");
    ack.str(b.error);
    ack.key("results");
    ack.begin_array();
    ack.end_array();
    ack.end_object();
    st::write(outbox_path(b.seq), ack.text());
    st::remove(inbox_path(b.seq));
    g.last_ack = b.seq;
    ++g.failed;
    log("batch %llu rejected: %s", (unsigned long long)b.seq, b.error.c_str());
}

void drain_one()
{
    g.blocked_seq = 0;
    g.blocked_why.clear();
    if (g.pending.empty())
        return;
    auto it = g.pending.begin();
    Batch &b = it->second;
    if (!b.parsed)
        parse_batch(b);
    if (b.bad) {
        reject_batch(b);
        g.done.insert(b.seq);
        g.pending.erase(it);
        return;
    }
    std::string why = state::blocked(b.anytime);
    if (!why.empty()) {
        g.blocked_seq = b.seq;
        g.blocked_why = why;
        if (g.blocked_logged_seq != b.seq || g.blocked_logged_why != why) {
            log("batch %llu waits: %s", (unsigned long long)b.seq, why.c_str());
            g.blocked_logged_seq = b.seq;
            g.blocked_logged_why = why;
            g.force_render = true;
        }
        return;
    }
    uint64_t sz = 0;
    if (st::size(inbox_path(b.seq), &sz) != st::Result::ok) {
        log("batch %llu vanished before it ran", (unsigned long long)b.seq);
        g.pending.erase(it);
        return;
    }
    run_batch(b);
    g.done.insert(b.seq);
    g.pending.erase(it);
}

// ---- snapshot -----------------------------------------------------------------------------------

void render_section(SectionEntry &s, uint64_t now)
{
    {
        if (!s.quarantined.empty())
            return;
        SectionCall call{s.fn, {}, {}};
        Clock::time_point c0 = Clock::now();
        uint64_t t0 = now_ms();
        unsigned long code = 0;
        bool ok = guarded(run_section_body, &call, &code);
        uint64_t took = now_ms() - t0;
        double cost_ms = std::chrono::duration<double, std::milli>(Clock::now() - c0).count();
        g.pass_ms += took;
        if (took >= g.pass_max) {
            g.pass_max = took;
            g.pass_slowest = s.name;
        }
        if (!ok) {
            char b[96];
            std::snprintf(b, sizeof b, "fault 0x%08lx while rendering", code);
            s.quarantined = b;
            s.rendered = false;
            g.sections_dirty = true;
            log("section %s FAULTED (0x%08lx); it is switched off until the game restarts", s.name.c_str(), code);
            return;
        }
        if (took > 25 && now - s.slow_logged > 60000) {
            s.slow_logged = now;
            log("section %s took %llu ms to render", s.name.c_str(), (unsigned long long)took);
        }
        Json v;
        std::string before = std::move(s.text);
        bool was_rendered = s.rendered;
        if (!call.error.empty()) {
            Writer w;
            w.begin_object();
            w.key("error");
            w.str(call.error);
            w.end_object();
            s.text = w.text();
            json::parse(s.text, s.value);
        } else if (call.w.text().empty() || !json::parse(call.w.text(), v)) {
            Writer w;
            w.begin_object();
            w.key("error");
            w.str("the section wrote invalid JSON");
            w.end_object();
            s.text = w.text();
            json::parse(s.text, s.value);
            log("section %s wrote invalid JSON: %s", s.name.c_str(), clip(call.w.text(), 200).c_str());
        } else {
            s.text = call.w.text();
            s.value = std::move(v);
        }
        s.rendered = true;
        bool changed = !was_rendered || s.text != before;
        if (changed)
            g.sections_dirty = true;
        sched::rendered(s.slot, g.sched_clock, double(now), cost_ms, changed);
        g.pass_rendered = true;
        if (s.text.size() > 64 * 1024 && !s.big_logged) {
            s.big_logged = true;
            log("section %s is %zu KB; the snapshot is rewritten whole, so keep sections under 64 KB", s.name.c_str(),
                s.text.size() / 1024);
        }
    }
}

bool perf_on()
{
    static const bool on = [] {
        const char *v = std::getenv("SWOFTY_PERF");
        return v && *v && *v != '0';
    }();
    return on;
}

void finish_render()
{
    g.renders++;
    uint64_t now = now_ms();
    if (perf_on() && now - g.perf_logged >= 10000) {
        g.perf_logged = now;
        log("[perf] sections pass %llu: %llu ms in total, slowest %s %llu ms (one section per frame)",
            (unsigned long long)g.renders, (unsigned long long)g.pass_ms, g.pass_slowest.c_str(),
            (unsigned long long)g.pass_max);
    }
    g.pass_ms = g.pass_max = 0;
    std::string all;
    for (auto &s : section_table())
        if (s.rendered)
            all += s.name + '=' + s.text + '\n';
    if (all != g.sections_text) {
        g.sections_text = std::move(all);
        g.sections_at = wall_ms();
    }
}

std::string snapshot_body(uint64_t now)
{
    Writer w;
    w.begin_object();
    w.key("boot");
    w.integer(int64_t(g.boot));
    w.key("pid");
    w.integer(int64_t(pid()));
    w.key("addon");
    w.begin_object();
    w.key("id");
    w.str("swofty.career-editor");
    w.key("version");
    w.str(kVersion);
    w.key("api");
    w.integer(swofty::api ? swofty::api->version : 0);
    w.end_object();

    w.key("status");
    state::write_status(w, now);

    w.key("bridge");
    w.begin_object();
    w.key("pending");
    w.begin_array();
    for (auto &[seq, b] : g.pending)
        w.integer(int64_t(seq));
    w.end_array();
    w.key("blocked");
    if (g.blocked_seq) {
        w.begin_object();
        w.key("seq");
        w.integer(int64_t(g.blocked_seq));
        w.key("why");
        w.str(g.blocked_why);
        w.end_object();
    } else {
        w.null();
    }
    w.key("last_ack");
    if (g.last_ack)
        w.integer(int64_t(g.last_ack));
    else
        w.null();
    w.key("applied");
    w.integer(int64_t(g.applied));
    w.key("failed");
    w.integer(int64_t(g.failed));
    w.key("viewer");
    w.boolean(g.viewer_at && wall_ms() - g.viewer_at < kViewerFreshMs);
    w.key("renders");
    w.integer(int64_t(g.renders));
    w.key("ops");
    w.begin_array();
    for (auto &e : op_table()) {
        w.begin_object();
        w.key("name");
        w.str(e.name);
        w.key("anytime");
        w.boolean(e.anytime);
        w.key("off");
        if (e.quarantined.empty())
            w.null();
        else
            w.str(e.quarantined);
        w.end_object();
    }
    w.end_array();
    w.key("sections");
    w.begin_array();
    for (auto &s : section_table()) {
        w.begin_object();
        w.key("name");
        w.str(s.name);
        w.key("off");
        if (s.quarantined.empty())
            w.null();
        else
            w.str(s.quarantined);
        w.end_object();
    }
    w.end_array();
    w.key("duplicates");
    w.begin_array();
    for (auto &d : duplicates())
        w.str(d);
    w.end_array();
    w.end_object();

    w.key("sections_live");
    w.boolean(g.sections_live);
    w.key("sections_at");
    if (g.sections_at)
        w.integer(int64_t(g.sections_at));
    else
        w.null();
    w.key("sections");
    if (g.sections_dirty) {
        Writer all;
        all.begin_object();
        for (auto &s : section_table()) {
            if (!s.rendered)
                continue;
            all.key(s.name.c_str());
            emit(all, s.value);
        }
        all.end_object();
        g.sections_json = all.text();
        g.sections_dirty = false;
    }
    return w.text() + g.sections_json + '}';
}

struct SnapshotWriter {
    std::mutex lock;
    std::condition_variable wake, idle;
    std::string pending;
    bool has = false, busy = false, started = false;
    std::thread worker;

    void run()
    {
        std::unique_lock<std::mutex> l(lock);
        for (;;) {
            wake.wait(l, [this] { return has; });
            std::string out = std::move(pending);
            has = false;
            busy = true;
            l.unlock();
            st::write("snapshot.json", out);
            l.lock();
            busy = false;
            idle.notify_all();
        }
    }

    void post(std::string out)
    {
        std::lock_guard<std::mutex> l(lock);
        if (!started) {
            started = true;
            worker = std::thread([this] { run(); });
            worker.detach();
        }
        pending = std::move(out);
        has = true;
        wake.notify_one();
    }

    void drain()
    {
        std::unique_lock<std::mutex> l(lock);
        idle.wait(l, [this] { return !has && !busy; });
    }
};

SnapshotWriter &writer()
{
    static SnapshotWriter *w = new SnapshotWriter;
    return *w;
}

bool viewer_fresh()
{
    g.viewer_at = watcher().viewer_at;
    uint64_t wall = wall_ms();
    bool open = g.viewer_at && wall >= g.viewer_at && wall - g.viewer_at < kViewerFreshMs;
    bool front = watcher().game_in_front;
    int now = open ? front ? 2 : 1 : 0;
    static int was = -1;
    if (was != now) {
        was = now;
        log(now == 1   ? "launcher page is open: sections render, one per frame, every %llu ms"
            : now == 2 ? "launcher page is open but the game is in front: sections are not rendered (%llu ms cadence "
                         "paused until another window is in front; status heartbeat only)"
                       : "launcher page is not open: sections are not rendered (%llu ms cadence paused; status heartbeat "
                         "only)",
            (unsigned long long)kSnapshotMs);
    }
    return now == 1;
}

void write_snapshot(uint64_t now)
{
    std::string body = snapshot_body(now);
    if (body == g.last_body && now - g.last_write < kHeartbeatMs)
        return;
    std::string out = "{\"v\":1,\"written\":" + std::to_string(wall_ms()) + "," + body.substr(1);
    g.last_body = std::move(body);
    g.last_write = now;
    writer().post(std::move(out));
}

void invalidate_sections()
{
    for (auto &s : section_table())
        sched::invalidate(s.slot);
    g.sched_clock.global_ms = 0;
}

void snapshot(uint64_t now, bool force)
{
    bool live = state::sections_ok();
    const char *phase = state::phase();
    bool phase_changed = std::strcmp(phase, g.last_phase) != 0;
    g.last_phase = phase;
    g.sections_live = live;
    if (phase_changed || (live && !g.was_live))
        invalidate_sections();
    g.was_live = live;
    if (!g.sections_live && !in_career())
        for (auto &s : section_table())
            if (s.rendered) {
                s.rendered = false;
                g.sections_dirty = true;
            }
    bool viewer = g.sections_live && viewer_fresh();
    if (viewer && !g.was_viewer)
        invalidate_sections();
    g.was_viewer = viewer;
    if (!viewer) {
        write_snapshot(now);
        return;
    }
    if (force) {
        for (auto &s : section_table())
            render_section(s, now);
        g.pass_rendered = false;
        finish_render();
        write_snapshot(now);
        return;
    }
    g.pass_active = true;
    g.pass_index = 0;
    g.pass_rendered = false;
}

void continue_pass(uint64_t now)
{
    auto &t = section_table();
    while (g.pass_index < t.size() && !t[g.pass_index].quarantined.empty())
        ++g.pass_index;
    while (g.pass_index < t.size() && !sched::may_render(t[g.pass_index].slot, g.sched_clock, double(now)))
        ++g.pass_index;
    if (g.pass_index < t.size()) {
        if (Clock::now() < g.next_section)
            return;
        render_section(t[g.pass_index++], now);
        g.next_section = Clock::now() + kSectionGap;
    }
    if (g.pass_index >= t.size()) {
        g.pass_active = false;
        if (g.pass_rendered)
            finish_render();
        g.pass_rendered = false;
        write_snapshot(now);
    }
}

enum Part { PartState, PartInbox, PartBatch, PartSnapshot, PartCount };

struct FramePerf {
    uint64_t calls = 0;
    uint64_t logged = 0;
    double total = 0;
    double max = 0;
    double part[PartCount] = {};
    Clock::time_point lap;
};

FramePerf g_perf;

void perf_lap(Part p)
{
    Clock::time_point t = Clock::now();
    double ms = std::chrono::duration<double, std::milli>(t - g_perf.lap).count();
    g_perf.part[p] = std::max(g_perf.part[p], ms);
    g_perf.lap = t;
}

void perf_frame(Clock::time_point start, uint64_t now)
{
    double ms = std::chrono::duration<double, std::milli>(Clock::now() - start).count();
    g_perf.calls++;
    g_perf.total += ms;
    g_perf.max = std::max(g_perf.max, ms);
    if (!g_perf.logged)
        g_perf.logged = now;
    if (now - g_perf.logged < 10000)
        return;
    log("[perf] frame callback: %llu call(s) in %.1f s, %.3f ms in total, slowest %.3f ms; slowest part: state %.3f, "
        "inbox %.3f, batches %.3f, snapshot %.3f ms (page %s)",
        (unsigned long long)g_perf.calls, double(now - g_perf.logged) / 1000.0, g_perf.total, g_perf.max,
        g_perf.part[PartState], g_perf.part[PartInbox], g_perf.part[PartBatch], g_perf.part[PartSnapshot],
        watcher().viewer_at && wall_ms() - watcher().viewer_at < kViewerFreshMs ? "open" : "closed");
    g_perf = FramePerf{};
    g_perf.logged = now;
}

} // namespace

void register_idle(IdleFn fn)
{
    idle_table().push_back(fn);
}

namespace bridge {

const Json *section(const char *name)
{
    for (auto &s : section_table())
        if (s.rendered && s.name == name)
            return &s.value;
    return nullptr;
}

void start()
{
    g.boot = wall_ms();
    g.storage = st::available();
    std::string ops, anytime, sections;
    size_t n_anytime = 0;
    for (auto &e : op_table()) {
        std::string &list = e.anytime ? anytime : ops;
        list += (list.empty() ? "" : " ") + e.name;
        n_anytime += e.anytime;
    }
    for (auto &s : section_table())
        sections += (sections.empty() ? "" : " ") + s.name;
    log("bridge v%s: %zu Team HQ op(s) [%s], %zu anytime op(s) [%s], %zu section(s) [%s]", kVersion,
        op_table().size() - n_anytime, ops.c_str(), n_anytime, anytime.c_str(), section_table().size(),
        sections.c_str());
    for (auto &d : duplicates())
        log("duplicate registration ignored: %s", d.c_str());
    if (!g.storage) {
        log("storage library unavailable: the mailbox is off (enable the storage feature)");
        return;
    }
    st::write("inbox/.keep", std::string_view());
    st::write("outbox/.keep", std::string_view());
    init_doorbell();
#if defined(_WIN32)
    log("mailbox ready in swofty/addons/swofty.career-editor/ (inbox doorbell %s)",
        watcher().doorbell ? "on" : "off, listing every 0.5 s");
#endif
    std::string names;
    if (read_inbox(names))
        merge_inbox(names);
    watcher().names = std::move(names);
    poll_viewer(watcher());
    std::thread(watch).detach();
    g.up = true;
}

void frame()
{
    static std::atomic_flag busy = ATOMIC_FLAG_INIT;
    if (!g.up || busy.test_and_set(std::memory_order_acquire))
        return;
    struct Done {
        ~Done() { busy.clear(std::memory_order_release); }
    } done;
    bool perf = perf_on();
    Clock::time_point start = perf ? Clock::now() : Clock::time_point{};
    g_perf.lap = start;
    uint64_t now = now_ms();
    if (now - g.last_state >= kStatePollMs) {
        g.last_state = now;
        state::poll(now);
    }
    if (perf)
        perf_lap(PartState);
    take_inbox();
    if (perf)
        perf_lap(PartInbox);
    drain_one();
    if (perf)
        perf_lap(PartBatch);
    if (g.force_render) {
        g.force_render = false;
        g.pass_active = false;
        g.last_snapshot = now;
        snapshot(now, true);
    } else if (g.pass_active) {
        continue_pass(now);
    } else if (now - g.last_snapshot >= kSnapshotMs) {
        g.last_snapshot = now;
        snapshot(now, false);
        if (g.pass_active)
            continue_pass(now);
    }
    if (g.was_viewer && g.sections_live)
        for (IdleFn fn : idle_table())
            if (fn() == 2)
                invalidate_sections();
    if (perf) {
        perf_lap(PartSnapshot);
        perf_frame(start, now);
    }
}

void flush_writes()
{
    writer().drain();
}

} // namespace bridge

} // namespace ce
