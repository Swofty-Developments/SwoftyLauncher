#include "ghost_arm_logic.hpp"
#include "ghost_request_logic.hpp"
#include "ghost_swlap_logic.hpp"
#include "hooks/guard.hpp"
#include "lap_assists_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include "apply/moments.hpp"

#include <safetyhook.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <memory>
#include <mutex>
#include <thread>
#include <string>
#include <vector>

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
std::string hook_error(const safetyhook::InlineHook::Error &e);
}

// Playing a stored lap as the rival ghost of a Time Trial session (re/db: game.trace_read_from_stream,
// game.telemetry_laps_*). Nothing here is proven in game; every stage logs one [ghost] line.
//
// The game keeps six telemetry laps ("TLap", 0x2d0 bytes each) in the vector at +0x50 of the telemetry component (the
// component with key 0x1461026a8): 0 records the lap in progress, 1 holds the last completed lap, 2 the personal best
// being raced, 3 the stored personal best, 4 the rival, 5 unknown. A TLap holds a trace (ghost_trace_logic.hpp) and, when
// it is bound to a ghost car (+0x130 non-null), the component's per-frame tick (FUN_1437b2410) drives that car from it
// while the TLap is in play mode (+0x23c == 2); the game's lap-start routine puts every TLap but 0 in play mode.
//
// Arming mirrors what Game_GhostLapHandler_OnSessionRivalSearchCompleted does after a rival download: reset the TLap
// (FUN_1437ab6b0), then Game_Trace_ReadFromStream(tlap, stream) with the trace bytes, then check +0x250. The stream is
// the game's memory stream: {vtable 0x145899750, data, size, 0}.
//
// Gate: "tt_ghost": true in swofty\tt.json, and never with SWOFTY_TT_GHOST=0. The request is swofty\tt\ghost\play.json
// {"lap": "<.swlap or .trace path>", "label": "<driver name>"}. SWOFTY_TT_DIAG=1 adds read-only logging of the trace
// reader, the six laps and the trace of every completed lap (dumped to swofty\tt\ghost\diag-<n>.trace); it changes no
// behaviour. SWOFTY_TT_GHOST_SLOT picks another telemetry lap than 4. The tick hook and the play.json poller do nothing
// unless a Time Trial session is live (runtime::session_kind()).

namespace runtime {
void player_name_probe_tick();
void player_name_pin_tick();
}

namespace {

namespace mem = swofty::mem;
namespace gt = runtime::ghost_trace;

const runtime::Sig S_TICK{"Game_TelemetryLaps_Tick",
                          "41 57 41 56 41 55 41 54 56 57 53 48 81 EC A0 00 00 00 C5 F8 29 BC 24 90 00 00 00 C5 F8 29 B4 24 80 "
                          "00 00 00 80 B9 81 0A 00 00 00 0F 84",
                          0x37b2410};
const runtime::Sig S_NEWLAP{"Game_TelemetryLaps_OnNewLapRecorded",
                            "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 58 C5 F8 29 74 24 40 80 B9 81 0A 00 00 00 0F 84",
                            0x37b0aa0};
const runtime::Sig S_READ{"Game_Trace_ReadFromStream",
                          "55 41 57 41 56 41 55 41 54 56 57 53 48 81 EC B8 00 00 00 48 8D AC 24 80 00 00 00 C5 78 29 75 20 C5 "
                          "78 29 6D 10 C5 78 29 65 00 C5 79 7F 5D F0",
                          0x37ad610};
const runtime::Sig S_RESET{"Game_TelemetryLap_Reset",
                           "56 48 83 EC 20 48 89 CE C5 F8 57 C0 C5 F8 11 81 68 02 00 00 80 A1 C0 02 00 00 F0 C5 F8 29 81 F0 01 "
                           "00 00 31 C0 48 83 B9 E0 01 00 00 00 0F 94 C0",
                           0x37ab6b0};

constexpr uintptr_t RVA_STREAM_VTABLE = 0x5899750;
constexpr size_t COMP_LAPS = 0x50, TL_VEHICLE = 0x130, TL_BUFFER = 0x1e0,
                 TL_HIGH = 0x1f8, TL_COUNT = 0x214, TL_MODE = 0x23c, TL_WANT = 0x240, TL_TIME = 0x24c,
                 TL_ERROR = 0x250, TL_CURVE = 0x254, TL_SUMMARY = 0x260, TL_LAP = 0x268, TL_FLAGS = 0x2c0;
constexpr int SLOTS = 6, MAX_DUMPS = 8;
constexpr uintptr_t DIAG_POLL_MS = 1000;

using ReadFn = int (*)(void *tlap, void *stream);
using ResetFn = void (*)(void *tlap);
using NewLapFn = uint64_t (*)(void *comp, void *msg);

struct Stream {
    const void *vtable;
    const void *data;
    uint64_t size;
    uint64_t pos;
};

safetyhook::MidHook h_tick;
safetyhook::InlineHook h_read, h_newlap;
ResetFn reset_fn = nullptr;
ReadFn read_fn = nullptr;
std::atomic<uintptr_t> component{0};
std::atomic<int> diag_reads{0};
bool ghost_on = false, diag_on = false, record_on = false;
std::atomic<bool> trace_armed{false};
int input_hist[4] = {};
int64_t last_input_sample = 0;
int slot_env = -1, slot = 4;
std::vector<uint8_t> saved_blob;
bool saved_has = false;
uintptr_t saved_for = 0;

struct Request {
    std::string path, label, driver, team;
    bool replay = false;
    runtime::ghost_request::Request meta;
    int64_t read_unix = 0;
    std::vector<uint8_t> blob;
    std::string kind;
    uint64_t stamp = 0;
    size_t records = 0;
    float lap_s = 0;
};

std::mutex pub_lock;
std::shared_ptr<const Request> published;
uint64_t pub_gen = 0;
std::atomic<bool> worker_run{false};
runtime::ghost_arm::Arming arming;
float armed_lap_s = 0;
int64_t last_diag = 0;
std::string last_slots;
int dumps = 0;

int64_t now_ms() { return int64_t(GetTickCount64()); }

bool env_is(const char *name, const char *value)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n && n < sizeof v && !strcmp(v, value);
}

std::string hex(const uint8_t *p, size_t n)
{
    std::string s;
    char b[4];
    for (size_t i = 0; i < n; i++) {
        snprintf(b, sizeof b, "%02x", p[i]);
        s += b;
    }
    return s;
}

std::wstring ghost_dir()
{
    std::wstring root = runtime::swofty_root();
    CreateDirectoryW((root + L"tt").c_str(), nullptr);
    std::wstring d = root + L"tt\\ghost\\";
    CreateDirectoryW(d.c_str(), nullptr);
    return d;
}

uintptr_t tlap_at(uintptr_t comp, int index)
{
    uintptr_t begin = 0, end = 0, tl = 0;
    if (!mem::read(comp + COMP_LAPS, begin) || !mem::read(comp + COMP_LAPS + 8, end) || !begin || end <= begin ||
        (end - begin) % 8 || uint64_t(index) >= (end - begin) / 8 || !mem::read(begin + 8 * uintptr_t(index), tl) || !tl ||
        !mem::readable(tl, 0x2d0))
        return 0;
    return tl;
}

int slot_of(uintptr_t comp, uintptr_t tl)
{
    for (int i = 0; i < SLOTS; i++)
        if (tlap_at(comp, i) == tl)
            return i;
    return -1;
}

struct LapState {
    uintptr_t bound = 0;
    uint32_t mode = 0, want = 0, error = 0, count = 0, curve = 0, flags = 0;
    uint64_t bytes = 0;
    float lap = 0, time = 0;
};

LapState state_of(uintptr_t tl)
{
    LapState s;
    mem::read(tl + TL_VEHICLE, s.bound);
    mem::read(tl + TL_MODE, s.mode);
    mem::read(tl + TL_WANT, s.want);
    mem::read(tl + TL_ERROR, s.error);
    mem::read(tl + TL_COUNT, s.count);
    mem::read(tl + TL_CURVE, s.curve);
    mem::read(tl + TL_FLAGS, s.flags);
    mem::read(tl + TL_HIGH, s.bytes);
    mem::read(tl + TL_LAP, s.lap);
    mem::read(tl + TL_TIME, s.time);
    return s;
}

std::string key_of(uintptr_t comp)
{
    std::string out;
    char b[40];
    for (int i = 0; i < SLOTS; i++) {
        uintptr_t tl = tlap_at(comp, i);
        if (!tl) {
            out += "-";
            continue;
        }
        LapState s = state_of(tl);
        snprintf(b, sizeof b, "%c%u/%u ", s.bound ? 'B' : 'f', s.mode, s.want);
        out += b;
    }
    return out;
}

std::string describe(uintptr_t comp)
{
    std::string out;
    char b[200];
    for (int i = 0; i < SLOTS; i++) {
        uintptr_t tl = tlap_at(comp, i);
        if (!tl) {
            snprintf(b, sizeof b, "%s%d:-", i ? " " : "", i);
            out += b;
            continue;
        }
        LapState s = state_of(tl);
        snprintf(b, sizeof b, "%s%d:%s m%u/%u e%u n%u b%llu lap%.3f f%x", i ? " " : "", i, s.bound ? "bound" : "free", s.mode, s.want,
                 s.error, s.count, (unsigned long long)s.bytes, s.lap, s.flags);
        out += b;
    }
    return out;
}

bool trace_bytes_of(uintptr_t tl, std::vector<uint8_t> &out)
{
    uint64_t high = 0, buffer = 0;
    uint32_t error = 0, count = 0;
    if (!mem::read(tl + TL_HIGH, high) || !mem::read(tl + TL_BUFFER, buffer) || !mem::read(tl + TL_ERROR, error) ||
        !mem::read(tl + TL_COUNT, count) || error != 0 || !buffer || !count || high == 0 || high > gt::MAX_DATA ||
        !mem::readable(uintptr_t(buffer), size_t(high)) || !mem::readable(tl + TL_SUMMARY, gt::SUMMARY))
        return false;
    out.clear();
    gt::put<uint64_t>(out, gt::FIXED + high);
    out.insert(out.end(), reinterpret_cast<const uint8_t *>(tl + TL_SUMMARY),
               reinterpret_cast<const uint8_t *>(tl + TL_SUMMARY) + gt::SUMMARY);
    out.insert(out.end(), gt::PAD, 0);
    out.insert(out.end(), reinterpret_cast<const uint8_t *>(buffer), reinterpret_cast<const uint8_t *>(buffer) + high);
    return true;
}

void report_trace(const char *what, const std::vector<uint8_t> &blob)
{
    gt::Trace t;
    gt::Status st = gt::decode(blob.data(), blob.size(), t);
    if (st != gt::Status::ok || t.records.empty()) {
        swofty::log("[ghost] %s: %zu bytes, does not decode as a trace (%s)", what, blob.size(), gt::status_name(st));
        return;
    }
    const gt::Record &first = t.records.front(), &last = t.records.back();
    uint32_t seen = 0;
    for (auto &r : t.records)
        seen |= r.mask;
    swofty::log("[ghost] %s: %zu bytes, %zu records, %u..%u ms, channels 0x%x, lap %.3f s, first position %.2f %.2f %.2f, first quaternion "
                "%d %d %d %d, summary %s",
                what, blob.size(), t.records.size(), first.ms, last.ms, seen, gt::lap_seconds(t), first.pos[0], first.pos[1], first.pos[2],
                first.quat[0], first.quat[1], first.quat[2], first.quat[3],
                hex(blob.data() + 8, gt::SUMMARY).c_str());
}

void dump(const std::vector<uint8_t> &blob)
{
    if (dumps >= MAX_DUMPS)
        return;
    std::wstring path = ghost_dir() + runtime::wide("diag-" + std::to_string(now_ms()) + ".trace");
    std::string bytes(reinterpret_cast<const char *>(blob.data()), blob.size());
    if (runtime::write_file(path, bytes)) {
        dumps++;
        swofty::log("[ghost] diag: wrote the game's trace of the last lap (%zu bytes) to tt\\ghost", blob.size());
    }
}

bool load_request(const std::wstring &play, Request &out, std::string &why)
{
    std::string text = runtime::read_file(play, size_t(1) << 16);
    runtime::ghost_request::Request req;
    auto rst = runtime::ghost_request::parse(text, req);
    if (rst != runtime::ghost_request::Status::ok) {
        why = runtime::ghost_request::status_name(rst);
        return false;
    }
    out.path = req.lap;
    out.label = req.label;
    out.replay = req.mode == runtime::ghost_request::Mode::replay;
    out.driver = req.driver;
    out.team = req.team;
    out.meta = req;
    out.read_unix = int64_t(time(nullptr));
    // A bare file name is a file next to play.json; the launcher cannot know the game's view of an absolute path.
    bool bare = out.path.find_first_of(":/\\") == std::string::npos;
    std::wstring file = bare ? ghost_dir() + runtime::wide(out.path) : runtime::wide(out.path);
    std::string bytes = runtime::read_file(file, size_t(64) << 20);
    if (bytes.empty()) {
        why = "the lap file is missing or empty";
        return false;
    }
    const uint8_t *p = reinterpret_cast<const uint8_t *>(bytes.data());
    if (bytes.size() >= 8 && !memcmp(p, runtime::lap_file::MAGIC, sizeof runtime::lap_file::MAGIC)) {
        out.kind = "swlap";
        runtime::lap_file::Parsed parsed;
        auto st = runtime::lap_file::parse(p, bytes.size(), parsed);
        if (st != runtime::lap_file::Status::ok) {
            why = "the lap file does not parse";
            return false;
        }
        std::string w;
        std::string lap_track = runtime::ghost_request::lower(std::string(parsed.header["board"]["track"].as_string()));
        if (!lap_track.empty() && lap_track != out.meta.track) {
            why = "the lap file is for track '" + lap_track + "', the request names '" + out.meta.track + "'";
            return false;
        }
        auto trace = gt::trace_of_lap(parsed, &w);
        if (!trace) {
            why = "the lap does not convert: " + w;
            return false;
        }
        if (!gt::encode(*trace, out.blob, &w)) {
            why = "the converted trace does not encode: " + w;
            return false;
        }
    } else {
        out.kind = "trace";
        out.blob.assign(p, p + bytes.size());
    }
    gt::Trace t;
    gt::Status st = gt::decode(out.blob.data(), out.blob.size(), t);
    if (st != gt::Status::ok) {
        why = std::string("the trace does not decode: ") + gt::status_name(st);
        return false;
    }
    if (!gt::accepted_when_bound(t)) {
        why = "the trace would fail the game's checks for a bound lap";
        return false;
    }
    out.records = t.records.size();
    out.lap_s = gt::lap_seconds(t);
    return true;
}

void publish(std::shared_ptr<const Request> r, uint64_t gen)
{
    std::lock_guard<std::mutex> g(pub_lock);
    published = std::move(r);
    pub_gen = gen;
}

void drop_stale_request(const std::wstring &play)
{
    WIN32_FILE_ATTRIBUTE_DATA d{};
    FILETIME created, exited, kernel, user;
    if (!GetFileAttributesExW(play.c_str(), GetFileExInfoStandard, &d) ||
        !GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user))
        return;
    if (CompareFileTime(&d.ftLastWriteTime, &created) < 0) {
        DeleteFileW(play.c_str());
        swofty::log("[ghost] stale play.json (older than this game process) deleted");
    }
}

void worker()
{
    uint64_t gen = 0;
    std::wstring play = ghost_dir() + L"play.json", done = ghost_dir() + L"play.done.json";
    drop_stale_request(play);
    while (worker_run.load()) {
        Sleep(500);
        if (!runtime::session_life::time_trial_live(runtime::session_kind()))
            continue;
        WIN32_FILE_ATTRIBUTE_DATA d{};
        if (!GetFileAttributesExW(play.c_str(), GetFileExInfoStandard, &d))
            continue;
        auto r = std::make_shared<Request>();
        std::string why;
        bool ok = load_request(play, *r, why);
        MoveFileExW(play.c_str(), done.c_str(), MOVEFILE_REPLACE_EXISTING);
        if (!ok) {
            publish(nullptr, 0);
            swofty::log("[ghost] request refused and consumed: %s", why.c_str());
            continue;
        }
        swofty::log("[ghost] request consumed (play.done.json): mode %s, %s lap '%s', driver '%s' team '%s', board track '%s': %zu records, "
                    "%zu bytes, lap %.3f s; it arms only in a Time Trial on that track",
                    r->replay ? "replay" : "ghost", r->kind.c_str(), r->path.c_str(),
                    r->meta.driver.empty() ? r->label.c_str() : r->meta.driver.c_str(), r->meta.team.c_str(), r->meta.track.c_str(),
                    r->records, r->blob.size(), r->lap_s);
        publish(r, ++gen);
    }
}

bool read_trace_into(uintptr_t tl, const std::vector<uint8_t> &blob)
{
    Stream stream{nullptr, blob.data(), blob.size(), 0};
    stream.vtable = reinterpret_cast<const void *>(swofty::api->module_base() + RVA_STREAM_VTABLE);
    reset_fn(reinterpret_cast<void *>(tl));
    int rc = read_fn(reinterpret_cast<void *>(tl), &stream);
    LapState s = state_of(tl);
    swofty::log("[ghost] arm: reader returned %d, lap %d count %u bytes %llu time %.3f error %u mode %u", rc, slot, s.count,
                (unsigned long long)s.bytes, s.lap, s.error, s.mode);
    return rc == 0 && (s.error & ~2u) == 0 && s.count > 0;
}

runtime::ghost_arm::Views views_of(uintptr_t comp)
{
    runtime::ghost_arm::Views v;
    for (int i = 0; i < SLOTS; i++) {
        uintptr_t tl = tlap_at(comp, i);
        if (!tl)
            continue;
        LapState s = state_of(tl);
        auto &o = v.s[i];
        o.present = true;
        o.bound = s.bound != 0;
        o.mode = s.mode;
        o.want = s.want;
        o.error = s.error;
        o.count = s.count;
    }
    return v;
}

void do_restore(uintptr_t comp)
{
    swofty::log("[ghost] restore: stage 1, checking lap 3");
    uintptr_t tl = tlap_at(comp, 3);
    if (tl && saved_for == comp) {
        LapState s = state_of(tl);
        if (std::fabs(s.lap - armed_lap_s) > 0.002f)
            swofty::log("[ghost] restore: lap 3 no longer holds the supplied trace (time %.3f); left as is", s.lap);
        else if (saved_has) {
            read_trace_into(tl, saved_blob);
            swofty::log("[ghost] restore: the player's own personal best trace is back in lap 3");
        } else {
            reset_fn(reinterpret_cast<void *>(tl));
            swofty::log("[ghost] restore: lap 3 reset, the player had no personal best trace");
        }
    }
    saved_for = 0;
    arming.restored();
}

void do_arm(uintptr_t comp, int pick, const Request &req)
{
    slot = pick;
    uintptr_t tl = tlap_at(comp, pick);
    if (!tl) {
        arming.failed(now_ms());
        return;
    }
    LapState before = state_of(tl);
    swofty::log("[ghost] arm: stage 1, lap %d at 0x%llx, bound 0x%llx, mode %u/%u, count %u; %s", pick, (unsigned long long)tl,
                (unsigned long long)before.bound, before.mode, before.want, before.count, describe(comp).c_str());
    if (pick == 3 && saved_for != comp) {
        saved_has = trace_bytes_of(tl, saved_blob);
        saved_for = comp;
        swofty::log("[ghost] arm: stage 2, lap 3 is the player's personal best slot; its trace is %s", saved_has ? "kept for the restore" : "empty");
    }
    swofty::log("[ghost] arm: stage 3, reading '%s' into lap %d", req.label.c_str(), pick);
    if (!read_trace_into(tl, req.blob)) {
        swofty::log("[ghost] arm: failed");
        arming.failed(now_ms());
        return;
    }
    armed_lap_s = req.lap_s;
    arming.armed_ok(pick);
    swofty::log("[ghost] armed: '%s' (%.3f s) is lap %d's trace; it plays from the next lap start (the stock lap-start routine sets play mode)", req.label.c_str(), req.lap_s, pick);
}

void game_step(uintptr_t comp)
{
    std::shared_ptr<const Request> req;
    uint64_t gen = 0;
    {
        std::unique_lock<std::mutex> l(pub_lock, std::try_to_lock);
        if (!l.owns_lock())
            return;
        req = published;
        gen = pub_gen;
    }
    arming.request(req ? gen : 0);
    auto d = arming.decide(views_of(comp), req && req->replay ? 3 : slot_env, now_ms());
    static std::string last_why;
    static bool playing = false;
    if (req && last_why != arming.why()) {
        last_why = arming.why();
        swofty::log("[ghost] request pending: %s", last_why.c_str());
    }
    if (arming.armed() && arming.slot() >= 0) {
        uintptr_t tl = tlap_at(comp, arming.slot());
        LapState s = tl ? state_of(tl) : LapState{};
        uint16_t live_flag = 0;
        if (tl)
            mem::read(tl + 0x2cc, live_flag);
        bool now_playing = tl && s.mode == 2 && s.count > 0 && std::fabs(s.lap - armed_lap_s) < 0.002f;
        if (now_playing != playing) {
            playing = now_playing;
            swofty::log("[ghost] %s: lap %d mode %u/%u count %u time %.3f t %.3f flag2cc %u bound 0x%llx", playing ? "PLAYBACK STARTED" : "playback stopped",
                        arming.slot(), s.mode, s.want, s.count, s.lap, s.time, unsigned(live_flag), (unsigned long long)s.bound);
        }
    } else
        playing = false;
    if (d.what == runtime::ghost_arm::Do::arm && req) {
        runtime::ghost_request::Session session;
        session.time_trial = runtime::lap_rec_current_board(session.content, session.track, session.weather);
        auto verdict = runtime::ghost_request::check(req->meta, session, int64_t(time(nullptr)), req->read_unix);
        if (verdict != runtime::ghost_request::Verdict::ok) {
            swofty::log("[ghost] request discarded: %s (requested track '%s', session track '%s', time trial %d)",
                        runtime::ghost_request::verdict_name(verdict), req->meta.track.c_str(), session.track.c_str(),
                        session.time_trial ? 1 : 0);
            publish(nullptr, 0);
            return;
        }
        do_arm(comp, d.slot, *req);
    }
    else if (d.what == runtime::ghost_arm::Do::restore)
        do_restore(comp);
}

int car_input(uintptr_t comp, uintptr_t *car_out = nullptr)
{
    uintptr_t tl = tlap_at(comp, 0), car = 0;
    uint32_t code = 0xff;
    if (!tl || !mem::read(tl + TL_VEHICLE, car) || !car || !mem::read(car + runtime::lap_assists::CAR_INPUT, code))
        return -1;
    if (car_out)
        *car_out = car;
    return code < 4 ? int(code) : -1;
}

void sample_input(uintptr_t comp)
{
    int64_t now = now_ms();
    if (now - last_input_sample < 250)
        return;
    last_input_sample = now;
    int code = car_input(comp);
    if (code >= 0)
        input_hist[code]++;
}

void pb_rearm_tick(uintptr_t comp);

void tick_mid(safetyhook::Context &c)
{
    static thread_local bool busy = false;
    uintptr_t comp = c.rcx;
    if (!comp || busy || !runtime::session_life::time_trial_live(runtime::session_kind()))
        return;
    busy = true;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    if (component.load() != comp) {
        component = comp;
        swofty::log("[ghost] telemetry component 0x%llx", (unsigned long long)comp);
        last_slots.clear();
        if (arming.armed())
            publish(nullptr, 0);
        arming.component(comp);
    }
    runtime::player_name_pin_tick();
    if (ghost_on)
        pb_rearm_tick(comp);
    if (record_on)
        sample_input(comp);
    if (ghost_on)
        game_step(comp);
    if (diag_on) {
        runtime::player_name_probe_tick();
        int64_t now = now_ms();
        if (now - last_diag >= int64_t(DIAG_POLL_MS)) {
            last_diag = now;
            std::string k = key_of(comp);
            if (k != last_slots) {
                last_slots = k;
                swofty::log("[ghost] diag: laps %s", describe(comp).c_str());
            }
        }
    }
    busy = false;
}

int read_hook(void *tlap, void *stream)
{
    uintptr_t comp = component.load();
    int which = comp ? slot_of(comp, uintptr_t(tlap)) : -1;
    uint64_t size = 0, n = 0;
    uintptr_t data = 0;
    if (diag_on && stream && mem::read(uintptr_t(stream) + 8, data) && mem::read(uintptr_t(stream) + 16, size) && data &&
        mem::readable(data, 8))
        mem::read(data, n);
    int rc = h_read.call<int>(tlap, stream);
    if (diag_on && diag_reads.fetch_add(1) < 40) {
        LapState s = state_of(uintptr_t(tlap));
        swofty::log("[ghost] diag: trace reader called on lap %d (0x%llx): stream %llu bytes, size word %llu, returned %d; now count %u, bytes %llu, "
                    "time %.3f, error %u",
                    which, (unsigned long long)uintptr_t(tlap), (unsigned long long)size, (unsigned long long)n, rc, s.count,
                    (unsigned long long)s.bytes, s.lap, s.error);
    }
    return rc;
}

struct PbKey {
    uint32_t count = 0;
    uint64_t bytes = 0;
    float lap = 0;
    bool operator==(const PbKey &o) const { return count == o.count && bytes == o.bytes && lap == o.lap; }
};

PbKey pb_key_of(uintptr_t tl)
{
    PbKey k;
    if (!tl)
        return k;
    LapState s = state_of(tl);
    k.count = s.count;
    k.bytes = s.bytes;
    k.lap = s.lap;
    return k;
}

std::mutex pb_lock;
bool pb_pending = false, pb_rearm_on = true;
uintptr_t pb_comp = 0;
PbKey pb_key, pb_done;
std::vector<uint8_t> pb_blob;
int64_t pb_rearmed_at = 0, pb_recorded_at = 0;
int pb_watch = 0;

void pb_rearm_tick(uintptr_t comp)
{
    int64_t now = now_ms();
    if (pb_watch > 0 && pb_rearmed_at && now - pb_rearmed_at >= (pb_watch == 2 ? 1500 : 6000)) {
        uintptr_t tl = tlap_at(comp, 3);
        LapState s = tl ? state_of(tl) : LapState{};
        swofty::log("[ghost] new PB watch: lap 3 %.1f s after the re-arm: mode %u/%u, time %.3f of %.3f, count %u, bound 0x%llx", (now - pb_rearmed_at) / 1000.0,
                    s.mode, s.want, s.time, s.lap, s.count, (unsigned long long)s.bound);
        pb_watch--;
    }
    std::vector<uint8_t> blob;
    PbKey key;
    int64_t recorded = 0;
    {
        std::lock_guard<std::mutex> g(pb_lock);
        if (!pb_pending || pb_comp != comp)
            return;
        key = pb_key;
        recorded = pb_recorded_at;
        if (pb_key == pb_done) {
            pb_pending = false;
            return;
        }
        blob = pb_blob;
    }
    uintptr_t tl3 = tlap_at(comp, 3), tl1 = tlap_at(comp, 1);
    if (!pb_rearm_on || !reset_fn || !read_fn || !tl3 || !tl1) {
        std::lock_guard<std::mutex> g(pb_lock);
        pb_pending = false;
        return;
    }
    if (swofty_core::apply::moments::session_mode_track().first != "time_trial" || !runtime::tt_bridge_ours_session()) {
        std::lock_guard<std::mutex> g(pb_lock);
        pb_pending = false;
        return;
    }
    int64_t waited = now - recorded;
    LapState before = state_of(tl3);
    // The game's own handler resets lap 3 a few milliseconds after the new PB (Game_GhostLapHandler_Reset on the commit path) and
    // reloads it from the download; arming before that reset is wiped. Wait for the wipe (count 0) or 600 ms, then arm.
    if (before.count > 0 && waited < 600)
        return;
    if (waited > 30000) {
        swofty::log("[ghost] new PB re-arm dropped: %.1f s since the lap, too late to start in sync", waited / 1000.0);
        std::lock_guard<std::mutex> g(pb_lock);
        pb_pending = false;
        return;
    }
    if (before.bound == 0) {
        swofty::log("[ghost] new PB re-arm skipped: lap 3 is not bound to a car");
        std::lock_guard<std::mutex> g(pb_lock);
        pb_pending = false;
        return;
    }
    slot = 3;
    if (!read_trace_into(tl3, blob)) {
        swofty::log("[ghost] new PB re-arm failed: the reader refused the lap's own trace");
        std::lock_guard<std::mutex> g(pb_lock);
        pb_pending = false;
        return;
    }
    LapState read_back = state_of(tl3);
    uint32_t keep[5] = {};
    if (!mem::readable(tl1 + TL_MODE, sizeof keep) || !mem::readable(tl3 + TL_MODE, sizeof keep)) {
        swofty::log("[ghost] new PB re-arm: the play state fields are not readable");
        return;
    }
    std::memcpy(keep, reinterpret_cast<const void *>(tl1 + TL_MODE), sizeof keep);
    float elapsed = float(waited) / 1000.0f;
    std::memcpy(&keep[4], &elapsed, 4);
    keep[0] = 2;
    keep[1] = 2;
    swofty_core::hooks::guard::copy_memory(reinterpret_cast<void *>(tl3 + TL_MODE), keep, sizeof keep);
    LapState after = state_of(tl3);
    {
        std::lock_guard<std::mutex> g(pb_lock);
        pb_done = key;
        pb_pending = false;
    }
    pb_rearmed_at = now;
    pb_watch = 2;
    swofty::log("[ghost] new PB armed in lap 3 for the running session %.2f s after the lap: read back %u records (%.3f s), then play mode %u/%u at time %.3f (the lap's own "
                "elapsed time); before: count %u mode %u/%u; the ghost car stays bound to 0x%llx (a slot rewrite, no renderer reload)",
                elapsed, read_back.count, read_back.lap, after.mode, after.want, after.time, before.count, before.mode, before.want, (unsigned long long)after.bound);
}

uint64_t newlap_hook(void *comp, void *msg)
{
    uintptr_t c = uintptr_t(comp);
    PbKey before3 = pb_key_of(tlap_at(c, 3));
    uint64_t r = h_newlap.call<uint64_t>(comp, msg);
    if (!diag_on && !record_on)
        return r;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    if (ghost_on && pb_rearm_on) {
        PbKey a1 = pb_key_of(tlap_at(c, 1)), a3 = pb_key_of(tlap_at(c, 3));
        std::vector<uint8_t> blob;
        uintptr_t tl1 = tlap_at(c, 1);
        LapState l1 = tl1 ? state_of(tl1) : LapState{};
        float sec[3] = {};
        if (tl1)
            for (int i = 0; i < 3; i++)
                mem::read(tl1 + TL_LAP + 4 + 4 * i, sec[i]);
        bool clean = (l1.flags & 3) == 0 && sec[0] > 0 && sec[1] > 0 && sec[2] > 0 && std::fabs(sec[0] + sec[1] + sec[2] - l1.lap) < 0.05f;
        if (a3.count > 0 && a1 == a3 && !(a3 == before3) && !clean)
            swofty::log("[ghost] new PB not armed: the lap is not a valid one for the game (flags 0x%x, sectors %.3f %.3f %.3f, lap %.3f)", l1.flags, sec[0], sec[1], sec[2], l1.lap);
        if (clean && a3.count > 0 && a1 == a3 && !(a3 == before3) && trace_bytes_of(tl1, blob)) {
            std::lock_guard<std::mutex> g(pb_lock);
            pb_pending = true;
            pb_comp = c;
            pb_key = a3;
            pb_recorded_at = now_ms();
            pb_blob = std::move(blob);
            swofty::log("[ghost] new PB recorded: %.3f s, %u records; re-arm queued for the game thread", a3.lap, a3.count);
        }
    }
    uint8_t car = 0xff, valid = 0;
    mem::read(uintptr_t(msg) + 0x10, car);
    mem::read(uintptr_t(msg) + 0x18, valid);
    if (record_on && valid) {
        uintptr_t tl1 = tlap_at(c, 1), car = 0;
        int game_code = car_input(c, &car), best = -1, perf = -1;
        for (int i = 0; i < 4; i++)
            if (input_hist[i] > 0 && (best < 0 || input_hist[i] > input_hist[best]))
                best = i;
        uint32_t pv = 0;
        if (car && mem::read(car + runtime::lap_assists::CAR_PERFORMANCE, pv) && pv < 2)
            perf = int(pv);
        uint8_t summary[runtime::lap_assists::SUMMARY_SIZE];
        if (tl1 && mem::readable(tl1 + TL_SUMMARY, sizeof summary)) {
            std::memcpy(summary, reinterpret_cast<const void *>(tl1 + TL_SUMMARY), sizeof summary);
            int device = best >= 0 ? best : game_code;
            if (best >= 0 && game_code >= 0 && best != game_code)
                swofty::log("[laps] assists: input device by samples %d, by the car at lap end %d; using the samples", best, game_code);
            runtime::lap_rec_assists(summary, sizeof summary, device, perf);
        }
        for (int &h : input_hist)
            h = 0;
        std::vector<uint8_t> blob;
        if (uintptr_t tl = tlap_at(c, 1); tl && trace_bytes_of(tl, blob))
            runtime::lap_rec_native_trace(blob.data(), blob.size());
    }
    if (!diag_on)
        return r;
    swofty::log("[ghost] diag: lap recorded for car %u (message flag %u); laps %s", unsigned(car), unsigned(valid), describe(c).c_str());
    for (int i : {1, 3}) {
        uintptr_t tl = tlap_at(c, i);
        std::vector<uint8_t> blob;
        if (!tl || !trace_bytes_of(tl, blob)) {
            swofty::log("[ghost] diag: lap %d holds no readable trace", i);
            continue;
        }
        report_trace(i == 1 ? "diag: last completed lap (slot 1)" : "diag: stored personal best (slot 3)", blob);
        if (i == 1)
            dump(blob);
    }
    return r;
}

void setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    bool wanted = gate.is_object() && gate["tt_ghost"].as_bool(false);
    bool killed = env_is("SWOFTY_TT_GHOST", "0");
    diag_on = env_is("SWOFTY_TT_DIAG", "1") && !killed;
    ghost_on = wanted && !killed;
    record_on = gate.is_object() && gate["tt_boards"].as_bool(false) && !env_is("SWOFTY_LAP_GHOST", "0");
    if (!wanted && !diag_on && !record_on)
        return;
    if (wanted && !ghost_on)
        swofty::log("[ghost] off (SWOFTY_TT_GHOST=0)");
    if (!runtime::same_build()) {
        swofty::log("[ghost] off: the hook points are this build's byte signatures; this exe is a different build");
        return;
    }
    char sv[8] = {};
    if (GetEnvironmentVariableA("SWOFTY_TT_GHOST_SLOT", sv, sizeof sv) && sv[0] >= '0' && sv[0] <= '5')
        slot_env = sv[0] - '0';
    if (!ghost_on && !diag_on && !record_on)
        return;
    if (record_on || diag_on) {
        if (uintptr_t nl = runtime::find(S_NEWLAP, "ghost", "ghost")) {
            auto ih = safetyhook::InlineHook::create(reinterpret_cast<void *>(nl), reinterpret_cast<void *>(&newlap_hook));
            if (ih) {
                h_newlap = std::move(*ih);
                trace_armed = record_on;
                swofty::log("[ghost] lap trace hook armed at base+0x%llx%s%s", (unsigned long long)(nl - swofty::api->module_base()),
                            record_on ? ": the game's trace of every completed lap goes to the lap file" : "",
                            diag_on ? ", diag on" : "");
            } else
                swofty::log("[ghost] lap trace hook failed: %s", runtime::hook_error(ih.error()).c_str());
        } else
            swofty::log("[ghost] lap trace hook off: needs %s (byte signature matched nothing unique)", S_NEWLAP.what);
    }
    pb_rearm_on = !env_is("SWOFTY_TT_PB_REARM", "0");
    if (!ghost_on && !diag_on && !record_on)
        return;
    uintptr_t tick = runtime::find(S_TICK, "ghost", "ghost");
    if (!tick) {
        swofty::log("[ghost] off: needs %s (byte signature matched nothing unique)", S_TICK.what);
        return;
    }
    if (ghost_on) {
        uintptr_t rd = runtime::find(S_READ, "ghost", "ghost"), rs = runtime::find(S_RESET, "ghost", "ghost");
        if (!rd || !rs) {
            swofty::log("[ghost] playback off: needs %s and %s (byte signature matched nothing unique)", S_READ.what, S_RESET.what);
            ghost_on = false;
        } else {
            read_fn = reinterpret_cast<ReadFn>(rd);
            reset_fn = reinterpret_cast<ResetFn>(rs);
        }
    }
    auto tm = safetyhook::MidHook::create(reinterpret_cast<void *>(tick), tick_mid);
    if (!tm) {
        swofty::log("[ghost] off: the tick hook failed: %s", runtime::hook_error(tm.error()).c_str());
        return;
    }
    h_tick = std::move(*tm);
    if (ghost_on) {
        worker_run = true;
        std::thread(worker).detach();
    }
    swofty::log("[ghost] %s%s%s; tick hook at base+0x%llx, rival lap %d", ghost_on ? "playback armed (tt.json tt_ghost)" : "playback off",
                diag_on ? ", diag on (SWOFTY_TT_DIAG)" : "", "", (unsigned long long)(tick - swofty::api->module_base()), slot);
    if (!diag_on)
        return;
    if (uintptr_t rd = runtime::find(S_READ, "ghost", "ghost")) {
        auto ih = safetyhook::InlineHook::create(reinterpret_cast<void *>(rd), reinterpret_cast<void *>(&read_hook));
        if (ih)
            h_read = std::move(*ih);
        else
            swofty::log("[ghost] diag: the trace reader hook failed: %s", runtime::hook_error(ih.error()).c_str());
    }
}

}

namespace runtime {

bool ghost_play_trace_armed() { return trace_armed.load(); }

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("ghost playback", setup);
}
