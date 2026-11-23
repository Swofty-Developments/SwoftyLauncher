// SPDX-License-Identifier: MIT
#pragma once
#include <swofty/live_images.h>
#include <array>
#include <atomic>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>
#include <swofty_api.h>
#include <swofty/db.hpp>
#include <swofty/lang.hpp>
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

// Live game state read from the game's memory: every car by vehicle id (the watched car included), the session
// header, the classification and road order, gaps, pit, tyre and penalty state, and the official turn at a car's
// lap distance. The core keeps one snapshot per frame and every call copies from it, so the calls are cheap and
// consistent from any thread. A value the game does not give for a car has its bit in Car::have clear and is zero;
// check has() before drawing it. A vehicle id is the slot of the game's vehicle table, the same index as
// cars::live::Car::index() and not the UDP participant index. Every call answers empty on a core without the
// live state, while no session is live, or while the group is not yet verified for the running build.

namespace swofty::live {

using Car = SwoftyLiveCar;
using Session = SwoftyLiveSession;
using Turn = SwoftyLiveTurn;
using CarDetail = SwoftyLiveCarDetail;

constexpr uint32_t version = SWOFTY_LIVE_VERSION;
constexpr uint32_t max_cars = SWOFTY_LIVE_MAX_CARS;
constexpr int32_t no_gap = SWOFTY_LIVE_NO_GAP;

enum class GameMode : uint8_t { unknown = 0, grand_prix = 1, time_trial = 2 };
enum class SafetyCar : uint8_t { none = 0, safety_car = 1, virtual_safety_car = 2, formation = 3, unknown = 0xff };

namespace detail {

inline std::atomic<const SwoftyLive *> &override_table()
{
    static std::atomic<const SwoftyLive *> t{nullptr};
    return t;
}

inline void set_table(const SwoftyLive *t) { override_table() = t; }

inline const SwoftyLive *table()
{
    if (auto t = override_table().load())
        return t->version >= 1 ? t : nullptr;
#if defined(_WIN32)
    static std::atomic<const SwoftyLive *> found{nullptr};
    if (auto t = found.load())
        return t;
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    auto get = core ? reinterpret_cast<SwoftyCoreLiveFn>(GetProcAddress(core, "SwoftyCoreLive")) : nullptr;
    const SwoftyLive *t = get ? get() : nullptr;
    if (t && t->version < 1)
        t = nullptr;
    found = t;
    return t;
#else
    return nullptr;
#endif
}

}

inline bool available()
{
    auto t = detail::table();
    return t && t->available && t->available();
}

inline std::optional<Session> session()
{
    auto t = detail::table();
    Session s{};
    if (!t || !t->session || t->session(&s) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return s;
}

inline std::optional<Car> car(uint32_t vehicle)
{
    auto t = detail::table();
    Car c{};
    if (!t || !t->car || t->car(vehicle, &c) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return c;
}

inline std::optional<Car> subject()
{
    auto t = detail::table();
    Car c{};
    if (!t || !t->subject || t->subject(&c) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return c;
}

inline std::optional<Car> player()
{
    auto s = session();
    return s && s->player >= 0 ? car(static_cast<uint32_t>(s->player)) : std::nullopt;
}

inline std::optional<Car> watched()
{
    auto s = session();
    return s && s->watched >= 0 ? car(static_cast<uint32_t>(s->watched)) : std::nullopt;
}

inline std::optional<Turn> turn(const Car &c)
{
    auto t = detail::table();
    Turn out{};
    if (!t || !t->turn || t->turn(c.vehicle, &out) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return out;
}

inline std::vector<Turn> track_turns()
{
    auto t = detail::table();
    uint32_t count = 0;
    if (!t || !t->track_turns || t->track_turns(&count, nullptr, 0) != SWOFTY_ASSET_OK || !count)
        return {};
    std::vector<Turn> out(count);
    if (t->track_turns(&count, out.data(), static_cast<uint32_t>(out.size())) != SWOFTY_ASSET_OK)
        return {};
    out.resize(count < out.size() ? count : out.size());
    return out;
}

inline std::optional<std::string> team_badge(uint32_t vehicle)
{
    auto t = detail::table();
    char key[48] = {};
    if (!t || !t->team_badge || t->team_badge(vehicle, key, sizeof key) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return std::string(key);
}

// The driver name the game shows for the car (the player's name for a human), UTF-8. Empty on a core whose live table
// is older than version 2 or while the driver names group is off.
inline std::optional<std::string> driver_name(uint32_t vehicle)
{
    auto t = detail::table();
    char name[128] = {};
    if (!t || t->version < 2 || !t->driver_name || t->driver_name(vehicle, name, sizeof name) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return std::string(name);
}

inline std::optional<std::string> team_badge_texture(uint32_t vehicle)
{
    auto t = detail::table();
    char path[96] = {};
    if (!t || t->version < 3 || !t->team_badge_texture ||
        t->team_badge_texture(vehicle, path, sizeof path) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return std::string(path);
}

// The fitted visual compound's compact icon, using the effective GameDB template.
// Empty on cores older than version 5 or when the mapping is unavailable.
inline std::optional<std::string> tyre_icon_texture(uint32_t vehicle)
{
    const auto *t = detail::table();
    char path[160] = {};
    if (!t || t->version < 5 || !t->tyre_icon_texture ||
        t->tyre_icon_texture(vehicle, path, sizeof path) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return std::string(path);
}

// More of the car's state from the same snapshot: starting grid slot, tyre wear, tyre age and stint laps, lap and sector
// times. Empty on cores older than version 6; each value's have bit says whether it was read.
inline std::optional<CarDetail> car_detail(uint32_t vehicle)
{
    const auto *t = detail::table();
    CarDetail d{};
    if (!t || t->version < 6 || !t->car_detail || t->car_detail(vehicle, &d) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return d;
}

// Caller-owned image bytes; no disk files or table presentation policy.
inline const SwoftyLiveImages *images()
{
    const auto *t = detail::table();
    return t && t->version >= 4 && t->images ? t->images() : nullptr;
}

inline std::optional<uint64_t> frame()
{
    auto t = detail::table();
    uint64_t f = 0;
    if (!t || !t->seq || t->seq(&f) != SWOFTY_ASSET_OK)
        return std::nullopt;
    return f;
}

inline bool has(const Car &c, uint64_t bit) { return (c.have & bit) != 0; }
inline bool has(const Session &s, uint64_t bit) { return (s.have & bit) != 0; }
inline bool has(const CarDetail &d, uint64_t bit) { return (d.have & bit) != 0; }

inline bool is_player(const Car &c) { return (c.flags & SWOFTY_LIVE_CAR_PLAYER) != 0; }
inline bool is_watched(const Car &c) { return (c.flags & SWOFTY_LIVE_CAR_WATCHED) != 0; }
inline bool is_racer(const Car &c) { return (c.flags & SWOFTY_LIVE_CAR_RACER) != 0; }
inline bool ai_driving(const Car &c) { return (c.flags & SWOFTY_LIVE_CAR_AI_DRIVING) != 0; }

inline std::optional<float> speed_kmh(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_SPEED) ? std::optional<float>(c.speed * 3.6f) : std::nullopt;
}

inline std::optional<float> ers_fraction(const Car &c)
{
    if (!has(c, SWOFTY_LIVE_HAVE_ERS_STORE) || c.ers_capacity <= 0)
        return std::nullopt;
    float f = c.ers_store / c.ers_capacity;
    return f < 0 ? 0.f : f > 1 ? 1.f : f;
}

inline std::optional<int32_t> gap_ahead_ms(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_GAP_AHEAD) && c.gap_ahead_ms != no_gap ? std::optional<int32_t>(c.gap_ahead_ms) : std::nullopt;
}

inline std::optional<int32_t> gap_leader_ms(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_GAP_LEADER) && c.gap_leader_ms != no_gap ? std::optional<int32_t>(c.gap_leader_ms) : std::nullopt;
}

inline std::optional<uint8_t> grid_position(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_GRID) ? std::optional<uint8_t>(d.grid_position) : std::nullopt;
}

inline std::optional<uint32_t> last_lap_ms(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_LAP_TIMES) && d.last_lap_ms ? std::optional<uint32_t>(d.last_lap_ms) : std::nullopt;
}

inline std::optional<uint32_t> best_lap_ms(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_LAP_TIMES) && d.best_lap_ms ? std::optional<uint32_t>(d.best_lap_ms) : std::nullopt;
}

inline bool in_pit(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_PIT) && c.pit_state >= 2 && c.pit_state <= 15;
}

inline std::optional<uint8_t> pit_status(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_PIT_LANE) ? std::optional<uint8_t>(c.pit_status) : std::nullopt;
}

inline std::optional<uint8_t> pit_stops(const Car &c)
{
    return has(c, SWOFTY_LIVE_HAVE_PIT_LANE) ? std::optional<uint8_t>(c.pit_stops) : std::nullopt;
}

enum class DriverStatus : uint8_t { garage = 0, flying_lap = 1, in_lap = 2, out_lap = 3, on_track = 4 };

// The session clock and lap count, from the game's UDP Session packet (see live.md for each field's source).
inline std::optional<float> time_left_s(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_TIME_LEFT) ? std::optional<float>(s.time_left_s) : std::nullopt;
}

inline std::optional<uint32_t> total_laps(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_TOTAL_LAPS) ? std::optional<uint32_t>(s.total_laps) : std::nullopt;
}

struct FastestLap {
    uint32_t vehicle;
    uint32_t lap_ms;
};

inline std::optional<FastestLap> fastest_lap(const Session &s)
{
    if (!has(s, SWOFTY_LIVE_SHAVE_FASTEST_LAP) || !s.fastest_lap_ms)
        return std::nullopt;
    return FastestLap{s.fastest_lap_vehicle, s.fastest_lap_ms};
}

inline std::optional<uint8_t> safety_car_phase(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_SAFETY_CAR_PHASE) ? std::optional<uint8_t>(s.safety_car_phase) : std::nullopt;
}

inline std::optional<bool> drs_enabled(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_DRS) ? std::optional<bool>(s.drs_enabled != 0) : std::nullopt;
}

inline std::optional<uint8_t> start_lights(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_START_LIGHTS) ? std::optional<uint8_t>(s.start_lights) : std::nullopt;
}

// Whether the game shows its race HUD now: false through menus, the track intro, the grid, cutscenes,
// replays, photo mode and the pause menu. Empty when the core cannot tell (no flow states seen).
inline std::optional<bool> hud_visible(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_HUD) ? std::optional<bool>(s.hud_visible != 0) : std::nullopt;
}

// What hides the race HUD, SWOFTY_LIVE_HUD_* bits; 0 while it shows or when the bit is clear.
inline uint8_t hud_hidden_by(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_HUD) ? s.hud_hidden_by : 0;
}

struct PitWindow {
    uint8_t ideal_lap, latest_lap, rejoin_position;
};

inline std::optional<PitWindow> pit_window(const Session &s)
{
    if (!has(s, SWOFTY_LIVE_SHAVE_PIT_WINDOW))
        return std::nullopt;
    return PitWindow{s.pit_window_ideal_lap, s.pit_window_latest_lap, s.pit_rejoin_position};
}

inline std::optional<bool> drs_allowed(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_DRS_ALLOWED) ? std::optional<bool>(d.drs_allowed != 0) : std::nullopt;
}

inline std::optional<bool> pit_limiter(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_PIT_LIMITER) ? std::optional<bool>(d.pit_limiter != 0) : std::nullopt;
}

inline std::optional<int8_t> fia_flag(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_FIA_FLAG) ? std::optional<int8_t>(d.fia_flag) : std::nullopt;
}

inline std::optional<DriverStatus> driver_status(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_DRIVER_STATUS) && d.driver_status <= 4
               ? std::optional<DriverStatus>(static_cast<DriverStatus>(d.driver_status))
               : std::nullopt;
}

inline std::optional<uint8_t> race_number(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_IDENTITY) ? std::optional<uint8_t>(d.race_number) : std::nullopt;
}

inline std::optional<uint32_t> team_colour(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_IDENTITY) && d.team_colour ? std::optional<uint32_t>(d.team_colour) : std::nullopt;
}

// The car's delta to the safety car's or the virtual safety car's required time, in seconds as the game sends it.
inline std::optional<float> safety_car_delta_s(const CarDetail &d)
{
    return has(d, SWOFTY_LIVE_HAVE_SC_DELTA) ? std::optional<float>(d.safety_car_delta_s) : std::nullopt;
}

// The car's best time in each sector this session, 0 for a sector it has no time in yet.
inline std::optional<std::array<uint32_t, 3>> sector_bests(const CarDetail &d)
{
    if (!has(d, SWOFTY_LIVE_HAVE_SECTOR_BESTS))
        return std::nullopt;
    return std::array<uint32_t, 3>{d.personal_best_sector_ms[0], d.personal_best_sector_ms[1], d.personal_best_sector_ms[2]};
}

// A car's driver by name, from the entrants.drivers row whose stat_id is the car's driver_id: code is the row's
// initials ("VER"), first_name and last_name its names in the game's language (the lng_* text, or the stored
// string when it is not a key), label the row's _metadata_, name_reversed its is_name_reversed (family name first).
struct DriverIdentity {
    uint32_t stat_id = 0;
    std::string code, first_name, last_name, label;
    bool name_reversed = false;
};

namespace detail {

inline std::string driver_text(const db::Cursor *c, const char *field)
{
    db::Value v{};
    if (db::cell(c, field, &v) != db::Result::ok || !v.s || !v.s[0])
        return {};
    std::string stored = v.s;
    if (stored.rfind("lng_", 0) != 0)
        return stored == "default" ? std::string{} : stored;
    return lang::text(stored).value_or(std::string{});
}

}

// The driver of the entrants.drivers row with this stat_id. Empty while the database is not loaded, when no row has
// it, and when the id is ambiguous: the 2024 and 2025 UDP formats carry one byte, so a driver id below 255 that is
// also the low byte of an added driver's stat_id names no single row. Reads the database at each call.
inline std::optional<DriverIdentity> driver_identity(uint32_t driver_id)
{
    if (driver_id == 255 || !db::available())
        return std::nullopt;
    std::optional<DriverIdentity> found;
    int matches = 0;
    db::Cursor cur{};
    for (db::Result r = db::open("entrants.drivers", &cur); r == db::Result::ok; r = db::step(&cur)) {
        db::Value id{};
        if (db::cell(&cur, "stat_id", &id) != db::Result::ok || id.i < 0)
            continue;
        uint32_t stat = static_cast<uint32_t>(id.i);
        if (stat != driver_id && !(driver_id < 255 && (stat & 0xff) == driver_id))
            continue;
        if (++matches > 1)
            return std::nullopt;
        DriverIdentity d;
        d.stat_id = stat;
        d.code = detail::driver_text(&cur, "initials");
        d.first_name = detail::driver_text(&cur, "first_name");
        d.last_name = detail::driver_text(&cur, "last_name");
        if (const char *label = db::row_name(&cur))
            d.label = label;
        db::Value reversed{};
        d.name_reversed = db::cell(&cur, "is_name_reversed", &reversed) == db::Result::ok && reversed.i != 0;
        found = std::move(d);
    }
    return found;
}

// The car's driver by name (driver_identity above), from its UDP driver id; empty without SWOFTY_LIVE_HAVE_IDENTITY.
inline std::optional<DriverIdentity> driver_identity(const CarDetail &d)
{
    if (!has(d, SWOFTY_LIVE_HAVE_IDENTITY))
        return std::nullopt;
    return driver_identity(d.driver_id);
}

inline GameMode game_mode(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_GAME_MODE) ? static_cast<GameMode>(s.game_mode) : GameMode::unknown;
}

inline SafetyCar safety_car(const Session &s)
{
    return has(s, SWOFTY_LIVE_SHAVE_SAFETY_CAR) ? static_cast<SafetyCar>(s.safety_car) : SafetyCar::unknown;
}

struct Order {
    std::vector<uint32_t> ids;
    int player_rank = -1;
    int subject_rank = -1;

    int rank_of(uint32_t vehicle) const
    {
        for (size_t i = 0; i < ids.size(); i++)
            if (ids[i] == vehicle)
                return static_cast<int>(i);
        return -1;
    }

    std::optional<Car> ahead(uint32_t vehicle) const
    {
        int r = rank_of(vehicle);
        if (r < 0 || ids.size() < 2)
            return std::nullopt;
        return car(ids[r == 0 ? 1 : static_cast<size_t>(r) - 1]);
    }

    std::optional<Car> behind(uint32_t vehicle) const
    {
        int r = rank_of(vehicle);
        if (r < 0 || static_cast<size_t>(r) + 1 >= ids.size())
            return std::nullopt;
        return car(ids[static_cast<size_t>(r) + 1]);
    }
};

inline Order order()
{
    Order o;
    auto s = session();
    if (!s || !has(*s, SWOFTY_LIVE_SHAVE_ORDER))
        return o;
    o.ids.assign(s->order, s->order + (s->classified < max_cars ? s->classified : max_cars));
    if (s->player >= 0)
        o.player_rank = o.rank_of(static_cast<uint32_t>(s->player));
    if (s->subject >= 0)
        o.subject_rank = o.rank_of(static_cast<uint32_t>(s->subject));
    return o;
}

inline std::vector<uint32_t> road_order()
{
    auto s = session();
    if (!s || !has(*s, SWOFTY_LIVE_SHAVE_ROAD_ORDER))
        return {};
    return std::vector<uint32_t>(s->road_order, s->road_order + (s->on_track < max_cars ? s->on_track : max_cars));
}

}
