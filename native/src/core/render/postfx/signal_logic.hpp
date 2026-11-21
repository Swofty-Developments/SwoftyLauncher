#pragma once
#include <cstddef>
#include <cstdint>
#include <string_view>

namespace render::postfx {

enum SignalKind : uint8_t { SK_BOOL, SK_NAME, SK_NUMBER, SK_EVENT };

struct SignalInfo {
    std::string_view id;
    SignalKind kind;
    std::string_view values;
    std::string_view unit;
    bool in_game;
};

enum Sig : int {
    S_VIEW_CAMERA,
    S_VIEW_REPLAY,
    S_VIEW_PAUSED,
    S_VIEW_PHOTO,
    S_VIEW_MENU,
    S_VIEW_CUTSCENE,
    S_VIEW_SPECTATING,
    S_SESSION_START,
    S_RACE_START,
    S_SESSION_LAP,
    S_SESSION_END,
    S_CAR_SPEED,
    S_CAR_RPM,
    S_CAR_GEAR,
    S_CAR_THROTTLE,
    S_CAR_BRAKE,
    S_CAR_STEER,
    S_CAR_G_LATERAL,
    S_CAR_G_LONG,
    S_CAR_SURFACE,
    S_CAR_IMPACT,
    S_CAR_DAMAGE,
    S_CAR_IN_PIT_LANE,
    S_CAR_OFF_TRACK,
    S_SESSION_FLAG,
    S_SESSION_SAFETY_CAR,
    S_SESSION_WEATHER,
    S_SESSION_WETNESS,
    S_SESSION_FLASHBACK,
    S_SESSION_CHEQUERED,
    S_CAR_WETNESS,
    SIGNAL_COUNT
};

inline constexpr SignalInfo SIGNALS[SIGNAL_COUNT] = {
    {"view.camera", SK_NAME, "cockpit tcam tcam_offset chase_near chase_far other trackside reverse_pod", "", false},
    {"view.replay", SK_BOOL, "", "", true},
    {"view.paused", SK_BOOL, "", "", true},
    {"view.photo", SK_BOOL, "", "", true},
    {"view.menu", SK_BOOL, "", "", true},
    {"view.cutscene", SK_BOOL, "", "", true},
    {"view.spectating", SK_BOOL, "", "", true},
    {"session.start", SK_EVENT, "", "", true},
    {"race.start", SK_EVENT, "", "", true},
    {"session.lap", SK_EVENT, "", "", true},
    {"session.end", SK_EVENT, "", "", true},
    {"car.speed", SK_NUMBER, "", "km/h", true},
    {"car.rpm", SK_NUMBER, "", "", true},
    {"car.gear", SK_NUMBER, "", "", true},
    {"car.throttle", SK_NUMBER, "", "", true},
    {"car.brake", SK_NUMBER, "", "", true},
    {"car.steer", SK_NUMBER, "", "", true},
    {"car.g_lateral", SK_NUMBER, "", "g", true},
    {"car.g_long", SK_NUMBER, "", "g", true},
    {"car.surface", SK_NAME, "tarmac kerb rumble grass gravel sand", "", true},
    {"car.impact", SK_EVENT, "", "g", false},
    {"car.damage", SK_EVENT, "", "", false},
    {"car.in_pit_lane", SK_BOOL, "", "", false},
    {"car.off_track", SK_BOOL, "", "", false},
    {"session.flag", SK_NAME, "none green blue yellow red", "", false},
    {"session.safety_car", SK_NAME, "none sc vsc formation", "", false},
    {"session.weather", SK_NAME, "clear light_cloud overcast light_rain heavy_rain storm", "", false},
    {"session.wetness", SK_NUMBER, "", "", true},
    {"session.flashback", SK_EVENT, "", "", false},
    {"session.chequered", SK_EVENT, "", "", false},
    {"car.wetness", SK_NUMBER, "", "", true},
};

inline int signal_index(std::string_view id)
{
    for (int i = 0; i < SIGNAL_COUNT; i++)
        if (SIGNALS[i].id == id)
            return i;
    return -1;
}

// The name of value i of a name signal ("" past the end).
inline std::string_view signal_value_name(int signal, size_t i)
{
    std::string_view v = SIGNALS[signal].values;
    size_t at = 0;
    for (size_t n = 0;; n++) {
        size_t end = v.find(' ', at);
        std::string_view word = v.substr(at, end == std::string_view::npos ? end : end - at);
        if (n == i)
            return word;
        if (end == std::string_view::npos)
            return {};
        at = end + 1;
    }
}

// The game's surface id (UDP m_surfaceType) as a value of car.surface: tarmac 0, kerb 1, rumble 2, grass 3,
// gravel 4, sand 5; -1 for no value (12 = no contact, or an id past the table).
inline int surface_value(int game_id)
{
    static constexpr int8_t MAP[12] = {0, 1, 0, 4, 4, 4, 5, 3, 0, 0, 0, 2};
    return game_id >= 0 && game_id < 12 ? MAP[game_id] : -1;
}

// The game keeps wheels as RL RR FL FR; the contract's order is FL FR RL RR.
inline void wheels_contract_order(const uint8_t game[4], uint8_t out[4])
{
    out[0] = game[2];
    out[1] = game[3];
    out[2] = game[0];
    out[3] = game[1];
}

// What the signals read at one presented frame. A signal without a value has has[] false: conditions on
// it fail and its events never occur. Bool signals hold 0 or 1, name signals the index of the value,
// numbers the number; car.surface holds one index per wheel (FL FR RL RR) in wheel[] (-1 for none). An
// event holds the magnitude of each occurrence this frame.
struct Signals {
    static constexpr size_t MAX_EVENTS = 4;
    bool has[SIGNAL_COUNT];
    double num[SIGNAL_COUNT];
    int wheel[4];
    uint8_t n_events[SIGNAL_COUNT];
    double magnitude[SIGNAL_COUNT][MAX_EVENTS];

    Signals() { clear(); }

    void clear()
    {
        for (int i = 0; i < SIGNAL_COUNT; i++) {
            has[i] = false;
            num[i] = 0;
            n_events[i] = 0;
        }
        for (int &w : wheel)
            w = -1;
    }

    void clear_events()
    {
        for (int i = 0; i < SIGNAL_COUNT; i++)
            n_events[i] = 0;
    }

    void set(int signal, double v)
    {
        has[signal] = true;
        num[signal] = v;
    }

    void set_surface(const int w[4])
    {
        has[S_CAR_SURFACE] = true;
        for (int i = 0; i < 4; i++)
            wheel[i] = w[i];
    }

    void add_event(int signal, double magnitude_ = 0)
    {
        if (n_events[signal] < MAX_EVENTS)
            magnitude[signal][n_events[signal]++] = magnitude_;
    }
};

}
