#include "ghost_swlap_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
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
// behaviour. SWOFTY_TT_GHOST_SLOT picks another telemetry lap than 4.

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
constexpr int SLOTS = 6, MAX_ATTEMPTS = 3, MAX_DUMPS = 8;
constexpr uintptr_t PLAY_POLL_MS = 500, DIAG_POLL_MS = 1000;

using ReadFn = int (*)(void *tlap, void *stream);
using ResetFn = void (*)(void *tlap);
using NewLapFn = uint64_t (*)(void *comp, void *msg);

struct Stream {
    const void *vtable;
    const void *data;
    uint64_t size;
    uint64_t pos;
};

std::mutex lock;
safetyhook::MidHook h_tick;
safetyhook::InlineHook h_read, h_newlap;
ResetFn reset_fn = nullptr;
ReadFn read_fn = nullptr;
std::atomic<uintptr_t> component{0};
std::atomic<int> diag_reads{0};
bool ghost_on = false, diag_on = false, record_on = false;
std::atomic<bool> trace_armed{false};
int slot_env = -1, slot = 4;
std::vector<uint8_t> saved_blob;
bool saved_has = false;
uintptr_t saved_for = 0;

struct Request {
    std::string path, label;
    std::vector<uint8_t> blob;
    std::string kind;
    uint64_t stamp = 0;
    size_t records = 0;
    float lap_s = 0;
};

Request request;
bool have_request = false;
uintptr_t armed_for = 0;
int attempts = 0;
uint64_t last_stamp = 0;
int64_t last_poll = 0, last_diag = 0;
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
    auto json = swofty::json::parse(text);
    if (!json.is_object()) {
        why = "play.json is not a JSON object";
        return false;
    }
    out.path = std::string(json["lap"].as_string());
    out.label = std::string(json["label"].as_string());
    if (out.path.empty()) {
        why = "play.json has no lap";
        return false;
    }
    std::string bytes = runtime::read_file(runtime::wide(out.path), size_t(64) << 20);
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

void restore(uintptr_t comp);

void poll_request()
{
    int64_t now = now_ms();
    if (now - last_poll < int64_t(PLAY_POLL_MS))
        return;
    last_poll = now;
    std::wstring play = ghost_dir() + L"play.json";
    WIN32_FILE_ATTRIBUTE_DATA d{};
    if (!GetFileAttributesExW(play.c_str(), GetFileExInfoStandard, &d)) {
        if (have_request) {
            have_request = false;
            last_stamp = 0;
            restore(component.load());
            armed_for = 0;
            swofty::log("[ghost] play.json is gone; the ghost is disarmed");
        }
        return;
    }
    uint64_t stamp = (uint64_t(d.ftLastWriteTime.dwHighDateTime) << 32) | d.ftLastWriteTime.dwLowDateTime;
    stamp ^= (uint64_t(d.nFileSizeLow) << 1);
    if (stamp == last_stamp)
        return;
    last_stamp = stamp;
    Request r;
    std::string why;
    if (!load_request(play, r, why)) {
        have_request = false;
        swofty::log("[ghost] request refused: %s", why.c_str());
        return;
    }
    r.stamp = stamp;
    {
        std::lock_guard<std::mutex> g(lock);
        request = std::move(r);
        have_request = true;
        armed_for = 0;
        attempts = 0;
    }
    swofty::log("[ghost] request: %s lap '%s' (%s), driver '%s': %zu records, %zu bytes, lap %.3f s", request.kind.c_str(), request.path.c_str(),
                request.kind.c_str(), request.label.c_str(), request.records, request.blob.size(), request.lap_s);
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

void restore(uintptr_t comp)
{
    if (slot != 3 || saved_for != comp || armed_for != comp)
        return;
    uintptr_t tl = tlap_at(comp, 3);
    if (!tl)
        return;
    LapState s = state_of(tl);
    if (std::fabs(s.lap - request.lap_s) > 0.002f) {
        swofty::log("[ghost] restore: lap 3 no longer holds the supplied trace (time %.3f); left as is", s.lap);
    } else if (saved_has) {
        read_trace_into(tl, saved_blob);
        swofty::log("[ghost] restore: the player's own personal best trace is back in lap 3");
    } else {
        reset_fn(reinterpret_cast<void *>(tl));
        swofty::log("[ghost] restore: lap 3 reset, the player had no personal best trace");
    }
    saved_for = 0;
    armed_for = 0;
}

void arm(uintptr_t comp)
{
    if (!ghost_on || !have_request || armed_for == comp || attempts >= MAX_ATTEMPTS)
        return;
    int pick = slot_env;
    if (pick < 0) {
        uintptr_t rival = tlap_at(comp, 4);
        pick = rival && state_of(rival).bound ? 4 : 3;
    }
    uintptr_t tl = tlap_at(comp, pick);
    if (!tl) {
        return;
    }
    slot = pick;
    if (slot == 3 && saved_for != comp) {
        saved_has = trace_bytes_of(tl, saved_blob);
        saved_for = comp;
        swofty::log("[ghost] arm: lap 3 is the player's personal best slot; its trace is %s", saved_has ? "kept for the restore" : "empty");
    }
    attempts++;
    LapState before = state_of(tl);
    swofty::log("[ghost] arm: component 0x%llx, lap %d at 0x%llx, bound %s (+0x130 = 0x%llx), mode %u/%u, count %u before; %s",
                (unsigned long long)comp, slot, (unsigned long long)tl, before.bound ? "to a car" : "to no car",
                (unsigned long long)before.bound, before.mode, before.want, before.count, describe(comp).c_str());
    if (!before.bound)
        swofty::log("[ghost] arm: lap %d is not bound to a ghost car; the trace is stored but the game plays it only when a car is bound. "
                    "Set SWOFTY_TT_GHOST_SLOT to a bound lap to test",
                    slot);
    if (!read_trace_into(tl, request.blob)) {
        swofty::log("[ghost] arm: failed (attempt %d of %d)", attempts, MAX_ATTEMPTS);
        return;
    }
    armed_for = comp;
    swofty::log("[ghost] armed: '%s' (%.3f s) is lap %d's trace; it plays from the next lap start while the lap is bound to a car",
                request.label.c_str(), request.lap_s, slot);
}

void tick_mid(safetyhook::Context &c)
{
    uintptr_t comp = c.rcx;
    if (!comp)
        return;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    std::lock_guard<std::mutex> g(lock);
    if (component.load() != comp) {
        component = comp;
        swofty::log("[ghost] telemetry component 0x%llx", (unsigned long long)comp);
        last_slots.clear();
    }
    if (ghost_on)
        poll_request();
    if (ghost_on && have_request)
        arm(comp);
    if (diag_on) {
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

uint64_t newlap_hook(void *comp, void *msg)
{
    uintptr_t c = uintptr_t(comp);
    uint64_t r = h_newlap.call<uint64_t>(comp, msg);
    if (!diag_on && !record_on)
        return r;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    uint8_t car = 0xff, valid = 0;
    mem::read(uintptr_t(msg) + 0x10, car);
    mem::read(uintptr_t(msg) + 0x18, valid);
    if (record_on && valid) {
        std::vector<uint8_t> blob;
        if (uintptr_t tl = tlap_at(c, 1); tl && trace_bytes_of(tl, blob))
            runtime::lap_rec_native_trace(blob.data(), blob.size());
    }
    if (!diag_on)
        return r;
    std::lock_guard<std::mutex> g(lock);
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
    diag_on = env_is("SWOFTY_TT_DIAG", "1");
    ghost_on = wanted && !env_is("SWOFTY_TT_GHOST", "0");
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
    if (!ghost_on && !diag_on)
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
