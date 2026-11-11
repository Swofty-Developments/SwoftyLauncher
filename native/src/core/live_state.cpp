#include "live/live_rows.hpp"
#include "live/live_snapshot.hpp"
#include "live/live_state.hpp"
#include "live/live_telemetry_fill.hpp"
#include "live/live_turns.hpp"
#include "runtime.hpp"
#include "telemetry_feed.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <mutex>

namespace {

namespace lv = swofty_core::live;

struct Named {
    const char *name;
    uintptr_t lv::Sites::*slot;
};

const Named SITE_NAMES[] = {
    {"g_Sess_VehicleTable", &lv::Sites::vehicle_table},
    {"g_Sess_VehicleCount", &lv::Sites::vehicle_count},
    {"g_Game_Manager", &lv::Sites::game_manager},
    {"g_Game_TrackLength", &lv::Sites::track_length},
    {"g_Sess_TyreRulesSessionType", &lv::Sites::session_type},
    {"vt_Car_SystemsContainer", &lv::Sites::vt_car_systems},
    {"k_Sess_ResultsSystemKey", &lv::Sites::key_results},
    {"k_Sess_SpectatorDirectorKey", &lv::Sites::key_spectator},
    {"k_Sess_TimingSystemKey", &lv::Sites::key_timing},
    {"k_Sess_PitStateKey", &lv::Sites::key_pit},
    {"k_Sess_SafetyCarLogicKey", &lv::Sites::key_safety_car},
    {"k_Sess_PenaltyLogKey", &lv::Sites::key_penalty_log},
    {"k_Sess_SessionComponentKey", &lv::Sites::key_session_component},
    {"k_Sess_TyreSystemKey", &lv::Sites::key_tyres},
    {"k_Sess_MarshalZonesKey", &lv::Sites::key_marshal},
    {"k_Sess_RedFlagKey", &lv::Sites::key_red_flag},
    {"k_Game_ModeContextKey", &lv::Sites::key_game_mode},
    {"k_Game_TeamTablesKey", &lv::Sites::key_team_tables},
    {"g_Sess_ControllerSystem", &lv::Sites::controllers},
    {"vt_Sess_ControllerSystem", &lv::Sites::vt_controllers},
    {"vt_Sess_HumanController", &lv::Sites::vt_human},
    {"g_Game_SectorTable", &lv::Sites::sector_block},
};

const char *TRACK_WORLD_PATTERN = "48 8B 05 ?? ?? ?? ?? 48 8B 80 D0 00 00 00 48 8B 80 E8 00 00 00";

bool kernel_copy(void *, uintptr_t addr, void *dst, size_t n)
{
    SIZE_T got = 0;
    return ReadProcessMemory(GetCurrentProcess(), reinterpret_cast<LPCVOID>(addr), dst, n, &got) && got == n;
}

bool env_on(const char *name)
{
    char v[8] = {};
    return GetEnvironmentVariableA(name, v, sizeof v) && v[0] && v[0] != '0';
}

struct Needs {
    lv::Group group;
    uintptr_t lv::Sites::*sites[6];
};

const Needs NEEDS[] = {
    {lv::G_CAR_SYSTEMS, {&lv::Sites::vehicle_table, &lv::Sites::vehicle_count}},
    {lv::G_ERS, {&lv::Sites::vehicle_table, &lv::Sites::vehicle_count}},
    {lv::G_WORLD, {&lv::Sites::vehicle_table, &lv::Sites::vehicle_count}},
    {lv::G_AERO, {&lv::Sites::vehicle_table, &lv::Sites::vehicle_count}},
    {lv::G_PROGRESS, {&lv::Sites::game_manager, &lv::Sites::track_length}},
    {lv::G_CLASS, {&lv::Sites::game_manager, &lv::Sites::key_results}},
    {lv::G_TIMING, {&lv::Sites::game_manager, &lv::Sites::key_timing, &lv::Sites::key_safety_car}},
    {lv::G_PIT, {&lv::Sites::game_manager, &lv::Sites::key_pit}},
    {lv::G_PENALTIES, {&lv::Sites::game_manager, &lv::Sites::key_penalty_log, &lv::Sites::key_session_component}},
    {lv::G_TYRES, {&lv::Sites::game_manager, &lv::Sites::key_tyres}},
    {lv::G_FLAGS, {&lv::Sites::game_manager, &lv::Sites::key_marshal, &lv::Sites::key_red_flag, &lv::Sites::sector_block}},
    {lv::G_CONTROL, {&lv::Sites::controllers, &lv::Sites::vt_controllers, &lv::Sites::vt_human}},
    {lv::G_SPECTATE, {&lv::Sites::game_manager, &lv::Sites::key_spectator}},
    {lv::G_MODE, {&lv::Sites::game_manager, &lv::Sites::key_game_mode}},
    {lv::G_BADGES, {&lv::Sites::game_manager, &lv::Sites::key_game_mode, &lv::Sites::key_team_tables}},
    {lv::G_TRACK, {&lv::Sites::track_length, &lv::Sites::session_type}},
};

constexpr lv::Group TELEMETRY_GROUPS[] = {lv::G_CAR_SYSTEMS, lv::G_ERS, lv::G_PROGRESS, lv::G_CLASS, lv::G_TIMING,
                                          lv::G_PIT,        lv::G_PENALTIES, lv::G_TYRES, lv::G_FLAGS, lv::G_CONTROL,
                                          lv::G_TRACK};

constexpr bool COLLISION_NEEDS_UDP = true;

bool g_on;
bool g_forced;
bool g_verify;
lv::Sites g_sites;
lv::Gates g_gates;
std::unique_ptr<lv::Live> g_live;
lv::Snapshot g_snap;
std::mutex g_lock;
std::atomic<bool> g_valid;
uint64_t g_frame;
std::vector<lv::Event> g_events;
SwoftyTelemetryFeed g_mem_view;
float g_prev_track_len = -1;
char g_prev_track_name[48];
int g_official = -1;
lv::turns::Table g_turns;

void resolve_sites()
{
    for (const Named &n : SITE_NAMES)
        g_sites.*n.slot = swofty::api && swofty::api->resolve ? swofty::api->resolve(n.name) : 0;
    if (auto *base = reinterpret_cast<const uint8_t *>(GetModuleHandleW(nullptr))) {
        auto *dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
        auto *nt = reinterpret_cast<const IMAGE_NT_HEADERS64 *>(base + dos->e_lfanew);
        g_sites.image_lo = uintptr_t(base);
        g_sites.image_hi = uintptr_t(base) + nt->OptionalHeader.SizeOfImage;
    }
    uintptr_t at = 0;
    int32_t disp = 0;
    if (swofty::api && swofty::api->scan &&
        swofty::api->scan(".xtls", TRACK_WORLD_PATTERN, &at) == 1 && at && swofty::mem::read(at + 3, disp))
        g_sites.track_world = at + 7 + intptr_t(disp);
}

void decide_gates()
{
    g_forced = env_on("SWOFTY_LIVE_FORCE");
    g_verify = env_on("SWOFTY_TELEMETRY_VERIFY");
    for (const Needs &n : NEEDS) {
        const lv::Row &row = lv::ROWS[n.group];
        bool sites = true;
        const char *missing = nullptr;
        for (auto s : n.sites)
            if (s && !(g_sites.*s)) {
                sites = false;
                missing = "a site that does not resolve";
            }
        bool ready = row.ready || g_forced;
        g_gates.on[n.group] = sites && ready;
        if (!g_gates.on[n.group])
            swofty::log("[live] %s off: %s", lv::group_name(n.group),
                        !sites ? missing : "needs a verified KB row (SWOFTY_LIVE_FORCE=1 reads it unverified)");
    }
}

void publish()
{
    lv::snap_begin(&g_snap);
    g_snap.frame = ++g_frame;
    g_snap.valid = g_live->active();
    g_snap.ses = g_live->ses;
    std::memcpy(g_snap.cars, g_live->cars, sizeof g_snap.cars);
    std::memcpy(g_snap.present, g_live->present, sizeof g_snap.present);
    g_snap.official = g_official;
    g_snap.turn_length = g_turns.length;
    lv::snap_end(&g_snap);
    g_valid.store(g_snap.valid, std::memory_order_release);
}

void refresh_turns()
{
    const SwoftyLiveSession &s = g_live->ses;
    float len = (s.have & SWOFTY_LIVE_SHAVE_TRACK_LENGTH) ? s.track_length_m : 0;
    if (len == g_prev_track_len && !std::strcmp(g_prev_track_name, s.track_name))
        return;
    g_prev_track_len = len;
    std::memcpy(g_prev_track_name, s.track_name, sizeof g_prev_track_name);
    if (len > 0)
        g_turns.reset(s.track_name, len);
    else
        g_turns = lv::turns::Table{};
    g_official = g_turns.official;
}

int live_available()
{
    return g_on && g_valid.load(std::memory_order_acquire) ? 1 : 0;
}

bool read_snapshot(lv::Snapshot &out)
{
    return g_on && lv::snap_read(&g_snap, &out) && out.valid;
}

int live_session(SwoftyLiveSession *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out)
        return SWOFTY_ASSET_INVALID;
    static thread_local lv::Snapshot s;
    if (!read_snapshot(s))
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = s.ses;
    return SWOFTY_ASSET_OK;
}

int car_from(const lv::Snapshot &s, uint32_t vehicle, SwoftyLiveCar *out)
{
    if (vehicle >= SWOFTY_LIVE_MAX_CARS || !s.present[vehicle])
        return SWOFTY_ASSET_UNAVAILABLE;
    *out = s.cars[vehicle];
    return SWOFTY_ASSET_OK;
}

int live_car(uint32_t vehicle, SwoftyLiveCar *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out || vehicle >= SWOFTY_LIVE_MAX_CARS)
        return SWOFTY_ASSET_INVALID;
    static thread_local lv::Snapshot s;
    if (!read_snapshot(s))
        return SWOFTY_ASSET_UNAVAILABLE;
    return car_from(s, vehicle, out);
}

int live_subject(SwoftyLiveCar *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out)
        return SWOFTY_ASSET_INVALID;
    static thread_local lv::Snapshot s;
    if (!read_snapshot(s) || s.ses.subject < 0)
        return SWOFTY_ASSET_UNAVAILABLE;
    return car_from(s, uint32_t(s.ses.subject), out);
}

int live_turn(uint32_t vehicle, SwoftyLiveTurn *out)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!out || vehicle >= SWOFTY_LIVE_MAX_CARS)
        return SWOFTY_ASSET_INVALID;
    static thread_local lv::Snapshot s;
    if (!read_snapshot(s) || !s.present[vehicle] || !(s.cars[vehicle].have & SWOFTY_LIVE_HAVE_PROGRESS) || s.official < 0)
        return SWOFTY_ASSET_UNAVAILABLE;
    lv::turns::Table t;
    t.official = s.official;
    t.length = s.turn_length;
    return t.at(s.cars[vehicle].lap_m, *out) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_UNAVAILABLE;
}

int live_track_turns(uint32_t *count, SwoftyLiveTurn *out, uint32_t cap)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!count)
        return SWOFTY_ASSET_INVALID;
    static thread_local lv::Snapshot s;
    if (!read_snapshot(s) || s.official < 0)
        return SWOFTY_ASSET_UNAVAILABLE;
    lv::turns::Table t;
    t.official = s.official;
    t.length = s.turn_length;
    *count = uint32_t(t.count());
    for (uint32_t i = 0; out && i < cap && i < *count; i++)
        t.fill(int(i), out[i], 0.f);
    return SWOFTY_ASSET_OK;
}

int live_team_badge(uint32_t vehicle, char *key, uint32_t cap)
{
    if (!g_on || !g_live)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!key || cap < 2 || vehicle >= SWOFTY_LIVE_MAX_CARS)
        return SWOFTY_ASSET_INVALID;
    std::lock_guard<std::mutex> g(g_lock);
    return g_live->read_team_badge(vehicle, key, cap) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_UNAVAILABLE;
}

int live_seq(uint64_t *frame)
{
    if (!g_on)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!frame)
        return SWOFTY_ASSET_INVALID;
    std::atomic_ref<uint64_t> f(g_snap.frame);
    *frame = f.load(std::memory_order_acquire);
    return SWOFTY_ASSET_OK;
}

const SwoftyLive table = {SWOFTY_LIVE_VERSION, live_available, live_session,      live_car,        live_subject,
                          live_turn,           live_track_turns, live_team_badge, live_seq};

}

extern "C" __declspec(dllexport) const SwoftyLive *SwoftyCoreLive(void)
{
    return &table;
}

namespace runtime {

void live_setup()
{
    if (!enabled("telemetry", nullptr))
        return;
    if (!same_build()) {
        swofty::log("[live] off: this is not the build the layouts are from");
        return;
    }
    resolve_sites();
    decide_gates();
    bool any = false;
    for (bool b : g_gates.on)
        any = any || b;
    if (!any) {
        swofty::log("[live] off: no group is verified yet");
        return;
    }
    lv::Mem mem;
    mem.fn = kernel_copy;
    g_live = std::make_unique<lv::Live>(mem, g_sites, g_gates);
    g_on = true;
    swofty::log("[live] on%s%s", g_forced ? ", reading unverified groups (SWOFTY_LIVE_FORCE)" : "",
                g_verify ? ", cross-checking against UDP (SWOFTY_TELEMETRY_VERIFY)" : "");
}

bool live_on()
{
    return g_on;
}

void live_tick(double now)
{
    if (!g_on)
        return;
    {
        std::lock_guard<std::mutex> g(g_lock);
        g_live->update(now);
        refresh_turns();
        for (const lv::Event &e : g_live->take_events())
            g_events.push_back(e);
        if (g_events.size() > 256)
            g_events.erase(g_events.begin(), g_events.end() - 256);
        publish();
    }
}

bool live_memory_verified()
{
    if (!g_on)
        return false;
    for (lv::Group g : TELEMETRY_GROUPS)
        if (!g_gates[g])
            return false;
    return true;
}

bool live_collision_needs_udp()
{
    return COLLISION_NEEDS_UDP;
}

bool live_events_from_memory()
{
    return g_on && g_gates[lv::G_CLASS];
}

uint32_t live_fill_feed(SwoftyTelemetryFeed &f)
{
    if (!g_on)
        return 0;
    lv::Snapshot s;
    if (!lv::snap_read(&g_snap, &s) || !s.valid)
        return 0;
    uint32_t groups = lv::fill_feed(f, s.ses, s.cars, s.present);
    if (g_verify) {
        swofty::tel::feed_init(&g_mem_view);
        lv::fill_feed(g_mem_view, s.ses, s.cars, s.present);
    }
    return groups;
}

bool live_verify()
{
    return g_on && g_verify;
}

const SwoftyTelemetryFeed &live_memory_view()
{
    return g_mem_view;
}

std::vector<LiveEvent> live_take_events()
{
    std::vector<LiveEvent> out;
    for (const lv::Event &e : g_events)
        out.push_back({e.kind == lv::Event::retirement ? SWOFTY_EVENT_TELEMETRY_RETIREMENT : SWOFTY_EVENT_TELEMETRY_OVERTAKE,
                       e.a, e.b});
    g_events.clear();
    return out;
}

}
