/* The C mirror of every standard library 1.6 event payload, with its x86_64 size and field offsets.
   sdk_api_c_test.c checks the mirror compiles as C with these numbers; sdk_headers_test.cpp checks the
   C++ payload structs have the same size and offsets, and are standard layout and trivially copyable. */
#ifndef SWOFTY_TEST_SDK_PAYLOADS_H
#define SWOFTY_TEST_SDK_PAYLOADS_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct SwoftyEvShell {
    uint32_t size, payload_version;
} SwoftyEvShell;

typedef struct SwoftyEvCar {
    uint32_t index;
    void *handle;
} SwoftyEvCar;

typedef struct SwoftyEvSeasonStart {
    uint32_t size, payload_version;
    int32_t year;
    int32_t season_index;
} SwoftyEvSeasonStart;

typedef struct SwoftyEvAddActivity {
    uint32_t size, payload_version;
    void *calendar;
    const void *activity;
    uint32_t driver;
    uint32_t day;
    uint8_t type;
    bool player;
} SwoftyEvAddActivity;

typedef struct SwoftyEvContractDecide {
    uint32_t size, payload_version;
    void *state;
    uint32_t link;
} SwoftyEvContractDecide;

typedef struct SwoftyEvContractSign {
    uint32_t size, payload_version;
    void *contracts;
    void *contract;
    uint32_t driver;
    uint32_t replaced;
    uint32_t team;
    uint8_t type;
    bool notify;
} SwoftyEvContractSign;

typedef struct SwoftyEvFuelLoad {
    uint32_t size, payload_version;
    const char *track_id;
    uint32_t session_type;
    float kg;
    uint8_t kind;
} SwoftyEvFuelLoad;

typedef struct SwoftyEvStandingsRebuild {
    uint32_t size, payload_version;
    void *championship;
} SwoftyEvStandingsRebuild;

typedef struct SwoftyEvUpgradesFit {
    uint32_t size, payload_version;
    int32_t car;
    const char *vehicle_id;
    int32_t department;
    float driver_stat;
    float rating;
} SwoftyEvUpgradesFit;

typedef struct SwoftyEvVtfRead {
    uint32_t size, payload_version;
    SwoftyEvCar car;
    const char *section_str;
    const char *field_str;
    const char *upgrade_str;
    float value;
} SwoftyEvVtfRead;

typedef struct SwoftyEvBuilt {
    uint32_t size, payload_version;
    SwoftyEvCar car;
} SwoftyEvBuilt;

typedef struct SwoftyEvReset {
    uint32_t size, payload_version;
    SwoftyEvCar car;
    bool upgrade;
} SwoftyEvReset;

typedef struct SwoftyEvControllerUpdate {
    uint32_t size, payload_version;
    int32_t vehicle;
    float throttle;
    float brake;
    float steering;
} SwoftyEvControllerUpdate;

typedef struct SwoftyEvPlayerControl {
    uint32_t size, payload_version;
    SwoftyEvCar car;
    int32_t from, to;
    int32_t reason;
} SwoftyEvPlayerControl;

typedef struct SwoftyEvLap {
    uint32_t size, payload_version;
    SwoftyEvCar car;
} SwoftyEvLap;

typedef struct SwoftyEvRaceFlag {
    uint32_t size, payload_version;
    uint8_t flag, previous;
    uint8_t yellow_sectors, previous_yellow_sectors;
    int32_t lap;
} SwoftyEvRaceFlag;

typedef struct SwoftyEvSafetyCar {
    uint32_t size, payload_version;
    uint8_t deployment, previous;
    int32_t lap;
} SwoftyEvSafetyCar;

typedef struct SwoftyEvRedFlag {
    uint32_t size, payload_version;
    uint8_t state, previous;
    int32_t lap;
} SwoftyEvRedFlag;

typedef struct SwoftyEvSector {
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t sector, previous;
    int32_t lap;
} SwoftyEvSector;

typedef struct SwoftyEvCarStatus {
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t status, previous;
    int32_t lap;
} SwoftyEvCarStatus;

typedef struct SwoftyEvPenalty {
    uint32_t size, payload_version;
    uint32_t car, other_car;
    uint8_t penalty_type, infringement, seconds, places_gained;
    int32_t lap;
} SwoftyEvPenalty;

typedef struct SwoftyEvPenaltyServed {
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t kind, pad[3];
    float stop_s;
    int32_t lap;
} SwoftyEvPenaltyServed;

typedef struct SwoftyEvDrs {
    uint32_t size, payload_version;
    uint8_t enabled, reason, pad[2];
    int32_t lap;
} SwoftyEvDrs;

typedef struct SwoftyEvStartLights {
    uint32_t size, payload_version;
    uint8_t lights, out, pad[2];
    int32_t lap;
} SwoftyEvStartLights;

typedef struct SwoftyEvSafetyCarPhase {
    uint32_t size, payload_version;
    uint8_t deployment, phase, previous, pad;
    int32_t lap;
} SwoftyEvSafetyCarPhase;

typedef struct SwoftyEvFastestLap {
    uint32_t size, payload_version;
    uint32_t car, lap_ms, previous_car, previous_ms;
    int32_t lap;
} SwoftyEvFastestLap;

typedef struct SwoftyEvClassifiedCar {
    uint32_t car;
    uint8_t position, laps, grid, points, pit_stops, status, reason, penalties_s;
    uint32_t best_lap_ms, race_ms;
} SwoftyEvClassifiedCar;

typedef struct SwoftyEvClassification {
    uint32_t size, payload_version;
    uint32_t cars;
    SwoftyEvClassifiedCar entries[24];
} SwoftyEvClassification;

typedef struct SwoftyEvControllersRebuilt {
    uint32_t size, payload_version;
    SwoftyEvCar car;
    uint32_t reason;
} SwoftyEvControllersRebuilt;

typedef struct SwoftyEvHudVisible {
    uint32_t size, payload_version;
    SwoftyEvCar following;
    bool visible;
} SwoftyEvHudVisible;

typedef struct SwoftyEvLapsRecorded {
    uint32_t size, payload_version;
    const char *path_str;
    int32_t time_ms;
    int32_t valid;
} SwoftyEvLapsRecorded;

typedef struct SwoftyEvLapsSealing {
    uint32_t size, payload_version;
    const char *path_str;
    int32_t time_ms;
    const uint8_t *body;
    uint32_t body_len;
    int32_t (*add_section_fn)(void *user, const char *tag, const void *bytes, uint32_t len);
    void *user;
} SwoftyEvLapsSealing;

typedef struct SwoftyEvPartsRegister {
    uint32_t size, payload_version;
    int32_t car;
    const char *vehicle_id;
    void *front_wing;
    int32_t (*add_fn)(void *user, const char *node, float detach_at, bool debris);
    void *user;
} SwoftyEvPartsRegister;

typedef struct SwoftyEvBundleResolve {
    uint32_t size, payload_version;
    int32_t car;
    const char *vehicle_id;
    const char *switch_name_str;
    const char *engine_sound_id;
    int32_t (*set_engine_sound_fn)(void *user, const char *id);
    void *user;
} SwoftyEvBundleResolve;

typedef struct SwoftyEvLogoResolve {
    uint32_t size, payload_version;
    int32_t context;
    const char *team_id;
    const char *livery_name_str;
    const char *key_str;
    int32_t (*set_key_fn)(void *user, const char *key);
    void *user;
} SwoftyEvLogoResolve;

typedef struct SwoftyEvNameResolve {
    uint32_t size, payload_version;
    const char *team_id;
    const char *name_str;
    bool name_literal;
    int32_t (*set_name_fn)(void *user, const char *name, bool literal);
    void *user;
} SwoftyEvNameResolve;

typedef struct SwoftyEvRatingsRead {
    uint32_t size, payload_version;
    const char *driver_id;
    int32_t stat;
    float value;
} SwoftyEvRatingsRead;

typedef struct SwoftyEvNumberAssign {
    uint32_t size, payload_version;
    const char *driver_id;
    int32_t number;
} SwoftyEvNumberAssign;

typedef struct SwoftyEvHeadResolve {
    uint32_t size, payload_version;
    const char *driver_id;
    const char *head_id;
    int32_t (*set_head_fn)(void *user, const char *id);
    void *user;
} SwoftyEvHeadResolve;

typedef struct SwoftyEvCatalogueBuild {
    uint32_t size, payload_version;
    int32_t (*add_fn)(void *user, const char *registry, const char *id);
    void *user;
} SwoftyEvCatalogueBuild;

typedef struct SwoftyEvWeatherSlot {
    uint8_t kind;
    uint8_t reserved[3];
    float duration_s, transition_s, intensity;
} SwoftyEvWeatherSlot;

typedef struct SwoftyEvWeatherSetup {
    uint32_t size, payload_version;
    uint32_t session_type;
    uint8_t game_scripted, time_trial, seed_forced, set_seed;
    uint32_t seed;
    uint8_t script_mask;
    uint8_t counts[8];
    uint8_t reserved[3];
    SwoftyEvWeatherSlot slots[8][6];
} SwoftyEvWeatherSetup;

typedef struct SwoftyEvTyreDamage {
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t wheel, state, previous, reserved;
    float deflation;
    int32_t lap;
} SwoftyEvTyreDamage;

typedef struct SwoftyEvPitStopTyres {
    uint32_t size, payload_version;
    uint32_t car;
    uint8_t kept;
    uint8_t reserved[3];
} SwoftyEvPitStopTyres;

/* X(C struct, C++ namespace, C++ struct, size) */
#define SWOFTY_EV_PAYLOADS(X)                                                                                                    \
    X(SwoftyEvShell, career, Created, 8)                                                                                         \
    X(SwoftyEvShell, career, Loaded, 8)                                                                                          \
    X(SwoftyEvShell, career, Unloaded, 8)                                                                                        \
    X(SwoftyEvShell, career, SeasonEnd, 8)                                                                                       \
    X(SwoftyEvShell, career, SeasonBreak, 8)                                                                                     \
    X(SwoftyEvShell, career, Saving, 8)                                                                                          \
    X(SwoftyEvSeasonStart, career, SeasonStart, 16)                                                                              \
    X(SwoftyEvAddActivity, career, AddActivity, 40)                                                                              \
    X(SwoftyEvContractDecide, career, ContractDecide, 24)                                                                        \
    X(SwoftyEvContractSign, career, ContractSign, 40)                                                                            \
    X(SwoftyEvFuelLoad, tracks, FuelLoad, 32)                                                                                    \
    X(SwoftyEvStandingsRebuild, career, StandingsRebuild, 16)                                                                    \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, 40)                                                                                \
    X(SwoftyEvVtfRead, cars, VtfRead, 56)                                                                                        \
    X(SwoftyEvBuilt, cars, Built, 24)                                                                                            \
    X(SwoftyEvReset, cars, Reset, 32)                                                                                            \
    X(SwoftyEvControllerUpdate, ai, ControllerUpdate, 24)                                                                        \
    X(SwoftyEvPlayerControl, sessions, PlayerControl, 40)                                                                        \
    X(SwoftyEvLap, sessions, Lap, 24)                                                                                            \
    X(SwoftyEvRaceFlag, race_control, Flag, 16)                                                                                  \
    X(SwoftyEvSafetyCar, race_control, SafetyCar, 16)                                                                            \
    X(SwoftyEvRedFlag, race_control, RedFlag, 16)                                                                                \
    X(SwoftyEvSector, race_control, Sector, 20)                                                                                  \
    X(SwoftyEvCarStatus, race_control, CarStatus, 20)                                                                            \
    X(SwoftyEvPenalty, race_control, Penalty, 24)                                                                                \
    X(SwoftyEvPenaltyServed, race_control, PenaltyServed, 24)                                                                    \
    X(SwoftyEvDrs, race_control, Drs, 16)                                                                                        \
    X(SwoftyEvStartLights, race_control, StartLights, 16)                                                                        \
    X(SwoftyEvSafetyCarPhase, race_control, SafetyCarPhase, 16)                                                                  \
    X(SwoftyEvFastestLap, race_control, FastestLap, 28)                                                                          \
    X(SwoftyEvClassification, race_control, Classification, 492)                                                                 \
    X(SwoftyEvControllersRebuilt, cars, ControllersRebuilt, 32)                                                                  \
    X(SwoftyEvHudVisible, spectator, HudVisible, 32)                                                                             \
    X(SwoftyEvLapsRecorded, laps, Recorded, 24)                                                                                  \
    X(SwoftyEvLapsSealing, laps, Sealing, 56)                                                                                    \
    X(SwoftyEvPartsRegister, cars, PartsRegister, 48)                                                                            \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, 56)                                                                          \
    X(SwoftyEvLogoResolve, teams, LogoResolve, 56)                                                                               \
    X(SwoftyEvNameResolve, teams, NameResolve, 48)                                                                               \
    X(SwoftyEvRatingsRead, drivers, RatingsRead, 24)                                                                             \
    X(SwoftyEvNumberAssign, drivers, NumberAssign, 24)                                                                           \
    X(SwoftyEvHeadResolve, drivers, HeadResolve, 40)                                                                             \
    X(SwoftyEvCatalogueBuild, wearables, CatalogueBuild, 24)                                                                     \
    X(SwoftyEvWeatherSetup, weather, Setup, 800)                                                                                 \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, 24)                                                                               \
    X(SwoftyEvPitStopTyres, sessions, PitStopTyres, 16)

/* X(C struct, C++ namespace, C++ struct, field, offset) */
#define SWOFTY_EV_FIELDS(X)                                                                                                      \
    X(SwoftyEvSeasonStart, career, SeasonStart, size, 0)                                                                         \
    X(SwoftyEvSeasonStart, career, SeasonStart, payload_version, 4)                                                              \
    X(SwoftyEvSeasonStart, career, SeasonStart, year, 8)                                                                         \
    X(SwoftyEvSeasonStart, career, SeasonStart, season_index, 12)                                                                \
    X(SwoftyEvAddActivity, career, AddActivity, calendar, 8)                                                                     \
    X(SwoftyEvAddActivity, career, AddActivity, activity, 16)                                                                    \
    X(SwoftyEvAddActivity, career, AddActivity, driver, 24)                                                                      \
    X(SwoftyEvAddActivity, career, AddActivity, day, 28)                                                                         \
    X(SwoftyEvAddActivity, career, AddActivity, type, 32)                                                                        \
    X(SwoftyEvAddActivity, career, AddActivity, player, 33)                                                                      \
    X(SwoftyEvContractDecide, career, ContractDecide, state, 8)                                                                  \
    X(SwoftyEvContractDecide, career, ContractDecide, link, 16)                                                                  \
    X(SwoftyEvContractSign, career, ContractSign, contracts, 8)                                                                  \
    X(SwoftyEvContractSign, career, ContractSign, contract, 16)                                                                  \
    X(SwoftyEvContractSign, career, ContractSign, driver, 24)                                                                    \
    X(SwoftyEvContractSign, career, ContractSign, replaced, 28)                                                                  \
    X(SwoftyEvContractSign, career, ContractSign, team, 32)                                                                      \
    X(SwoftyEvContractSign, career, ContractSign, type, 36)                                                                      \
    X(SwoftyEvContractSign, career, ContractSign, notify, 37)                                                                    \
    X(SwoftyEvFuelLoad, tracks, FuelLoad, track_id, 8)                                                                           \
    X(SwoftyEvFuelLoad, tracks, FuelLoad, session_type, 16)                                                                      \
    X(SwoftyEvFuelLoad, tracks, FuelLoad, kg, 20)                                                                                \
    X(SwoftyEvFuelLoad, tracks, FuelLoad, kind, 24)                                                                              \
    X(SwoftyEvStandingsRebuild, career, StandingsRebuild, championship, 8)                                                       \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, size, 0)                                                                           \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, payload_version, 4)                                                                \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, car, 8)                                                                            \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, vehicle_id, 16)                                                                    \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, department, 24)                                                                    \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, driver_stat, 28)                                                                   \
    X(SwoftyEvUpgradesFit, cars, UpgradesFit, rating, 32)                                                                        \
    X(SwoftyEvVtfRead, cars, VtfRead, car, 8)                                                                                    \
    X(SwoftyEvVtfRead, cars, VtfRead, section_str, 24)                                                                           \
    X(SwoftyEvVtfRead, cars, VtfRead, field_str, 32)                                                                             \
    X(SwoftyEvVtfRead, cars, VtfRead, upgrade_str, 40)                                                                           \
    X(SwoftyEvVtfRead, cars, VtfRead, value, 48)                                                                                 \
    X(SwoftyEvBuilt, cars, Built, car, 8)                                                                                        \
    X(SwoftyEvReset, cars, Reset, car, 8)                                                                                        \
    X(SwoftyEvReset, cars, Reset, upgrade, 24)                                                                                   \
    X(SwoftyEvControllerUpdate, ai, ControllerUpdate, vehicle, 8)                                                                \
    X(SwoftyEvControllerUpdate, ai, ControllerUpdate, throttle, 12)                                                              \
    X(SwoftyEvControllerUpdate, ai, ControllerUpdate, brake, 16)                                                                 \
    X(SwoftyEvControllerUpdate, ai, ControllerUpdate, steering, 20)                                                              \
    X(SwoftyEvPlayerControl, sessions, PlayerControl, car, 8)                                                                    \
    X(SwoftyEvPlayerControl, sessions, PlayerControl, from, 24)                                                                  \
    X(SwoftyEvPlayerControl, sessions, PlayerControl, to, 28)                                                                    \
    X(SwoftyEvPlayerControl, sessions, PlayerControl, reason, 32)                                                                \
    X(SwoftyEvLap, sessions, Lap, car, 8)                                                                                        \
    X(SwoftyEvRaceFlag, race_control, Flag, flag, 8)                                                                             \
    X(SwoftyEvRaceFlag, race_control, Flag, previous, 9)                                                                         \
    X(SwoftyEvRaceFlag, race_control, Flag, yellow_sectors, 10)                                                                  \
    X(SwoftyEvRaceFlag, race_control, Flag, previous_yellow_sectors, 11)                                                         \
    X(SwoftyEvRaceFlag, race_control, Flag, lap, 12)                                                                             \
    X(SwoftyEvSafetyCar, race_control, SafetyCar, deployment, 8)                                                                 \
    X(SwoftyEvSafetyCar, race_control, SafetyCar, previous, 9)                                                                   \
    X(SwoftyEvSafetyCar, race_control, SafetyCar, lap, 12)                                                                       \
    X(SwoftyEvRedFlag, race_control, RedFlag, state, 8)                                                                          \
    X(SwoftyEvRedFlag, race_control, RedFlag, previous, 9)                                                                       \
    X(SwoftyEvRedFlag, race_control, RedFlag, lap, 12)                                                                           \
    X(SwoftyEvSector, race_control, Sector, car, 8)                                                                              \
    X(SwoftyEvSector, race_control, Sector, sector, 12)                                                                          \
    X(SwoftyEvSector, race_control, Sector, previous, 13)                                                                        \
    X(SwoftyEvSector, race_control, Sector, lap, 16)                                                                             \
    X(SwoftyEvCarStatus, race_control, CarStatus, car, 8)                                                                        \
    X(SwoftyEvCarStatus, race_control, CarStatus, status, 12)                                                                    \
    X(SwoftyEvCarStatus, race_control, CarStatus, previous, 13)                                                                  \
    X(SwoftyEvCarStatus, race_control, CarStatus, lap, 16)                                                                       \
    X(SwoftyEvPenalty, race_control, Penalty, car, 8)                                                                            \
    X(SwoftyEvPenalty, race_control, Penalty, other_car, 12)                                                                     \
    X(SwoftyEvPenalty, race_control, Penalty, penalty_type, 16)                                                                  \
    X(SwoftyEvPenalty, race_control, Penalty, infringement, 17)                                                                  \
    X(SwoftyEvPenalty, race_control, Penalty, seconds, 18)                                                                       \
    X(SwoftyEvPenalty, race_control, Penalty, places_gained, 19)                                                                 \
    X(SwoftyEvPenalty, race_control, Penalty, lap, 20)                                                                           \
    X(SwoftyEvPenaltyServed, race_control, PenaltyServed, car, 8)                                                                \
    X(SwoftyEvPenaltyServed, race_control, PenaltyServed, kind, 12)                                                              \
    X(SwoftyEvPenaltyServed, race_control, PenaltyServed, stop_s, 16)                                                            \
    X(SwoftyEvPenaltyServed, race_control, PenaltyServed, lap, 20)                                                               \
    X(SwoftyEvDrs, race_control, Drs, enabled, 8)                                                                                \
    X(SwoftyEvDrs, race_control, Drs, reason, 9)                                                                                 \
    X(SwoftyEvDrs, race_control, Drs, lap, 12)                                                                                   \
    X(SwoftyEvStartLights, race_control, StartLights, lights, 8)                                                                 \
    X(SwoftyEvStartLights, race_control, StartLights, out, 9)                                                                    \
    X(SwoftyEvStartLights, race_control, StartLights, lap, 12)                                                                   \
    X(SwoftyEvSafetyCarPhase, race_control, SafetyCarPhase, deployment, 8)                                                       \
    X(SwoftyEvSafetyCarPhase, race_control, SafetyCarPhase, phase, 9)                                                            \
    X(SwoftyEvSafetyCarPhase, race_control, SafetyCarPhase, previous, 10)                                                        \
    X(SwoftyEvSafetyCarPhase, race_control, SafetyCarPhase, lap, 12)                                                             \
    X(SwoftyEvFastestLap, race_control, FastestLap, car, 8)                                                                      \
    X(SwoftyEvFastestLap, race_control, FastestLap, lap_ms, 12)                                                                  \
    X(SwoftyEvFastestLap, race_control, FastestLap, previous_car, 16)                                                            \
    X(SwoftyEvFastestLap, race_control, FastestLap, previous_ms, 20)                                                             \
    X(SwoftyEvFastestLap, race_control, FastestLap, lap, 24)                                                                     \
    X(SwoftyEvClassification, race_control, Classification, cars, 8)                                                             \
    X(SwoftyEvClassification, race_control, Classification, entries, 12)                                                         \
    X(SwoftyEvControllersRebuilt, cars, ControllersRebuilt, car, 8)                                                              \
    X(SwoftyEvControllersRebuilt, cars, ControllersRebuilt, reason, 24)                                                          \
    X(SwoftyEvHudVisible, spectator, HudVisible, following, 8)                                                                   \
    X(SwoftyEvHudVisible, spectator, HudVisible, visible, 24)                                                                    \
    X(SwoftyEvLapsRecorded, laps, Recorded, path_str, 8)                                                                         \
    X(SwoftyEvLapsRecorded, laps, Recorded, time_ms, 16)                                                                         \
    X(SwoftyEvLapsRecorded, laps, Recorded, valid, 20)                                                                           \
    X(SwoftyEvLapsSealing, laps, Sealing, path_str, 8)                                                                           \
    X(SwoftyEvLapsSealing, laps, Sealing, time_ms, 16)                                                                           \
    X(SwoftyEvLapsSealing, laps, Sealing, body, 24)                                                                              \
    X(SwoftyEvLapsSealing, laps, Sealing, body_len, 32)                                                                          \
    X(SwoftyEvLapsSealing, laps, Sealing, add_section_fn, 40)                                                                    \
    X(SwoftyEvLapsSealing, laps, Sealing, user, 48)                                                                              \
    X(SwoftyEvPartsRegister, cars, PartsRegister, car, 8)                                                                        \
    X(SwoftyEvPartsRegister, cars, PartsRegister, vehicle_id, 16)                                                                \
    X(SwoftyEvPartsRegister, cars, PartsRegister, front_wing, 24)                                                                \
    X(SwoftyEvPartsRegister, cars, PartsRegister, add_fn, 32)                                                                    \
    X(SwoftyEvPartsRegister, cars, PartsRegister, user, 40)                                                                      \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, car, 8)                                                                      \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, vehicle_id, 16)                                                              \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, switch_name_str, 24)                                                         \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, engine_sound_id, 32)                                                         \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, set_engine_sound_fn, 40)                                                     \
    X(SwoftyEvBundleResolve, sounds, BundleResolve, user, 48)                                                                    \
    X(SwoftyEvLogoResolve, teams, LogoResolve, context, 8)                                                                       \
    X(SwoftyEvLogoResolve, teams, LogoResolve, team_id, 16)                                                                      \
    X(SwoftyEvLogoResolve, teams, LogoResolve, livery_name_str, 24)                                                              \
    X(SwoftyEvLogoResolve, teams, LogoResolve, key_str, 32)                                                                      \
    X(SwoftyEvLogoResolve, teams, LogoResolve, set_key_fn, 40)                                                                   \
    X(SwoftyEvLogoResolve, teams, LogoResolve, user, 48)                                                                         \
    X(SwoftyEvNameResolve, teams, NameResolve, team_id, 8)                                                                       \
    X(SwoftyEvNameResolve, teams, NameResolve, name_str, 16)                                                                     \
    X(SwoftyEvNameResolve, teams, NameResolve, name_literal, 24)                                                                 \
    X(SwoftyEvNameResolve, teams, NameResolve, set_name_fn, 32)                                                                  \
    X(SwoftyEvNameResolve, teams, NameResolve, user, 40)                                                                         \
    X(SwoftyEvRatingsRead, drivers, RatingsRead, driver_id, 8)                                                                   \
    X(SwoftyEvRatingsRead, drivers, RatingsRead, stat, 16)                                                                       \
    X(SwoftyEvRatingsRead, drivers, RatingsRead, value, 20)                                                                      \
    X(SwoftyEvNumberAssign, drivers, NumberAssign, driver_id, 8)                                                                 \
    X(SwoftyEvNumberAssign, drivers, NumberAssign, number, 16)                                                                   \
    X(SwoftyEvHeadResolve, drivers, HeadResolve, driver_id, 8)                                                                   \
    X(SwoftyEvHeadResolve, drivers, HeadResolve, head_id, 16)                                                                    \
    X(SwoftyEvHeadResolve, drivers, HeadResolve, set_head_fn, 24)                                                                \
    X(SwoftyEvHeadResolve, drivers, HeadResolve, user, 32)                                                                       \
    X(SwoftyEvCatalogueBuild, wearables, CatalogueBuild, add_fn, 8)                                                              \
    X(SwoftyEvCatalogueBuild, wearables, CatalogueBuild, user, 16)                                                               \
    X(SwoftyEvWeatherSetup, weather, Setup, session_type, 8)                                                                     \
    X(SwoftyEvWeatherSetup, weather, Setup, game_scripted, 12)                                                                   \
    X(SwoftyEvWeatherSetup, weather, Setup, time_trial, 13)                                                                      \
    X(SwoftyEvWeatherSetup, weather, Setup, seed_forced, 14)                                                                     \
    X(SwoftyEvWeatherSetup, weather, Setup, set_seed, 15)                                                                        \
    X(SwoftyEvWeatherSetup, weather, Setup, seed, 16)                                                                            \
    X(SwoftyEvWeatherSetup, weather, Setup, script_mask, 20)                                                                     \
    X(SwoftyEvWeatherSetup, weather, Setup, counts, 21)                                                                          \
    X(SwoftyEvWeatherSetup, weather, Setup, slots, 32)                                                                           \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, car, 8)                                                                           \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, wheel, 12)                                                                        \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, state, 13)                                                                        \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, previous, 14)                                                                     \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, deflation, 16)                                                                    \
    X(SwoftyEvTyreDamage, tyre_damage, Damage, lap, 20)                                                                          \
    X(SwoftyEvPitStopTyres, sessions, PitStopTyres, car, 8)                                                                      \
    X(SwoftyEvPitStopTyres, sessions, PitStopTyres, kept, 12)

#endif
