#include "runtime.hpp"

#include <windows.h>

#include "apply/moments.hpp"
#include "direct_start_tt.hpp"
#include "hooks/core_events.hpp"
#include "hooks/hooks.hpp"
#include "lap_rec_logic.hpp"
#include "live_car.hpp"
#include "perf.hpp"

#include <swofty/mixin.hpp>
#include <swofty/systems/laps.hpp>

#include <atomic>
#include <cmath>
#include <mutex>

// The lap recorder: in a Time Trial, while swofty\tt.json carries "tt_boards": true, the flow tick samples the
// player's car and the line crossings of ers_newlap.cpp (session.lap) cut it into laps; each completed lap is written
// to swofty\laps\<UTC stamp>-<n>.swlap (lap_file_logic.hpp). The state machine is lap_rec_logic.hpp. Crossings are
// queued and handled on the next flow tick, so file writing and the game reads stay on the flow thread.
// lap_result() decides the lap time; it is the one place that changes when the game's own lap record is read.

namespace {

namespace lr = runtime::lap_rec;
namespace lf = runtime::lap_file;

#ifndef SWOFTY_CORE_VERSION
#define SWOFTY_CORE_VERSION ""
#endif

constexpr size_t MAX_SEALED_SECTIONS = 4, MAX_SEALED_BYTES = size_t(1) << 20;

struct Board {
    std::string content, track, weather;
};

struct Pending {
    lr::Outcome outcome;
};

struct LapResult {
    int64_t time_ms = 0;
    std::vector<int64_t> sectors_ms;
    std::optional<bool> valid;
    const char *source = "clock";
};

std::mutex lock;
std::atomic<bool> active{false};
std::atomic<int> player_car{-1};
lr::Recorder recorder;
std::vector<Pending> queued;
lr::Gate gate;
bool is_time_trial = false;
bool planned = false;
lr::Plan plan;
Board board;
std::string started_utc;
LARGE_INTEGER qpc_freq;
uint64_t discarded = 0, written = 0;

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

LapResult lap_result(const lr::Lap &lap)
{
    LapResult r;
    r.time_ms = int64_t(std::llround((lap.ended_s - lap.started_s) * 1000.0));
    return r;
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

Board snapshot(const live_car::LiveCar &c)
{
    Board b;
    auto facts = swofty_core::hooks::core().current();
    b.content = facts && !facts->content.empty() ? facts->content : "game/f1_2025";
    b.track = runtime::lower(runtime::direct_start::tt::current_track());
    b.weather = lr::weather_of(c.has_wetness, c.wetness);
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

void finish(lr::Lap &lap, const Board &b, const std::string &utc, const live_car::LiveCar &c)
{
    LARGE_INTEGER t0, t1, f;
    QueryPerformanceCounter(&t0);
    LapResult r = lap_result(lap);
    lf::Header h;
    h.core = SWOFTY_CORE_VERSION;
    h.game_build = runtime::build_exe_sha();
    if (h.game_build == "-")
        h.game_build.clear();
    h.nonce = gate.nonce;
    h.content = b.content;
    h.track = b.track;
    h.weather = b.weather;
    if (auto facts = swofty_core::hooks::core().current(); facts && facts->player) {
        h.team = facts->player->team;
        h.vehicle = facts->player->vehicle;
        h.driver = facts->player->driver;
    }
    h.time_ms = r.time_ms;
    h.sectors_ms = r.sectors_ms;
    h.valid = r.valid;
    h.source = r.source;
    h.lap_index = int64_t(lap.index);
    h.started_utc = utc;
    h.rate_hz = 60;
    (void)c;

    std::vector<uint8_t> body = lf::build(h, plan.layout, lap.samples.data(), lap.count, {}, {});
    body.resize(body.size() - 8);

    SYSTEMTIME st;
    utc_now(&st);
    std::wstring dir = runtime::swofty_root() + L"laps\\";
    CreateDirectoryW(dir.c_str(), nullptr);
    std::string name =
        lr::file_name(st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, lap.index);
    std::wstring path = dir + runtime::wide(name);
    std::string narrow_path = runtime::narrow(path);

    Seal seal;
    swofty::laps::events::Sealing ev{};
    ev.path_str = narrow_path.c_str();
    ev.time_ms = int32_t(r.time_ms);
    ev.body = body.data();
    ev.body_len = uint32_t(body.size());
    ev.add_section_fn = add_section;
    ev.user = &seal;
    swofty_core::hooks::events::laps_sealing(ev);

    for (auto &s : seal.sections)
        lf::append_section(body, s.tag, s.bytes.data(), s.bytes.size());
    lf::append_section(body, "END\0", nullptr, 0);

    if (!runtime::write_file(path, std::string(body.begin(), body.end()))) {
        swofty::log("[laps] writing %s failed", name.c_str());
        return;
    }
    QueryPerformanceCounter(&t1);
    QueryPerformanceFrequency(&f);
    written++;
    swofty::log("[laps] wrote %s lap %lld ms, %u samples, board %s|%s|%s", name.c_str(), (long long)r.time_ms, lap.count,
                b.content.c_str(), b.track.empty() ? "unknown" : b.track.c_str(),
                b.weather.empty() ? "unknown" : b.weather.c_str());
    swofty::log("[laps] lap %u: %zu bytes, built and written in %.1f ms", lap.index, body.size(),
                1000.0 * double(t1.QuadPart - t0.QuadPart) / double(f.QuadPart));
    swofty_core::hooks::events::laps_recorded(narrow_path.c_str(), int32_t(r.time_ms),
                                              r.valid ? (*r.valid ? 1 : 0) : -1);
}

void drain(const live_car::LiveCar &c)
{
    std::vector<Pending> work;
    {
        std::lock_guard<std::mutex> g(lock);
        work.swap(queued);
    }
    for (auto &p : work) {
        lr::Outcome &o = p.outcome;
        if (o.kind == lr::Outcome::completed) {
            finish(o.lap, board, started_utc, c);
            board = snapshot(c);
            started_utc = utc_now();
        } else if (o.kind == lr::Outcome::started) {
            board = snapshot(c);
            started_utc = utc_now();
            swofty::log("[laps] timing started, board %s|%s|%s", board.content.c_str(),
                        board.track.empty() ? "unknown" : board.track.c_str(),
                        board.weather.empty() ? "unknown" : board.weather.c_str());
        } else if (o.kind == lr::Outcome::discarded) {
            discarded++;
            swofty::log("[laps] lap discarded: %s", lr::discard_name(o.why));
        }
    }
}

void handle(lr::Outcome o)
{
    if (o.kind == lr::Outcome::none)
        return;
    std::lock_guard<std::mutex> g(lock);
    queued.push_back({std::move(o)});
}

}

namespace runtime {

void lap_rec_setup()
{
    swofty::log("[laps] recorder ready: it runs in a Time Trial when swofty\\tt.json has tt_boards on");
}

void lap_rec_session_enter()
{
    std::lock_guard<std::mutex> g(lock);
    active = false;
    queued.clear();
    planned = false;
    gate = lr::gate_of(read_file(swofty_root() + L"tt.json", size_t(1) << 20));
    is_time_trial = swofty_core::apply::moments::session_mode_track().first == "time_trial";
    if (!gate.on || !is_time_trial || !enabled("lap_rec", nullptr)) {
        recorder.end_session();
        return;
    }
    recorder.begin_session();
    discarded = written = 0;
    player_car = -1;
    active = true;
    swofty::log("[laps] recording this Time Trial session%s", gate.nonce.empty() ? "" : " (session nonce set)");
}

void lap_rec_session_end()
{
    if (!active.load())
        return;
    live_car::LiveCar c;
    live_car::read(c);
    drain(c);
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

void lap_rec_newlap(int car)
{
    if (!active.load(std::memory_order_relaxed))
        return;
    double now = now_seconds();
    std::lock_guard<std::mutex> g(lock);
    if (!active.load())
        return;
    int player = player_car.load();
    lr::Outcome o = recorder.newlap(now, player >= 0 && car == player);
    if (o.kind != lr::Outcome::none)
        queued.push_back({std::move(o)});
}

void lap_rec_tick()
{
    if (!active.load(std::memory_order_relaxed))
        return;
    static const uint32_t probe = perf::probe("core", "lap recorder tick");
    perf::Scope scope(probe);
    live_car::LiveCar c;
    live_car::LiveDynamics d;
    if (!live_car::read_dynamics(d) || !live_car::read(c))
        return;
    lr::Outcome out;
    {
        std::lock_guard<std::mutex> g(lock);
        player_car = d.ai ? -1 : d.player;
        if (!planned) {
            plan = lr::plan_of(avail_of(c, d));
            planned = true;
            swofty::log("[laps] fields resolved: %zu of %d; missing: %s", plan.layout.columns.size(), int(lr::Col::count),
                        plan.missing.empty() ? "none" : plan.missing.c_str());
        }
    }
    lr::Frame f;
    f.now_s = now_seconds();
    f.player = !d.ai && d.has_pos;
    if (d.has_pos)
        for (int i = 0; i < 3; i++)
            f.pos[i] = d.pos[i];
    f.speed_ms = d.has_vel ? std::sqrt(d.vel[0] * d.vel[0] + d.vel[1] * d.vel[1] + d.vel[2] * d.vel[2])
                           : c.speed_kmh / 3.6;
    static double last_pos[3];
    static bool have_last;
    std::vector<uint8_t> sample(plan.layout.stride);
    {
        std::lock_guard<std::mutex> g(lock);
        double dist = recorder.path_m();
        if (have_last && recorder.state() == lr::State::timing)
            dist += std::sqrt(std::pow(f.pos[0] - last_pos[0], 2) + std::pow(f.pos[1] - last_pos[1], 2) +
                              std::pow(f.pos[2] - last_pos[2], 2));
        fill(sample.data(), c, d, recorder.elapsed(f.now_s), dist);
        out = recorder.step(f, sample.data(), sample.size());
    }
    for (int i = 0; i < 3; i++)
        last_pos[i] = f.pos[i];
    have_last = true;
    handle(std::move(out));
    drain(c);
}

}
