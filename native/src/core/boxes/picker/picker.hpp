#pragma once

#include "core/boxes/picker/log_caps.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/log_changed.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <array>
#include <atomic>
#include <cstddef>
#include <cstring>
#include <map>
#include <mutex>
#include <string>
#include <vector>

#include "core/boxes/era_logic.hpp"

namespace era_picker {

namespace mem = swofty::mem;
namespace logic = runtime::era;
using swofty::game::FeTile_Add_EraTile_t;
using swofty::game::FeTile_Clear_EraTileSet_Thunk_t;
using swofty::game::FeTile_Is_EraScenarioAvailable_Thunk_t;
using swofty::game::FeTile_Press_EraTile_t;
using swofty::game::Game_F1WorldActivityContainer_SelectEntrantList_t;
using swofty::game::Vm_GetOrCreate_ListChild_t;
using swofty::game::Vm_Set_Bool_t;
using swofty::game::Vm_Set_NodeRef_t;

constexpr const char *MIXIN = "swofty_core.dll (era picker)";
constexpr const char *SPLIT_OPTION = "split_season_picker";

constexpr size_t MODE_NAME = 0x278;
constexpr size_t VM_ROOT = 0x10;
constexpr size_t VM_ALLOCATOR = 0x270;
constexpr size_t VM_MODE = 0x670;
constexpr size_t VM_TILES = 0x690;
constexpr size_t VM_TILES_NODE = 0x698;
constexpr size_t VM_SELECTED_NODE = 0x6b8;
constexpr size_t VM_TILE_BEGIN = 0x6d8;
constexpr size_t VM_TILE_END = 0x6e0;
constexpr size_t TILE_ENTRY = 0x10;

constexpr size_t CONTAINER_MODE = 0x2f0;
constexpr size_t MODE_SETUP = 0x88;
constexpr size_t SETUP_MODES = 0x260;
constexpr size_t SETUP_MODES_VFUNC = 0x1d0;
constexpr size_t SELECTED_LIST = 0xe18;
constexpr size_t SETUP_ENTRANT_SETUP = 0xe10;
constexpr size_t SETUP_GAME_SETUP_MODE = 0x2b0;
using logic::ENTRANT_SETUP_MAX_ENTRANTS;
constexpr size_t SETUP_MODE_NAME = 0xd8;
constexpr size_t ENTRANT_SETUPS = 0x10;
constexpr size_t ENTRANT_SETUP_COUNT = 0x70;
constexpr size_t LISTS = logic::ESM_LISTS;
constexpr size_t LIST_COUNT = logic::ESM_LIST_COUNT;
constexpr size_t LIST_SLOTS = logic::ESM_LIST_SLOTS;
constexpr size_t LIST_NAME = 0x28;
constexpr int64_t ENTRANT_SETUP_SLOTS = 12;

using logic::Handle;

struct Set {
    void *vt0;
    Handle list;
    void *vt1;
    Handle selected;
    void *allocator;
    uint8_t *begin, *end, *cap;
};
static_assert(sizeof(Set) == 0x60);
static_assert(offsetof(Set, list) == VM_TILES_NODE - VM_TILES && offsetof(Set, selected) == VM_SELECTED_NODE - VM_TILES &&
              offsetof(Set, begin) == VM_TILE_BEGIN - VM_TILES);

struct Bound {
    uintptr_t site, press, select, add, available, list_child, node_ref, set_bool, clear, registry;
};

extern Bound b;

struct Built {
    uint8_t *vm = nullptr;
    size_t stock = 0, boxes = 0;
    std::vector<std::string> lists;
};

using Narrowed = logic::ListCut;

struct Picker {
    std::mutex lock;
    std::vector<logic::Box> boxes;
    std::vector<std::string> skipped;
    runtime::loader::MixinGate gate{};
    std::atomic<bool> live{}, off{};
    std::string off_why;
    Set set{};
    Built built;
    Narrowed narrowed[16];
    size_t narrowed_count = 0;
    std::string last_pick, last_press;
    FeTile_Press_EraTile_t *press_orig = nullptr;
    Game_F1WorldActivityContainer_SelectEntrantList_t *select_orig = nullptr;
    PickerLines lines;
};

extern Picker picker;

struct BoxRules {
    logic::DrsSwaps drs;
    runtime::LogChanged drs_log{"[drs] %s"};
    logic::WarmerSwaps warmers;
    runtime::LogChanged warmers_log{"[tyres] %s"};
    logic::EngineSwaps rev_lights;
    runtime::LogChanged rev_lights_log{"[rev-lights] %s"};
    logic::PointsSwaps points;
    runtime::LogChanged points_log{"[points] %s"};
    logic::TyreChangeSwaps tyre_change;
    runtime::LogChanged tyre_change_log{"[tyres] %s"};
    logic::TimeLimitSwaps time_limits;
    logic::WeekendSwaps weekends;
    runtime::LogChanged weekend_log{"[weekend] %s"};
    runtime::LogChanged box_options_log{"[box-rules] %s"};
    runtime::LogChanged grid_log{"[grid] %s"};
    runtime::LogChanged career_log{"[career-rules] %s"};
    // The setup mode the swaps above were last made in: swofty_f1 for a box press, the career's own for its season.
    std::string host{logic::HOST};
};

extern BoxRules box_rules;

// The game mode's game setup modes: the vector its setup owner's virtual at +0x1d0 returns. False when the mode has
// no setup owner or the owner returns no vector.
struct SetupModes {
    uint8_t **begin = nullptr, **end = nullptr;
    bool found = false;

    explicit operator bool() const { return found; }
};

constexpr const char *VIRTUAL_HOST = "swofty_virtual_list";

constexpr size_t LIST_VECTOR = 0x08, LIST_VECTOR_SIZE = 0x20;

using ParseList_t = void(int *ordinal, const void *row_key, void *list, void *driver_registry);
struct VirtualLists {
    ParseList_t *parse_list_orig = nullptr;
    std::atomic<void *> boot_registry{};
    std::atomic<bool> parse_list_bound{};
    std::map<std::string, std::array<uint8_t, LIST_VECTOR_SIZE>> parsed_lists;
    Cap<LOG_FIRST> virtual_lines;
};

extern VirtualLists virtual_lists;

struct Filled {
    void *host = nullptr;
    std::array<uint8_t, LIST_VECTOR_SIZE> own{};
    std::string row, label;
};

extern Filled filled;

extern logic::GridLimit continue_grid;

struct Hold {
    std::mutex lock;
    logic::TtHold held;
    bool holding = false;
};

extern Hold hold;

using SetupVehicles_t = void(void *module);
struct VehicleGuard {
    SetupVehicles_t *orig = nullptr;
    runtime::loader::MixinGate gate{};
    std::atomic<bool> bound{};
    Cap<LOG_FIRST> lines;
};

extern VehicleGuard vehicle_guard;

// Holds a Time Trial session's entrants to the vehicle table before the game builds its participants (time_trial.cpp).
bool install_entrant_cap();

struct TimeTrialGuard {
    uint32_t trace_lines = 0;
    std::atomic<int64_t> budget{-1};
    std::vector<std::string> kept_out;
    Cap<LOG_FIRST * 4> kept_lines;
};

extern TimeTrialGuard time_trial_guard;

struct AliasGuard {
    int (*orig)(void *, void *) = nullptr;
    runtime::loader::MixinGate gate{};
    std::atomic<bool> bound{};
};

extern AliasGuard alias_guard;

struct Unlisted {
    std::mutex lock;
    std::string list;
};

extern Unlisted unlisted;

using ContentAvailability_t = void(void *self, void *availability);
struct AvailabilityGuard {
    ContentAvailability_t *orig = nullptr;
    std::atomic<bool> bound{}, logged_dropped{};
};

extern AvailabilityGuard availability_guard;

using CollectSetupModes_t = void *(void *out, void *allocator);
struct SetupListGuard {
    CollectSetupModes_t *orig = nullptr;
    std::atomic<bool> bound{};
    Cap<LOG_FIRST> swap_lines;
};

extern SetupListGuard setup_list_guard;

const char *name_of(const void *object, size_t off);
uint8_t *setup_owner(void *mode);
SetupModes setup_modes(void *mode);
uint8_t *find_helper_mode(void *mode, const std::string &want, std::string &where);
bool time_trial(void *mode);
int64_t cars_of(void *mode, const std::string &list);
logic::ListCars list_cars(const void *list);
std::vector<logic::ListCars> lists_of(const uint8_t *entrant_setup, std::vector<void *> &ptrs);

// The setup modes a box's rules go into: every swofty_f1 mode for a box press (host_target), the career's current game
// setup mode for a career season (career_rules.cpp). host names them in the lines, prefix leads each line, and weekend
// false keeps the modes' own weekends.
struct RuleTarget {
    std::vector<uint8_t *> modes;
    std::string host, prefix;
    bool weekend = true;
};

RuleTarget host_target(void *mode);
void apply_drs(const RuleTarget &t, const char *list);
void apply_warmers(const RuleTarget &t, const char *list);
void apply_rev_lights(void *mode, const char *list);
void apply_fuel(const RuleTarget &t, const char *list);
void clear_fuel();
void apply_weekend(const RuleTarget &t, void *mode, const char *list);
void apply_box_options(const char *list);
void apply_grid(const RuleTarget &t, const char *list);
void apply_quali_tyres(const RuleTarget &t, const char *list);
void apply_points(const RuleTarget &t, const char *list);
void apply_tyre_change(const RuleTarget &t, const char *list);
void put_back_drs();

// Career season rules (career_rules.cpp). career_session reads, at a session load, whether the session is a career's and
// whether the rules applied at its last load still hold (the same career, season, box and setup mode); apply_career_rules
// then puts the season's box rules into the career's setup mode. put_back_career_rules undoes them when the career
// unloads; forget_career_rules drops them with the setup modes.
struct CareerSession {
    bool career = false, kept = false;
};
CareerSession career_session(uintptr_t ctx);
std::string mode_label(uintptr_t ctx);
void apply_career_rules();
void put_back_career_rules(const char *why);
void forget_career_rules();
void put_back_failed(const Narrowed &n);

void parse_list_entry(int *ordinal, const void *row_key, void *list, void *registry);
std::string pressed_list(const char *label);
void restore_lists();
void restore();
void narrow(void *mode, const char *list);
logic::CutsPutBack put_back(bool cuts = false);

void session_trace(const std::string &where, int64_t vehicles);
void vehicles_entry(void *module);
bool keep_time_trial_lists(void *mode);
bool box_list(const std::string &name);
void time_trial_budget(void *mode, uintptr_t setup);

bool is_ours(const char *list);
void *registry();
void site_entry(SwoftyContext *c);
char *press_entry(void *vm, void *command);
void select_entry(void *container, const char *entrant_list);

int entrant_vector_entry(void *vector, void *bag);
void availability_entry(void *self, void *availability);
void *setup_list_entry(void *out, void *allocator);

std::string text(const char *s);

template <class T> T at(const void *base, size_t off)
{
    return *reinterpret_cast<const T *>(static_cast<const uint8_t *>(base) + off);
}

template <class T> T &ref(void *base, size_t off) { return *reinterpret_cast<T *>(static_cast<uint8_t *>(base) + off); }

template <class F> void for_each_host_setup(void *mode, F f)
{
    SetupModes modes = setup_modes(mode);
    if (!modes)
        return;
    for (auto **p = modes.begin; p != modes.end; ++p) {
        const char *name = name_of(*p, SETUP_MODE_NAME);
        if (!name || strcmp(name, logic::HOST))
            continue;
        int64_t n = at<int64_t>(*p, ENTRANT_SETUP_COUNT);
        auto **setups = reinterpret_cast<uint8_t **>(*p + ENTRANT_SETUPS);
        for (int64_t i = 0; i < n && i < ENTRANT_SETUP_SLOTS; ++i)
            f(setups[i]);
    }
}

}
