#include "runtime.hpp"
#include "view_state.hpp"

#include <windows.h>

#include "apply/moments.hpp"
#include "direct_start_tt.hpp"
#include "era_logic.hpp"
#include "ghost_trace_logic.hpp"
#include "lap_assists_logic.hpp"
#include "lap_setup_logic.hpp"
#include "hooks/core_events.hpp"
#include "hooks/hooks.hpp"
#include "lap_game_logic.hpp"
#include "lap_rec_logic.hpp"
#include "live_car.hpp"
#include "perf.hpp"
#include "tt_bridge_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>
#include <swofty/systems/laps.hpp>

#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <map>
#include <mutex>
#include <thread>

// The lap recorder: in a Time Trial, while swofty\tt.json carries "tt_boards": true, the flow tick samples the
// player's car and the line crossings of ers_newlap.cpp (session.lap) cut it into laps; each completed lap is written
// to swofty\laps\<UTC stamp>-<n>.swlap (lap_file_logic.hpp). The state machine is lap_rec_logic.hpp. Crossings are
// queued and handled on the next flow tick, so the game reads stay on the flow thread. The flow tick runs once a
// frame; the car is read only when the recorder is due a sample (60 Hz), under a Trust so the heap checks are
// paid once per HEAP_TRUST_MS, and the inbox is drained only when a crossing, a game result or a blob is waiting.
// The board (content, track) and the player's team label are read once per session, during the out-lap, never at
// the line. The board's dry/wet is the car read's track wetness; where that read is off (the EA build), it is the D/W of
// the board key the game built for the session at the ghost storage's onSessionEnter (tt_bridge.cpp S_BOARD_KEY). At the line the flow thread builds the file header from state it already has and hands the samples to
// one writer thread, which builds the file, runs laps.sealing (under a Trust, its time kept off the game threads in
// perf) and writes it; the completions (the bridge's
// lap_finished, laps.recorded) come back through an Outbox and fire from the flow tick in lap order.
// lap_result() decides the lap time: the game's own lap record when the hook below delivered one, else the clock.
// With the game hooks armed a completed lap waits up to two seconds for the game's lap result and, on a valid lap, for
// the ghost blob the game commits on a personal best (lap_game_logic.hpp, Sealer); with them off the lap is written at
// once from the clock.

namespace runtime {
std::vector<era::Box> era_boxes();
tt_bridge::BoardTrack tt_bridge_board_track(const std::string &code, int stat_hint);
}

namespace {

namespace mem = swofty::mem;
namespace lr = runtime::lap_rec;
namespace lf = runtime::lap_file;

#ifndef SWOFTY_CORE_VERSION
#define SWOFTY_CORE_VERSION ""
#endif

constexpr size_t MAX_SEALED_SECTIONS = 4, MAX_SEALED_BYTES = size_t(1) << 20;

struct Board {
    std::string content, track, weather, track_addon, track_name, track_kind;
};

struct Pending {
    lr::Outcome outcome;
};

struct LapContext {
    Board board;
    std::string started_utc;
    std::optional<double> session_time_s;
    swofty::json::Value setup;
};

struct Blob {
    std::vector<uint8_t> bytes;
    double at_s;
};

constexpr const char *GAME_MIXIN = "swofty_core.dll (lap recorder game lap result)";
constexpr const char *LAP_TARGET = "Sess_LapTimes_ReportNewLapTime";
const runtime::Sig SIG_TLAP_KEY{"game.telemetry_lap_registry_key_1461026a8", "48 8D 05 ?? ?? ?? ?? 48 89 84 24 98 00 00 00 48 8D 94 24 98 00 00 00 48 89 D9",
                       0x61026a8, runtime::Sig::riprel, 0, 3, 7};
std::atomic<uintptr_t> tlap_key{0};
constexpr uintptr_t HANDLER_BLOB = 0x278, HANDLER_BLOB_SIZE = 0x280, HANDLER_REGISTRY = 0x20, REGISTRY_MAP = 0x30,
                    HANDLER_TIMING = 0x30, TIMING_SESSION = 0x88;

using ReportFn = uint64_t(void *, void *);
ReportFn *orig_report = nullptr;
bool game_offered = false, game_armed = false, ghost_armed = false, hooks_settled = false;
std::atomic<uintptr_t> registry_map{0}, session_base{0};

struct LapResult {
    int64_t time_ms = 0;
    std::vector<int64_t> sectors_ms;
    std::optional<bool> valid;
    const char *source = "clock";
};

std::mutex lock;
std::atomic<bool> active{false};
std::atomic<int> player_car{-1};
bool sampler_warned = false;
std::atomic<bool> inbox{false};
std::atomic<bool> finished_waiting{false};
lr::Recorder recorder;
std::vector<Pending> queued;
lr::Gate gate;
bool is_time_trial = false;
bool planned = false;
lr::Plan plan;
std::vector<uint8_t> sample_buf;
Board board;
Board session_board;
bool have_session_board = false;
std::string session_team;
bool have_session_team = false;
double next_board_try_s = 0;
int board_tries = 0;
bool last_has_wetness = false;
float last_wetness = 0;
bool weather_logged = false;
lr::WeatherFrom weather_from = lr::WeatherFrom::none;
std::string started_utc;
lr::Sealer sealer;
lr::Outbox outbox;
std::map<uint32_t, LapContext> contexts;

struct Job {
    lf::Header header;
    lf::Layout layout;
    std::vector<uint8_t> samples;
    uint32_t count = 0;
    std::vector<uint8_t> ghost;
    std::wstring path;
    std::string name, narrow_path;
    Board board;
    uint32_t index = 0;
    int64_t time_ms = 0;
    int valid = -1;
    const char *source = "clock";
};

std::mutex work_lock;
std::condition_variable work_cv;
std::deque<Job> jobs;
size_t in_flight = 0;
bool worker_up = false;
struct AssistReading {
    runtime::lap_assists::Raw raw;
    int device = -1, performance = -1;
    double at_s = -1e9;
    bool used = true;
};
AssistReading assist_reading;
runtime::lap_setup::Setup setup_start;
bool have_setup_start = false;
std::vector<lr::GameLap> game_in;
std::vector<Blob> ghost_in;
LARGE_INTEGER qpc_freq;
uint64_t discarded = 0, written = 0;

std::atomic<double> paused_total{0};

double now_seconds()
{
    LARGE_INTEGER n;
    QueryPerformanceCounter(&n);
    if (!qpc_freq.QuadPart)
        QueryPerformanceFrequency(&qpc_freq);
    return double(n.QuadPart) / double(qpc_freq.QuadPart);
}

std::string utc_now(SYSTEMTIME *out = nullptr)
{
    SYSTEMTIME t;
    GetSystemTime(&t);
    if (out)
        *out = t;
    char b[32];
    snprintf(b, sizeof b, "%04d-%02d-%02dT%02d:%02d:%02dZ", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    return b;
}

LapResult lap_result(const lr::Lap &lap, const std::optional<lr::GameLap> &game)
{
    LapResult r;
    if (game && game->usable) {
        r.time_ms = game->time_ms;
        r.sectors_ms = game->sectors_ms;
        r.valid = game->lap_valid;
        r.source = "game";
        return r;
    }
    r.time_ms = int64_t(std::llround((lap.ended_s - lap.started_s) * 1000.0));
    return r;
}

bool read_memory(uint64_t at, void *dst, size_t n)
{
    if (!mem::readable(uintptr_t(at), n))
        return false;
    std::memcpy(dst, reinterpret_cast<const void *>(uintptr_t(at)), n);
    return true;
}

std::optional<double> session_clock()
{
    const SwoftyTelemetry *t = runtime::telemetry_table();
    SwoftySessionState st{};
    if (!t || !t->session || t->session(&st) != SWOFTY_ASSET_OK || !std::isfinite(st.session_time) || st.session_time <= 0)
        return std::nullopt;
    return double(st.session_time);
}

std::string content_title(const std::string &content)
{
    for (const auto &box : runtime::era_boxes())
        if (!box.title.empty() && (box.owner + "/" + box.entrant_list == content || box.time_trial == content))
            return box.title;
    return {};
}

lr::Avail avail_of(const live_car::LiveCar &c, const live_car::LiveDynamics &d)
{
    lr::Avail a;
    a.pos = d.has_pos;
    a.vel = d.has_vel;
    a.pedals = c.has_pedals;
    a.steer = c.has_steer;
    a.gear = c.has_gear;
    a.rpm = c.has_rpm;
    a.speed = c.has_speed || d.has_vel;
    a.orient = d.has_orient;
    a.spin = d.has_spin;
    a.g = c.has_g;
    a.drs = d.has_drs;
    a.surface = c.has_surface;
    a.slip = d.has_slip;
    return a;
}

void fill(uint8_t *sample, const live_car::LiveCar &c, const live_car::LiveDynamics &d, double t, double dist)
{
    using lr::Col;
    float speed = d.has_vel ? std::sqrt(d.vel[0] * d.vel[0] + d.vel[1] * d.vel[1] + d.vel[2] * d.vel[2]) * 3.6f
                            : c.speed_kmh;
    const float nan = std::nanf("");
    for (size_t i = 0; i < plan.ids.size(); i++) {
        double v = nan;
        switch (plan.ids[i]) {
        case Col::t: v = t; break;
        case Col::dist: v = dist; break;
        case Col::px: v = d.has_pos ? d.pos[0] : nan; break;
        case Col::py: v = d.has_pos ? d.pos[1] : nan; break;
        case Col::pz: v = d.has_pos ? d.pos[2] : nan; break;
        case Col::vx: v = d.has_vel ? d.vel[0] : nan; break;
        case Col::vy: v = d.has_vel ? d.vel[1] : nan; break;
        case Col::vz: v = d.has_vel ? d.vel[2] : nan; break;
        case Col::throttle: v = c.has_pedals ? c.throttle : nan; break;
        case Col::brake: v = c.has_pedals ? c.brake : nan; break;
        case Col::steer: v = c.has_steer ? c.steer : nan; break;
        case Col::gear: v = c.has_gear ? c.gear : -128; break;
        case Col::rpm: v = c.has_rpm ? c.rpm : nan; break;
        case Col::speed: v = speed; break;
        case Col::qx: v = d.has_orient ? d.quat[0] : nan; break;
        case Col::qy: v = d.has_orient ? d.quat[1] : nan; break;
        case Col::qz: v = d.has_orient ? d.quat[2] : nan; break;
        case Col::qw: v = d.has_orient ? d.quat[3] : nan; break;
        case Col::ws_rl: v = d.has_spin ? d.spin[0] : nan; break;
        case Col::ws_rr: v = d.has_spin ? d.spin[1] : nan; break;
        case Col::ws_fl: v = d.has_spin ? d.spin[2] : nan; break;
        case Col::ws_fr: v = d.has_spin ? d.spin[3] : nan; break;
        case Col::g_lat: v = c.has_g ? c.g_lateral : nan; break;
        case Col::g_long: v = c.has_g ? c.g_long : nan; break;
        case Col::drs: v = d.has_drs ? d.drs : nan; break;
        case Col::surf_fl: v = c.has_surface ? c.surface[0] : 255; break;
        case Col::surf_fr: v = c.has_surface ? c.surface[1] : 255; break;
        case Col::surf_rl: v = c.has_surface ? c.surface[2] : 255; break;
        case Col::surf_rr: v = c.has_surface ? c.surface[3] : 255; break;
        case Col::slip_ang_rl: v = d.has_slip ? d.slip_angle[0] : nan; break;
        case Col::slip_ang_rr: v = d.has_slip ? d.slip_angle[1] : nan; break;
        case Col::slip_ang_fl: v = d.has_slip ? d.slip_angle[2] : nan; break;
        case Col::slip_ang_fr: v = d.has_slip ? d.slip_angle[3] : nan; break;
        case Col::slip_ratio_rl: v = d.has_slip ? d.slip_ratio[0] : nan; break;
        case Col::slip_ratio_rr: v = d.has_slip ? d.slip_ratio[1] : nan; break;
        case Col::slip_ratio_fl: v = d.has_slip ? d.slip_ratio[2] : nan; break;
        case Col::slip_ratio_fr: v = d.has_slip ? d.slip_ratio[3] : nan; break;
        case Col::count: break;
        }
        plan.layout.set(sample, i, v);
    }
}

// The session's content and track: a walk of the game's mode objects, read once per session.
Board session_snapshot()
{
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    Board b;
    auto facts = swofty_core::hooks::core().current();
    b.content = facts && !facts->content.empty() ? facts->content : "game/f1_2025";
    std::string code;
    if (!runtime::tt_bridge_session_track(code))
        code = runtime::direct_start::tt::session_track();
    runtime::tt_bridge::BoardTrack t = runtime::tt_bridge_board_track(runtime::lower(code),
                                                                      runtime::tt_bridge::parse_board(runtime::tt_bridge_last_board()).track);
    b.track = t.key;
    b.track_name = t.name;
    b.track_addon = t.addon;
    b.track_kind = t.kind;
    return b;
}

// Reads the board and the team label once; a session whose track is not known yet is asked again later.
void resolve_session(double now, bool forced = false)
{
    if (have_session_board && have_session_team)
        return;
    if (now < next_board_try_s)
        return;
    next_board_try_s = now + 1.0;
    if (!have_session_board && lr::board_read_due(forced, board_tries)) {
        ++board_tries;
        Board b = session_snapshot();
        std::lock_guard<std::mutex> g(lock);
        session_board = std::move(b);
        have_session_board = !session_board.track.empty();
    }
    if (!have_session_team) {
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        std::string team = runtime::direct_start::tt::player_team();
        std::lock_guard<std::mutex> g(lock);
        session_team = std::move(team);
        have_session_team = !session_team.empty();
    }
}

Board board_now(bool force)
{
    if (force) {
        next_board_try_s = 0;
        resolve_session(now_seconds(), true);
    }
    Board b = session_board;
    std::string key = last_has_wetness ? std::string() : runtime::tt_bridge_session_board();
    lr::Weather w = lr::weather_pick(last_has_wetness, last_wetness, runtime::tt_bridge::parse_board(key).weather);
    b.weather = w.word;
    if (!weather_logged || w.from != weather_from) {
        weather_logged = true;
        weather_from = w.from;
        if (w.from == lr::WeatherFrom::wetness)
            swofty::log("[laps] weather %s from %s (%.2f)", w.word.c_str(), lr::weather_from_name(w.from), last_wetness);
        else if (w.from == lr::WeatherFrom::board)
            swofty::log("[laps] weather %s from %s %s (the track wetness is not read on this build)", w.word.c_str(), lr::weather_from_name(w.from), key.c_str());
        else
            swofty::log("[laps] weather unknown: no track wetness on this build and no session board key from the game; the laps carry no dry or wet");
    }
    return b;
}

struct Seal {
    std::vector<lf::Section> sections;
    size_t bytes = 0;
};

int32_t add_section(void *user, const char *tag, const void *bytes, uint32_t len)
{
    auto *seal = static_cast<Seal *>(user);
    if (!tag || (len && !bytes) || lf::reserved(tag) || seal->sections.size() >= MAX_SEALED_SECTIONS ||
        seal->bytes + len > MAX_SEALED_BYTES)
        return SWOFTY_ASSET_INVALID;
    auto *p = static_cast<const uint8_t *>(bytes);
    seal->sections.push_back({{tag[0], tag[1], tag[2], tag[3]}, std::vector<uint8_t>(p, p + len)});
    seal->bytes += len;
    return SWOFTY_ASSET_OK;
}

lr::ExtrasFrom read_extras(lr::GameLap &g);
void log_extras(unsigned lap, const char *when, const lr::GameLap &g, const lr::ExtrasFrom &from);

void write_job(Job &j)
{
    LARGE_INTEGER t0, t1, f;
    QueryPerformanceCounter(&t0);
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    std::vector<uint8_t> body = lf::build(j.header, j.layout, j.samples.data(), j.count, j.ghost, {});
    body.resize(body.size() - 8);

    Seal seal;
    swofty::laps::events::Sealing ev{};
    ev.path_str = j.narrow_path.c_str();
    ev.time_ms = int32_t(j.time_ms);
    ev.body = body.data();
    ev.body_len = uint32_t(body.size());
    ev.add_section_fn = add_section;
    ev.user = &seal;
    swofty_core::hooks::events::laps_sealing(ev);

    for (auto &s : seal.sections)
        lf::append_section(body, s.tag, s.bytes.data(), s.bytes.size());
    lf::append_section(body, "END\0", nullptr, 0);

    lr::Finished done;
    done.index = j.index;
    done.path = j.narrow_path;
    done.time_ms = j.time_ms;
    done.valid = j.valid;
    done.ok = runtime::write_file(j.path, std::string(body.begin(), body.end()));
    if (!done.ok)
        swofty::log("[laps] writing %s failed", j.name.c_str());
    else {
        QueryPerformanceCounter(&t1);
        QueryPerformanceFrequency(&f);
        const Board &b = j.board;
        swofty::log("[laps] wrote %s lap %lld ms (%s), %u samples, ghost %zu bytes, board %s|%s|%s%s", j.name.c_str(),
                    (long long)j.time_ms, j.source, j.count, j.ghost.size(), b.content.c_str(),
                    b.track.empty() ? "unknown" : b.track.c_str(), b.weather.empty() ? "unknown" : b.weather.c_str(),
                    b.track_kind.empty() ? "" : (", custom track (" + b.track_kind + " from " + b.track_addon + ")").c_str());
        swofty::log("[laps] lap %u: %zu bytes, built and written in %.1f ms on the writer thread", j.index, body.size(),
                    1000.0 * double(t1.QuadPart - t0.QuadPart) / double(f.QuadPart));
    }
    std::lock_guard<std::mutex> g(lock);
    outbox.complete(std::move(done));
    finished_waiting = true;
}

void worker_main()
{
    perf::background();
    for (;;) {
        Job j;
        {
            std::unique_lock<std::mutex> g(work_lock);
            work_cv.wait(g, [] { return !jobs.empty(); });
            j = std::move(jobs.front());
            jobs.pop_front();
        }
        write_job(j);
        {
            std::lock_guard<std::mutex> g(work_lock);
            in_flight--;
        }
        work_cv.notify_all();
    }
}

void post(Job j)
{
    std::lock_guard<std::mutex> g(work_lock);
    jobs.push_back(std::move(j));
    in_flight++;
    if (!worker_up) {
        worker_up = true;
        std::thread(worker_main).detach();
    }
    work_cv.notify_all();
}

bool wait_idle(uint32_t ms)
{
    std::unique_lock<std::mutex> g(work_lock);
    return work_cv.wait_for(g, std::chrono::milliseconds(ms), [] { return in_flight == 0; });
}

// The completions the writer thread posted, in lap order, fired on the flow thread.
void deliver()
{
    if (!finished_waiting.load(std::memory_order_relaxed))
        return;
    std::vector<lr::Finished> ready;
    {
        std::lock_guard<std::mutex> g(lock);
        ready = outbox.ready();
        finished_waiting = outbox.completed() > 0;
    }
    for (auto &d : ready) {
        if (!d.ok)
            continue;
        written++;
        runtime::tt_bridge_lap_finished(d.time_ms, d.valid);
        swofty_core::hooks::events::laps_recorded(d.path.c_str(), int32_t(d.time_ms), d.valid);
    }
}

// On the flow thread: the header from state already in hand and the one late game read, then the samples go to
// the writer thread.
void finish(lr::Sealed &sealed, const LapContext &ctx)
{
    LARGE_INTEGER t0, t1, f;
    QueryPerformanceCounter(&t0);
    lr::Lap &lap = sealed.lap;
    const Board &b = ctx.board;
    if (sealed.game && sealed.game->usable && (!sealed.game->tlap.ok || !sealed.game->stats.ok)) {
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        lr::ExtrasFrom late = read_extras(*sealed.game);
        if (sealed.game->tlap.ok || sealed.game->stats.ok || late.tlap != lr::From::none)
            log_extras(lap.index, " (read when the lap was written)", *sealed.game, late);
    }
    LapResult r = lap_result(lap, sealed.game);
    if (sealer.game_armed() && !sealed.game)
        swofty::log("[laps] lap %u: the game reported no lap result within the wait; using the clock", lap.index);
    lf::Header h;
    h.core = SWOFTY_CORE_VERSION;
    h.game_build = runtime::build_exe_sha();
    if (h.game_build == "-")
        h.game_build.clear();
    if (gate.nonce.empty()) {
        std::string late = lr::gate_of(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20)).nonce;
        if (!late.empty()) {
            gate.nonce = late;
            swofty::log("[laps] the session nonce arrived after the session started; lap %u and the ones after carry it", lap.index);
        }
    }
    h.nonce = gate.nonce;
    h.content = b.content;
    h.track = b.track;
    h.track_name = b.track_name;
    h.track_addon = b.track_addon;
    h.track_kind = b.track_kind;
    h.weather = b.weather;
    h.content_name = content_title(b.content);
    std::string game_board = runtime::tt_bridge_last_board();
    runtime::tt_bridge::BoardName gb = runtime::tt_bridge::parse_board(game_board);
    if (gb.ok && (b.weather.empty() || b.weather == runtime::tt_bridge::weather_word(gb.weather)))
        h.game_board = game_board;
    uint8_t id_block[runtime::tt_bridge::ID_BLOCK];
    if (!sealed.ghost.empty() && runtime::tt_bridge_ghost_id(id_block))
        h.ghost_id = runtime::tt_bridge::base64(id_block, sizeof id_block);
    {
        runtime::presence_id::Id pid;
        if (runtime::tt_bridge_presence_id(pid)) {
            h.presence = pid;
            char dv[8] = {};
            if (GetEnvironmentVariableA("SWOFTY_TT_DIAG", dv, sizeof dv) && !strcmp(dv, "1")) {
                auto stream = runtime::presence_id::serialise(pid);
                swofty::log("[laps] lap %u: presence in the header: type %u, platform %d, value0 %llu, flags %u %u, stream hex %s base64 %s", lap.index,
                            unsigned(pid.type), runtime::presence_id::platform_of(pid.type), (unsigned long long)pid.value[0], unsigned(pid.flag39),
                            unsigned(pid.flag38), runtime::presence_id::hex(stream).c_str(), runtime::presence_id::base64(stream).c_str());
            }
        } else
            swofty::log("[laps] lap %u: the game's presence id was never read (no board request passed the gate yet); the header has no presence", lap.index);
    }
    if (auto facts = swofty_core::hooks::core().current(); facts && facts->player) {
        h.team = facts->player->team;
        h.vehicle = facts->player->vehicle;
        h.driver = facts->player->driver;
    }
    if (h.team.empty()) {
        h.team = session_team;
        if (!h.team.empty())
            swofty::log("[laps] lap %u: car team label %s from the player's entrant selection", lap.index, h.team.c_str());
    }
    h.time_ms = r.time_ms;
    h.sectors_ms = r.sectors_ms;
    h.valid = r.valid;
    h.source = r.source;
    h.lap_index = int64_t(lap.index);
    h.started_utc = ctx.started_utc;
    h.session_time_s = ctx.session_time_s;
    h.rate_hz = 60;
    h.setup = ctx.setup;
    if (!assist_reading.used && std::fabs(now_seconds() - assist_reading.at_s) < 20.0) {
        const AssistReading &a = assist_reading;
        h.assists = runtime::lap_assists::to_json(a.raw, a.device);
        h.driver_assists = int64_t(runtime::lap_assists::packed(a.raw));
        if (a.performance == 0 || a.performance == 1)
            h.performance = lr::performance_name(a.performance);
        swofty::log("[laps] assists: lap %u %s", lap.index, runtime::lap_assists::diag_text(a.raw, a.device, a.performance).c_str());
        assist_reading.used = true;
    } else
        swofty::log("[laps] assists: lap %u none read from the game's telemetry lap", lap.index);
    if (sealed.game && sealed.game->usable) {
        const lr::TLapExtras &t = sealed.game->tlap;
        if (t.ok && !h.driver_assists) {
            h.assists = swofty::json::Value::object(
                {{"brk", int64_t(t.brk)}, {"abs", int64_t(t.abs)}, {"trc", int64_t(t.trc)}, {"drl", int64_t(t.drl)},
                 {"gbx", int64_t(t.gbx)}, {"ers", int64_t(t.ers)}, {"input", int64_t(t.input)}});
            h.performance = lr::performance_name(t.performance);
        }
        if (sealed.game->stats.ok) {
            h.team_stat = lr::packed_stat(sealed.game->stats.team);
            h.vehicle_stat = lr::packed_stat(sealed.game->stats.vehicle);
            h.livery_stat = lr::packed_stat(sealed.game->stats.livery);
        }
    }

    SYSTEMTIME st;
    utc_now(&st);
    std::wstring dir = runtime::swofty_root() + L"laps\\";
    CreateDirectoryW(dir.c_str(), nullptr);
    Job j;
    j.name = lr::file_name(st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, lap.index);
    j.path = dir + runtime::wide(j.name);
    j.narrow_path = runtime::narrow(j.path);
    j.header = std::move(h);
    j.layout = plan.layout;
    j.samples = std::move(lap.samples);
    j.count = lap.count;
    j.ghost = std::move(sealed.ghost);
    j.board = b;
    j.index = lap.index;
    j.time_ms = r.time_ms;
    j.valid = r.valid ? (*r.valid ? 1 : 0) : -1;
    j.source = r.source;
    {
        std::lock_guard<std::mutex> g(lock);
        outbox.expect(lap.index);
    }
    post(std::move(j));
    QueryPerformanceCounter(&t1);
    QueryPerformanceFrequency(&f);
    swofty::log("[laps] lap %u: header built and handed to the writer thread in %.1f ms", lap.index,
                1000.0 * double(t1.QuadPart - t0.QuadPart) / double(f.QuadPart));
}

bool read_setup_now(runtime::lap_setup::Setup &out)
{
    if (!live_car::kb_fields())
        return false;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    uintptr_t car = live_car::player_car();
    return car && runtime::lap_setup::read(read_memory, car, out);
}

swofty::json::Value lap_setup_of(uint32_t index)
{
    runtime::lap_setup::Setup end;
    bool have_end = read_setup_now(end);
    runtime::lap_setup::Setup use = have_setup_start ? setup_start : end;
    bool have = have_setup_start || have_end;
    if (have_setup_start && have_end)
        use.changed = runtime::lap_setup::differs(setup_start, end);
    swofty::log("[laps] setup: lap %u %s%s %s", index, have ? "" : "unreadable", use.changed ? " (changed during the lap)" : "",
                have ? runtime::lap_setup::diag_text(use).c_str() : "");
    setup_start = end;
    have_setup_start = have_end;
    return have ? runtime::lap_setup::to_json(use) : swofty::json::Value();
}

void release(std::vector<lr::Sealed> ready)
{
    for (auto &s : ready) {
        LapContext ctx;
        auto it = contexts.find(s.lap.index);
        if (it != contexts.end()) {
            ctx = std::move(it->second);
            contexts.erase(it);
        }
        finish(s, ctx);
    }
}

void drain()
{
    std::vector<Pending> work;
    std::vector<lr::GameLap> games;
    std::vector<Blob> ghosts;
    {
        std::lock_guard<std::mutex> g(lock);
        work.swap(queued);
        games.swap(game_in);
        ghosts.swap(ghost_in);
        inbox = false;
    }
    for (auto &g : games) {
        double at = g.at_s;
        sealer.game(std::move(g), at);
    }
    for (auto &b : ghosts)
        sealer.ghost(std::move(b.bytes), b.at_s);
    for (auto &p : work) {
        lr::Outcome &o = p.outcome;
        if (o.kind == lr::Outcome::completed) {
            contexts[o.lap.index] = {board, started_utc, session_clock(), lap_setup_of(o.lap.index)};
            double ended = o.lap.ended_s + o.lap.shift_s + paused_total.load();
            sealer.lap(std::move(o.lap), ended);
            board = board_now(false);
            started_utc = utc_now();
        } else if (o.kind == lr::Outcome::started) {
            have_setup_start = read_setup_now(setup_start);
            board = board_now(!have_session_board);
            started_utc = utc_now();
            swofty::log("[laps] timing started, board %s|%s|%s%s", board.content.c_str(),
                        board.track.empty() ? "unknown" : board.track.c_str(),
                        board.weather.empty() ? "unknown" : board.weather.c_str(),
                        board.track_kind.empty() ? "" : (", custom track (" + board.track_kind + " from " + board.track_addon + ")").c_str());
        } else if (o.kind == lr::Outcome::discarded) {
            discarded++;
            swofty::log("[laps] lap discarded: %s", lr::discard_name(o.why));
        }
    }
    release(sealer.poll(now_seconds()));
}

lr::ExtrasFrom read_extras(lr::GameLap &g)
{
    lr::ExtrasRoots roots;
    roots.manager = uintptr_t(runtime::direct_start_context().game_manager);
    roots.handler_map = registry_map.load();
    roots.handler_base = session_base.load();
    roots.key = tlap_key.load();
    auto rdr = read_memory;
    return lr::read_extras(rdr, g, roots);
}

void log_extras(unsigned lap, const char *when, const lr::GameLap &g, const lr::ExtrasFrom &from)
{
    swofty::log("[laps] lap %u extras%s: assists/input/performance %s%s, team/vehicle/livery stat ids %s%s", lap, when,
                g.tlap.ok ? "from the telemetry lap via " : "unavailable",
                g.tlap.ok ? lr::from_name(from.tlap) : "",
                g.stats.ok ? "from the participant list via " : "unavailable",
                g.stats.ok ? lr::from_name(from.stats) : "");
}

uint64_t report_lap(void *lap_times, void *msg)
{
    uint64_t r = orig_report(lap_times, msg);
    if (!active.load(std::memory_order_relaxed))
        return r;
    uint8_t car = 0;
    int player = player_car.load();
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    if (player < 0 || !mem::read(uintptr_t(msg) + lr::MSG_CAR, car) || car != player)
        return r;
    auto rdr = read_memory;
    lr::GameLap g = lr::decode_lap(rdr, uint64_t(lap_times), uint64_t(msg));
    lr::sanity(g);
    g.at_s = now_seconds();
    if (g.usable) {
        lr::ExtrasFrom from = read_extras(g);
        log_extras(g.msg_lap, "", g, from);
        swofty::log("[laps] lap result from the game: car %u record %d (message lap %u) %lld ms, sectors %lld %lld %lld ms, "
                    "lap %s, sectors %d%d%d valid",
                    unsigned(g.car), g.record, g.msg_lap, (long long)g.time_ms, (long long)g.sectors_ms[0],
                    (long long)g.sectors_ms[1], (long long)g.sectors_ms[2], g.lap_valid ? "valid" : "invalid",
                    g.sector_valid[0], g.sector_valid[1], g.sector_valid[2]);
        if (g.tlap.ok)
            swofty::log("[laps] game lap extras: assists brk %u abs %u trc %u drl %u gbx %u ers %u, input %s, performance %s",
                        g.tlap.brk, g.tlap.abs, g.tlap.trc, g.tlap.drl, g.tlap.gbx, g.tlap.ers,
                        lr::input_name(g.tlap.input), lr::performance_name(g.tlap.performance));
        else
            swofty::log("[laps] game lap extras: the telemetry lap is not readable yet (%s)",
                        runtime::direct_start_context().game_manager ? "no matching lap in the systems map" : "no game manager");
        if (g.stats.ok)
            swofty::log("[laps] game lap stats: team %u vehicle %u livery %u", g.stats.team, g.stats.vehicle, g.stats.livery);
    } else {
        swofty::log("[laps] game lap result refused (%s): lap %.3f s, sectors %.3f %.3f %.3f s; the clock decides this lap",
                    g.why.c_str(), g.lap_s, g.sector_s[0], g.sector_s[1], g.sector_s[2]);
    }
    std::lock_guard<std::mutex> lk(lock);
    if (active.load()) {
        game_in.push_back(std::move(g));
        inbox = true;
    }
    return r;
}

bool env_off(const char *name, const char *label)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    if (!n || n >= sizeof v || (strcmp(v, "0") && strcmp(v, "off")))
        return false;
    swofty::log("[laps] %s off (%s=%s)", label, name, v);
    return true;
}

}

namespace runtime {

void lap_rec_ghost(uintptr_t handler)
{
    uintptr_t blob = 0, timing = 0, registry = 0, map = 0, base = 0;
    uint64_t size = 0;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    if (mem::read(handler + HANDLER_REGISTRY, registry) && registry && mem::read(registry + REGISTRY_MAP, map))
        registry_map = map;
    if (mem::read(handler + HANDLER_TIMING, timing) && timing && mem::read(timing + TIMING_SESSION, base))
        session_base = base;
    if (!active.load(std::memory_order_relaxed))
        return;
    if (!mem::read(handler + HANDLER_BLOB, blob) || !mem::read(handler + HANDLER_BLOB_SIZE, size) ||
        !lr::ghost_size_ok(size) || !mem::readable(blob, size_t(size))) {
        swofty::log("[laps] ghost blob refused: handler 0x%llx, pointer 0x%llx, size %llu", (unsigned long long)handler,
                    (unsigned long long)blob, (unsigned long long)size);
        return;
    }
    Blob b{std::vector<uint8_t>(size), now_seconds()};
    std::memcpy(b.bytes.data(), reinterpret_cast<const void *>(blob), size_t(size));
    swofty::log("[laps] ghost blob from the game: %llu bytes", (unsigned long long)size);
    std::lock_guard<std::mutex> lk(lock);
    if (active.load()) {
        ghost_in.push_back(std::move(b));
        inbox = true;
    }
}

void lap_rec_assists(const uint8_t *summary, size_t size, int device, int performance)
{
    runtime::lap_assists::Raw raw;
    if (size < runtime::lap_assists::SUMMARY_SIZE || !runtime::lap_assists::from_summary(summary, raw)) {
        swofty::log("[laps] assists: the game's summary bytes are not assist levels (unwritten)");
        return;
    }
    assist_reading = {raw, device, performance, now_seconds(), false};
}

bool lap_rec_current_board(std::string &content, std::string &track, std::string &weather)
{
    if (swofty_core::apply::moments::session_mode_track().first != "time_trial")
        return false;
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    live_car::LiveCar c;
    live_car::read(c);
    Board b;
    bool cached = false;
    {
        std::lock_guard<std::mutex> g(lock);
        if (active.load() && have_session_board) {
            b = session_board;
            cached = true;
        }
    }
    if (!cached)
        b = session_snapshot();
    content = b.content;
    track = b.track;
    std::string key = c.has_wetness ? std::string() : runtime::tt_bridge_session_board();
    weather = lr::weather_pick(c.has_wetness, c.wetness, runtime::tt_bridge::parse_board(key).weather).word;
    return true;
}

void lap_rec_native_trace(const uint8_t *data, size_t size)
{
    if (!active.load(std::memory_order_relaxed))
        return;
    runtime::ghost_trace::Trace t;
    if (runtime::ghost_trace::decode(data, size, t) != runtime::ghost_trace::Status::ok || !lr::ghost_size_ok(size)) {
        swofty::log("[laps] native trace refused: %zu bytes do not decode", size);
        return;
    }
    swofty::log("[laps] native trace from the game: %zu bytes, %zu records", size, t.records.size());
    Blob b{std::vector<uint8_t>(data, data + size), now_seconds()};
    std::lock_guard<std::mutex> lk(lock);
    if (active.load()) {
        ghost_in.push_back(std::move(b));
        inbox = true;
    }
}

std::vector<CoreMixins> lap_rec_mixins()
{
    std::vector<CoreMixins> out;
    if (game_offered || !code_ready())
        return out;
    game_offered = true;
    if (!lr::gate_of(read_file(swofty_root() + L"tt.json", size_t(1) << 20)).on)
        return out;
    if (!verified_build()) {
        swofty::log("[laps] game lap result off: this is not a build the lap offsets are checked on; laps use "
                    "the clock");
        return out;
    }
    tlap_key = runtime::find(SIG_TLAP_KEY, "telemetry lap key", "laps");
    if (!tlap_key.load()) {
        swofty::log("[laps] game lap result off: the telemetry lap key did not resolve on this build; laps use the clock");
        return out;
    }
    if (!env_off("SWOFTY_LAP_GAME_RESULT", "game lap result")) {
        if (swofty::api->resolve(LAP_TARGET))
            out.push_back({GAME_MIXIN, {{LAP_TARGET, SWOFTY_MIXIN_WRAP, 100, (void *)&report_lap, (void **)&orig_report}}});
        else
            swofty::log("[laps] game lap result off: needs %s, which this build does not resolve; laps use the clock",
                        LAP_TARGET);
    }
    return out;
}

void lap_rec_commit()
{
    if (!game_offered || hooks_settled)
        return;
    hooks_settled = true;
    game_armed = core_mixins_active(GAME_MIXIN);
    ghost_armed = tt_bridge_pb_armed() || ghost_play_trace_armed();
    swofty::log("[laps] game lap result %s; ghost blob %s", game_armed ? "armed" : "not armed (laps use the clock)",
                ghost_armed ? "armed" : "not armed (no GHST section)");
}

void lap_rec_setup()
{
    swofty::log("[laps] recorder ready: it runs in a Time Trial when swofty\\tt.json has tt_boards on");
}

void lap_rec_session_enter()
{
    std::lock_guard<std::mutex> g(lock);
    active = false;
    queued.clear();
    game_in.clear();
    ghost_in.clear();
    sealer = lr::Sealer(lr::Sealer::Config{5.0, 5.0});
    contexts.clear();
    planned = false;
    inbox = false;
    have_session_board = have_session_team = false;
    session_board = Board{};
    session_team.clear();
    next_board_try_s = 0;
    board_tries = 0;
    last_has_wetness = false;
    weather_logged = false;
    gate = lr::gate_of(read_file(swofty_root() + L"tt.json", size_t(1) << 20));
    is_time_trial = swofty_core::apply::moments::session_mode_track().first == "time_trial";
    if (!gate.on || !is_time_trial || !enabled("lap_rec", nullptr)) {
        recorder.end_session();
        return;
    }
    recorder.begin_session();
    ghost_armed = tt_bridge_pb_armed() || ghost_play_trace_armed();
    sealer.expect(game_armed, ghost_armed);
    tt_bridge_session_enter();
    discarded = written = 0;
    player_car = -1;
    sampler_warned = false;
    active = true;
    swofty::log("[laps] recording this Time Trial session%s; lap source %s, ghost blob %s",
                gate.nonce.empty() ? "" : " (session nonce set)", game_armed ? "the game's lap record" : "the clock",
                ghost_armed ? "from the game" : "off");
}

void lap_rec_session_end()
{
    if (!active.load())
        return;
    {
        mem::Trust trust(runtime::HEAP_TRUST_MS);
        drain();
        release(sealer.flush());
    }
    if (!wait_idle(3000))
        swofty::log("[laps] the writer thread is still writing a lap; its completion fires on a later tick");
    deliver();
    active = false;
    lr::Outcome o;
    {
        std::lock_guard<std::mutex> g(lock);
        o = recorder.end_session();
        queued.clear();
    }
    swofty::log("[laps] session ended: %llu lap(s) written, %llu discarded%s", (unsigned long long)written,
                (unsigned long long)discarded, o.kind == lr::Outcome::discarded ? ", the lap in progress dropped" : "");
}

bool lap_rec_active()
{
    return active.load(std::memory_order_relaxed);
}

void lap_rec_newlap(int car)
{
    if (!active.load(std::memory_order_relaxed))
        return;
    double now = now_seconds();
    std::lock_guard<std::mutex> g(lock);
    if (!active.load())
        return;
    int player = player_car.load();
    lr::Outcome o = recorder.newlap(now - paused_total.load(), player >= 0 && car == player);
    // Timing starts after an out-lap, or the lap that just ended was dropped: the replay recording starts again at this line.
    if (o.kind == lr::Outcome::started || o.kind == lr::Outcome::discarded)
        runtime::tt_bridge_lap_started();
    if (o.kind != lr::Outcome::none) {
        queued.push_back({std::move(o)});
        inbox = true;
    }
}

void lap_rec_tick()
{
    toast::tick();
    deliver();
    if (!active.load(std::memory_order_relaxed))
        return;
    static const uint32_t probe = perf::probe("core", "lap recorder tick");
    perf::Scope scope(probe);
    static double paused_since = -1;
    static uint32_t paused_mask = 0, stalls_logged = 0;
    static double next_read_s = 0;
    static double last_pos[3];
    static bool have_last;
    double now = now_seconds();
    uint32_t view_flags = view::current().flags;
    uint32_t reasons = view::pause_reasons();
    if (reasons || (view_flags & view::PHOTO)) {
        if (paused_since < 0) {
            paused_since = now;
            paused_mask = reasons;
            swofty::log("[laps] paused (pause mask %08x%s): sampling and the lap clock stop, the lap in progress continues "
                        "on resume",
                        reasons, view_flags & view::PHOTO ? ", photo mode" : "");
        }
        return;
    }
    if (paused_since >= 0) {
        swofty::log("[laps] resumed after %.1f s (pause mask %08x); the lap in progress continues", now - paused_since,
                    paused_mask);
        paused_total.store(paused_total.load() + now - paused_since);
        paused_since = -1;
    }
    double now_lap = now - paused_total.load();
    bool due, work;
    {
        std::lock_guard<std::mutex> g(lock);
        due = recorder.state() == lr::State::timing ? recorder.sample_due(now_lap) : now >= next_read_s;
        work = inbox.load(std::memory_order_relaxed) || sealer.waiting() > 0;
    }
    mem::Trust trust(runtime::HEAP_TRUST_MS);
    if (!due) {
        if (work)
            drain();
        return;
    }
    next_read_s = now + recorder.config().sample_period_s;
    if (!have_session_board || !have_session_team)
        resolve_session(now);
    live_car::LiveCar c;
    live_car::LiveDynamics d;
    if (!live_car::read_dynamics(d) || !live_car::read(c)) {
        if (!live_car::available() && !sampler_warned) {
            sampler_warned = true;
            swofty::log("[laps] NO LAP IS RECORDED IN THIS SESSION: the live car sampler is off on this build (see the "
                        "[shaders] live car line above), so the recorder never learns which car is the player's and "
                        "drops every lap");
        }
        if (work)
            drain();
        return;
    }
    last_has_wetness = c.has_wetness;
    last_wetness = c.wetness;
    lr::Frame f;
    f.now_s = now_lap;
    f.player = !d.ai && d.has_pos;
    f.interrupted = (view_flags & (view::REPLAY | view::CUTSCENE | view::SPECTATING)) != 0;
    if (d.has_pos)
        for (int i = 0; i < 3; i++)
            f.pos[i] = d.pos[i];
    f.speed_ms = d.has_vel ? std::sqrt(d.vel[0] * d.vel[0] + d.vel[1] * d.vel[1] + d.vel[2] * d.vel[2])
                           : c.speed_kmh / 3.6;
    lr::Outcome out;
    {
        std::lock_guard<std::mutex> g(lock);
        player_car = d.ai ? -1 : d.player;
        if (!planned) {
            plan = lr::plan_of(avail_of(c, d));
            planned = true;
            sample_buf.assign(plan.layout.stride, 0);
            swofty::log("[laps] fields resolved: %zu of %d; missing: %s", plan.layout.columns.size(), int(lr::Col::count),
                        plan.missing.empty() ? "none" : plan.missing.c_str());
        }
        double dist = recorder.path_m();
        if (have_last && recorder.state() == lr::State::timing)
            dist += std::sqrt(std::pow(f.pos[0] - last_pos[0], 2) + std::pow(f.pos[1] - last_pos[1], 2) +
                              std::pow(f.pos[2] - last_pos[2], 2));
        fill(sample_buf.data(), c, d, recorder.elapsed(f.now_s), dist);
        if (recorder.state() == lr::State::timing)
            runtime::tt_bridge_lap_progress(dist, recorder.elapsed(f.now_s));
        out = recorder.step(f, sample_buf.data(), sample_buf.size());
        if (recorder.stalls() < stalls_logged)
            stalls_logged = 0;
        if (recorder.stalls() > stalls_logged) {
            swofty::log("[laps] the game stalled without a pause (the car did not move while the clock ran): %.2f s taken "
                        "out of the lap clock in total, %u stall(s)",
                        recorder.shift_s(), recorder.stalls());
            stalls_logged = recorder.stalls();
        }
        if (out.kind != lr::Outcome::none) {
            queued.push_back({std::move(out)});
            inbox = true;
            work = true;
        }
    }
    for (int i = 0; i < 3; i++)
        last_pos[i] = f.pos[i];
    have_last = true;
    if (work)
        drain();
}

}
