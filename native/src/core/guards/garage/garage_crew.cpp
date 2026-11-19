#include "core/guards/garage/cutscene_team_logic.hpp"
#include "core/guards/garage/garage_crew_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <utility>
#include <vector>

// Garage crew guard (re/db/globals.tsv site_Game_GarageCrew_SpawnVector).
//
// Game_GarageCrewModule_SpawnGarageCrew collects the session's assignment records for one garage into a vector of
// 16-byte entries {record, slot}, then creates one crew instance per entry and stores it at [rsp + index*8 + 0x290]
// with no bound. The array holds the two cars a garage has; the slot after it holds the vector's own pointer, which
// the epilogue frees through GAME_DEFAULT. A team that fields a third car puts a third record (slot 2) in its garage,
// the third crew pointer overwrites the vector pointer, and the free fails with "Invalid address for this heap" as the
// session loads. Sess_Set_GarageSlotEntrant already ignores slot >= 2, so only the crew spawn trips over it.
//
// At the site (rcx = vector begin, r13 = vector end, ebp = garage id) the guard keeps the first two entries whose slot
// is below 2 and lowers r13 over the rest. The extra car still races; its garage crew is not spawned.
//
// The crew instance of a garage is built from the team's crew template, found through the session's team tables
// ([[module + 0x48] + 0x2c8 + kind * 0x10], row team * 0x10 + 8, the record's kind at +0x58 and team at +0x5a); the
// function reads [template + 0x560] with no null test, so a car whose team has no template faults the session load
// at SpawnGarageCrew+0xce0. The guard drops those entries from the vector as well; the module is at [rsp + 0xc0].
//
// The pit stops module sets its crews up from fn 0x1403c69e0, which walks the 32 garages and, for each, scans the
// garage's entry vector ([rsp + 0xa8]) for the first entry with a team crew template (the same chain as above); rdi is
// that template or 0. It is passed to Game_PitStops_SetupCrew as the fifth argument, which reads [template + 0x560]
// at +0x12fb with no null test. A garage whose entries have no template (an empty garage, or a team the session has
// no crew template for) faults the session load there ("reading 0x560"). At site_Game_PitStops_CrewTemplateRead the
// guard sends such a garage to site_Game_PitStops_CrewTemplateSkip, the game's own test r12,r12 ahead of the free of
// the entry vector (an empty garage has no vector and takes the game's je), and the loop moves on to the next garage;
// the garage gets no pit stops crew and its cars still race. The loop carries r13 (the address of the key at
// [rsp + 0xc0], which the skipped body would have set back at 0x1403c7745) and sil (the byte at [rsp + 0x4b], reloaded
// at 0x1403c77e0) into the next garage, so the guard restores both along with r12 = [rsp + 0xa8].
//
// Game_GarageCrewModule_SpawnGarageCrew keeps the garage crew scene it looked up at [self + 0xc8]. A track without
// the garage scenes (an imported track: findScene fails for garagecrew, garagecrew_vehicle, _vehicleLocater,
// _ui_equipment and garagegantry) leaves it null. The code after the lookup handles null, but the compiler hoisted
// movzx esi, [rdi + 0xe0] above the test at 0x14038ba27, so the null scene faults at +0x2d30. At
// site_Game_GarageCrew_SceneRead the guard sets esi = 0 and continues at site_Game_GarageCrew_SceneTest, which takes
// the game's own null path.
//
// The garage cutscene states (driver getting ready, crew working on the car, driver in the car and the others in
// CUTSCENES below) read the team's record from the key they hold at [this + 0x31a]: after the 0x3fff0000 test they take
// the record at [[[session + 0x290] + set * 0x10 + 0x2c8] + 8][team * 0x10 + 8] and read its label at +0x18 with no
// null test. A team the game has not loaded a team record for (a 1990 team in a career: its rows are not available
// until a list names them) makes the cutscene's Initialise fault reading 0x18 (Game_DriverGettingReady01_Initialise
// +0xe60, 1.10 dev). At each of those reads the guard looks at rax, the record: when it is null the cutscene takes the
// game's own no-team path, the je of the 0x3fff0000 test found in the bytes before the site, as if the key held the
// sentinel. The cutscene then shows no team; the rest of the garage is untouched.
//
// SWOFTY_GARAGE_CREW_GUARD=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace gc = runtime::garage_crew;
namespace guards = swofty_core::guards;

constexpr const char *MIXIN = "swofty_core.dll (garage crew guard)";
constexpr const char *SITE = "site_Game_GarageCrew_SpawnVector";
constexpr size_t MAX_ENTRIES = 256;
constexpr size_t REC_DRIVER = 0x52;
constexpr size_t REC_CAR_NUMBER = 0x83;
constexpr const char *SETUP_SITE = "site_Game_PitStops_CrewTemplateRead";
constexpr const char *SETUP_SKIP = "site_Game_PitStops_CrewTemplateSkip";
constexpr const char *SCENE_SITE = "site_Game_GarageCrew_SceneRead";
constexpr const char *SCENE_TEST = "site_Game_GarageCrew_SceneTest";
constexpr int LOG_FIRST = 8;
constexpr uint64_t MODULE_VEHICLE_SCENE = 0xd0, MODULE_EQUIPMENT_SCENE = 0xe0;

guards::LogFirst<LOG_FIRST> dropped{"drops"};
std::vector<uint32_t> seen;
uintptr_t setup_skip = 0;
gc::SetupSkips setup_seen;
uintptr_t scene_test = 0;
guards::LogFirst<3> scene_skips{"ones"};

struct Log {
    uint32_t garage;
};

void log_dropped(void *ctx, uint64_t record, uint32_t slot, gc::Drop why)
{
    uint32_t garage = static_cast<Log *>(ctx)->garage;
    uint16_t driver = 0xffff, team = 0xffff;
    uint8_t number = 0;
    if (record) {
        mem::read(record + REC_DRIVER, driver);
        mem::read(record + gc::REC_TEAM, team);
        mem::read(record + REC_CAR_NUMBER, number);
    }
    uint32_t key = uint32_t(driver) << 16 | team;
    for (uint32_t k : seen)
        if (k == key)
            return;
    seen.push_back(key);
    auto line = dropped.note();
    if (!line)
        return;
    const char *tail = line.tail;
    if (why == gc::Drop::no_template)
        swofty::log("[garage] garage %u: team %u car #%u (driver %u) has no crew template in the session; its crew is "
                    "not spawned, the car still races%s",
                    unsigned(garage), unsigned(team), unsigned(number), unsigned(driver), tail);
    else
        swofty::log("[garage] garage %u: team %u car #%u (driver %u) is the %s car of its garage; its crew is not "
                    "spawned, the car still races%s",
                    unsigned(garage), unsigned(team), unsigned(number), unsigned(driver), slot == 2 ? "third" : "extra", tail);
}

void spawn_vector(SwoftyContext *c)
{
    uintptr_t begin = c->rcx, end = c->r13;
    if (!begin || end <= begin || (end - begin) % gc::ENTRY_SIZE)
        return;
    size_t n = (end - begin) / gc::ENTRY_SIZE;
    if (n > MAX_ENTRIES)
        return;
    uint64_t session = 0;
    bool known = gc::session_of(c->rsp, guards::read_if_readable, nullptr, &session);
    Log log{uint32_t(c->rbp)};
    size_t kept =
        gc::compact(reinterpret_cast<uint8_t *>(begin), n, session, known, guards::read_if_readable, nullptr, log_dropped, &log);
    if (kept != n)
        c->r13 = begin + kept * gc::ENTRY_SIZE;
}

void setup_template(SwoftyContext *c)
{
    if (!gc::setup_template_missing(c->rdi))
        return;
    uint64_t begin = 0;
    uint8_t flag = 0;
    if (!guards::read_if_readable(c->rsp + gc::SETUP_VECTOR_BEGIN, &begin, sizeof begin, nullptr) ||
        !guards::read_if_readable(c->rsp + gc::SETUP_FLAG, &flag, sizeof flag, nullptr))
        return;
    if (setup_seen.first(c->r14)) {
        uint64_t record = 0;
        uint16_t team = 0xffff;
        uint8_t number = 0;
        if (gc::user_pointer(begin) && guards::read_if_readable(begin, &record, sizeof record, nullptr) &&
            gc::user_pointer(record)) {
            mem::read(record + gc::REC_TEAM, team);
            mem::read(record + REC_CAR_NUMBER, number);
        }
        if (team == 0xffff)
            swofty::log("[garage] garage %u has no cars with a crew template; its pit stops crew is not set up",
                        unsigned(c->r14));
        else
            swofty::log("[garage] garage %u: team %u car #%u has no pit stops crew template; its pit stops crew is not "
                        "set up, the car still races",
                        unsigned(c->r14), unsigned(team), unsigned(number));
    }
    c->r12 = begin;
    c->r13 = c->rsp + gc::SETUP_KEY;
    c->rsi = flag;
    c->rip = setup_skip;
}

void scene_read(SwoftyContext *c)
{
    if (!gc::scene_missing(c->rdi))
        return;
    if (auto line = scene_skips.note()) {
        uint64_t vehicle = 0, equipment = 0;
        mem::read(c->rbp + MODULE_VEHICLE_SCENE, vehicle);
        mem::read(c->rbp + MODULE_EQUIPMENT_SCENE, equipment);
        swofty::log("[garage] the garage crew scene is missing for this track (garage crew scene 0x%llx, vehicle scene "
                    "0x%llx, equipment scene 0x%llx); no garage crew is shown%s",
                    (unsigned long long)c->rdi, (unsigned long long)vehicle, (unsigned long long)equipment, line.tail);
    }
    c->rsi = 0;
    c->rip = scene_test;
}

constexpr const char *CUTSCENES[][2] = {
    {"site_Game_CrewWorkingOnCar_TeamRecord", "CrewWorkingOnCar"},
    {"site_Game_SwapTyres_TeamRecord", "SwapTyres"},
    {"site_Game_DriverSatInCar06_TeamRecord", "DriverSatInCar06"},
    {"site_Game_GettingReadyToRace_TeamRecordA", "GettingReadyToRace"},
    {"site_Game_GettingReadyToRace_TeamRecordB", "GettingReadyToRace"},
    {"site_Game_GettingReadyToRaceIdle01_TeamRecord", "GettingReadyToRaceIdle01"},
    {"site_Game_DriverInCarIdle03_TeamRecordA", "DriverInCarIdle03"},
    {"site_Game_DriverInCarIdle03_TeamRecordB", "DriverInCarIdle03"},
    {"site_Game_DriverInCarIdle02_TeamRecord", "DriverInCarIdle02"},
    {"site_Game_DriverInCarIdle04_TeamRecord", "DriverInCarIdle04"},
    {"site_Game_DriverInCarIdle05_TeamRecordA", "DriverInCarIdle05"},
    {"site_Game_DriverInCarIdle05_TeamRecordB", "DriverInCarIdle05"},
    {"site_Game_DriverGettingReady01_TeamRecord", "DriverGettingReady01"},
    {"site_Game_DriverGettingWeighed01_TeamRecordA", "DriverGettingWeighed01"},
    {"site_Game_DriverGettingWeighed01_TeamRecordB", "DriverGettingWeighed01"},
    {"site_Game_DriverTalkToEngineer01_TeamRecordA", "DriverTalkToEngineer01"},
    {"site_Game_DriverTalkToEngineer01_TeamRecordB", "DriverTalkToEngineer01"},
    {"site_Game_DriverTalksToCrew01_TeamRecord", "DriverTalksToCrew01"},
    {"site_Game_CutsceneAt354530_TeamRecord", "the state at 0x140354530"},
};
constexpr size_t CUTSCENE_COUNT = sizeof CUTSCENES / sizeof CUTSCENES[0];

uintptr_t cutscene_resume[CUTSCENE_COUNT];
std::mutex cutscene_mu;
runtime::cutscene_team::Seen cutscene_seen;

template <size_t N> void cutscene_team(SwoftyContext *c)
{
    if (!runtime::cutscene_team::record_missing(c->rax))
        return;
    uintptr_t target = cutscene_resume[N];
    if (!target)
        return;
    uint32_t team = runtime::cutscene_team::team_of(c->rcx);
    bool say;
    {
        std::lock_guard g(cutscene_mu);
        say = cutscene_seen.first(uint32_t(N), team);
    }
    if (say)
        swofty::log("[garage] cutscene %s: team %u has no team record; showing no team", CUTSCENES[N][1], unsigned(team));
    c->rip = target;
}

const SwoftyMixin CUTSCENE_MIXINS[CUTSCENE_COUNT] = {
    {"site_Game_CrewWorkingOnCar_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<0>, nullptr},
    {"site_Game_SwapTyres_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<1>, nullptr},
    {"site_Game_DriverSatInCar06_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<2>, nullptr},
    {"site_Game_GettingReadyToRace_TeamRecordA", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<3>, nullptr},
    {"site_Game_GettingReadyToRace_TeamRecordB", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<4>, nullptr},
    {"site_Game_GettingReadyToRaceIdle01_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<5>, nullptr},
    {"site_Game_DriverInCarIdle03_TeamRecordA", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<6>, nullptr},
    {"site_Game_DriverInCarIdle03_TeamRecordB", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<7>, nullptr},
    {"site_Game_DriverInCarIdle02_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<8>, nullptr},
    {"site_Game_DriverInCarIdle04_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<9>, nullptr},
    {"site_Game_DriverInCarIdle05_TeamRecordA", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<10>, nullptr},
    {"site_Game_DriverInCarIdle05_TeamRecordB", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<11>, nullptr},
    {"site_Game_DriverGettingReady01_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<12>, nullptr},
    {"site_Game_DriverGettingWeighed01_TeamRecordA", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<13>, nullptr},
    {"site_Game_DriverGettingWeighed01_TeamRecordB", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<14>, nullptr},
    {"site_Game_DriverTalkToEngineer01_TeamRecordA", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<15>, nullptr},
    {"site_Game_DriverTalkToEngineer01_TeamRecordB", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<16>, nullptr},
    {"site_Game_DriverTalksToCrew01_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<17>, nullptr},
    {"site_Game_CutsceneAt354530_TeamRecord", SWOFTY_MIXIN_SITE, 100, (void *)&cutscene_team<18>, nullptr},
};

void arm_cutscenes(runtime::CoreMixins &set)
{
    size_t armed = 0;
    for (size_t n = 0; n < CUTSCENE_COUNT; n++) {
        uintptr_t at = swofty::api->resolve(CUTSCENE_MIXINS[n].target);
        uint8_t code[runtime::cutscene_team::LOOKBACK] = {};
        if (!at || !guards::read_if_readable(at - sizeof code, code, sizeof code, nullptr))
            continue;
        cutscene_resume[n] = runtime::cutscene_team::no_team_target(code, sizeof code, at - sizeof code, at);
        if (!cutscene_resume[n])
            continue;
        set.mixins.push_back(CUTSCENE_MIXINS[n]);
        armed++;
    }
    if (armed != CUTSCENE_COUNT)
        swofty::log("[garage] cutscene team guard: %u of %u cutscene sites resolve with a no-team path on this build; the "
                    "rest keep the game's lookup",
                    unsigned(armed), unsigned(CUTSCENE_COUNT));
}

class CrewGuard final : public guards::SiteGuard {
public:
    CrewGuard()
        : SiteGuard(guards::CoreSetId::garage_crew,
                    {
                        .set = MIXIN,
                        .name = "[garage] crew guard",
                        .env = "SWOFTY_GARAGE_CREW_GUARD",
                        .needs = {SITE},
                        .mixins = {{SITE, SWOFTY_MIXIN_SITE, 100, (void *)&spawn_vector, nullptr}},
                        .armed = "a garage's crew is spawned for its first two cars; a third car is left without one",
                    })
    {
    }

private:
    void extend(runtime::CoreMixins &set) override
    {
        setup_skip = swofty::api->resolve(SETUP_SKIP);
        if (setup_skip && swofty::api->resolve(SETUP_SITE))
            set.mixins.push_back({SETUP_SITE, SWOFTY_MIXIN_SITE, 100, (void *)&setup_template, nullptr});
        else
            swofty::log("[garage] pit stops crew guard off: needs %s and %s, which this build does not resolve", SETUP_SITE,
                        SETUP_SKIP);
        arm_cutscenes(set);
        scene_test = swofty::api->resolve(SCENE_TEST);
        if (scene_test && swofty::api->resolve(SCENE_SITE))
            set.mixins.push_back({SCENE_SITE, SWOFTY_MIXIN_SITE, 100, (void *)&scene_read, nullptr});
        else
            swofty::log("[garage] crew scene guard off: needs %s and %s, which this build does not resolve", SCENE_SITE,
                        SCENE_TEST);
    }
};

CrewGuard guard;

}
