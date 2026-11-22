#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <string>

// The addons' rows on the race MFD's pit page (SwoftyCorePitRows). The launcher clones the page's front wing repair
// row (a toggle bound to [repair]) once per row as swofty_pit_<k> (k = 0, 1; settings.hpp numbers them). The pit
// page view model builds the repair row object (Hud_PitRepairRow_Init, 0x118 bytes); right after it the core builds a
// copy of that object per row, with its own VM child swofty_pit_<k> under settings_list (Hud_MfdRow_BaseInit), its own
// data list NO = 0, YES = 1 (Hud_MfdRow_AddOption) and its own pit lane system: a block whose car slots
// (+0x68 + car * 0x10) all point at one car state, +0x04 the pit state the row's enabled bool follows and +0x0c the
// choice. The game's own Hud_PitRepairRow_Sync points the row's current_repair at the choice and subscribes the
// game's change handler, which writes the player's pick into +0x0c; Hud_PitRepairRow_UpdateEnabled greys the row out
// for the pit states 2..4, so the core gives it state 2 while the addon locks it, else the player car's real state.
// The row's handles are released with the object's own destructor before the view model is destroyed
// (Hud_PitSettingsVm_Destroy). Every game call runs on the thread that runs the page; the addon calls touch atomics.
// SWOFTY_PIT_ROWS=0 turns it off.

namespace frontend::pit_rows {

namespace {

namespace guards = swofty_core::guards;

constexpr const char *SET = "swofty_core.dll (pit rows)";
constexpr const char *INIT = "Hud_PitRepairRow_Init";
constexpr const char *ENABLED = "Hud_PitRepairRow_UpdateEnabled";
constexpr const char *SYNC = "Hud_PitRepairRow_Sync";
constexpr const char *DESTROY = "Hud_PitSettingsVm_Destroy";
constexpr const char *BASE_INIT = "Hud_MfdRow_BaseInit";
constexpr const char *ADD_OPTION = "Hud_MfdRow_AddOption";
constexpr const char *LIST_CHILD = "Vm_GetOrCreate_ListChild";
constexpr const char *REGISTRY = "g_Vm_Registry";
constexpr const char *EMPTY = "k_Vm_EmptyNodeHandle";

constexpr size_t ROWS = PIT_ROWS_MAX;
constexpr size_t ROW_BYTES = 0x118;
constexpr uintptr_t ROW_NODE = 0x10, ROW_SYSTEM = 0xb8, ROW_VEHICLE = 0xd0, ROW_STATE_CACHE = 0xd4, ROW_DATA = 0xe0;
constexpr uintptr_t ROW_HANDLES[] = {0x10, 0x30, 0x50, 0x70, 0x90, 0xe0, 0x100};
constexpr uintptr_t VM_REPAIR_ROW = 0x468;
constexpr uintptr_t SYSTEM_CARS = 0x68, CAR_STATE = 0x04, CAR_CHOICE = 0x0c;
constexpr uint32_t CARS = 27, NO_CAR_STATE = 0x10, LOCKED = 2;
constexpr uintptr_t NODE_REFS = 0x24, NODE_HANDLES = 0x28;
constexpr uint32_t DATA_HASH = 0x7c95915f;
constexpr const char *NAMES[ROWS] = {"swofty_pit_0", "swofty_pit_1"};

using Init = void (*)(void *row, void *list, uint8_t vehicle, const char *name);
using Enabled = void (*)(void *row, bool force);
using Sync = void (*)(void *row);
using Destroy = void (*)(void *vm);
using AddOption = void (*)(void *list, const char *id, const char *lng, uint32_t value);
using ListChild = void **(*)(void *registry, void *parent, const uint32_t *hash);
using RowDtor = void *(*)(void *row, uint32_t flags);

Init init_original = nullptr;
Enabled enabled_original = nullptr;
Destroy destroy_original = nullptr;
uintptr_t a_sync = 0, a_base_init = 0, a_add_option = 0, a_list_child = 0, a_registry = 0, a_empty = 0;

struct alignas(16) Row {
    uint8_t object[0x120];
    uint8_t system[SYSTEM_CARS + CARS * 0x10 + 0x10];
    uint8_t car[0x20];
    uintptr_t real;
    bool built;
};

Row rows[ROWS];
std::atomic<int> locked[ROWS];
std::atomic<int> wanted[ROWS] = {-1, -1};
std::atomic<uint32_t> vehicles[ROWS] = {0xff, 0xff};
std::atomic<bool> live{false};

uint32_t load_u32(const void *p) { return __atomic_load_n(reinterpret_cast<const uint32_t *>(p), __ATOMIC_RELAXED); }
void store_u32(void *p, uint32_t v) { __atomic_store_n(reinterpret_cast<uint32_t *>(p), v, __ATOMIC_RELAXED); }

template <class T> T &at(Row &r, uintptr_t off) { return *reinterpret_cast<T *>(r.object + off); }

void *parent_of(uintptr_t node) { return node ? reinterpret_cast<void *>(node + 8) : reinterpret_cast<void *>(a_empty); }

// The game's handle link (Hud_PitRepairRow_Init's data handle): the handle's node field joins the node's handle list
// at its head (+0x28) and the node's count (+0x24) goes up.
void link(uint8_t *field, uintptr_t node)
{
    auto *f = reinterpret_cast<uintptr_t *>(field);
    f[0] = node;
    f[1] = *reinterpret_cast<uintptr_t *>(node + NODE_HANDLES);
    f[2] = 0;
    if (f[1])
        *reinterpret_cast<uintptr_t *>(f[1] + 0x10) = reinterpret_cast<uintptr_t>(field);
    *reinterpret_cast<uintptr_t *>(node + NODE_HANDLES) = reinterpret_cast<uintptr_t>(field);
    _InterlockedIncrement16(reinterpret_cast<volatile short *>(node + NODE_REFS));
}

void release(size_t k)
{
    Row &r = rows[k];
    if (!r.built)
        return;
    r.built = false;
    vehicles[k] = 0xff;
    uintptr_t vt = at<uintptr_t>(r, 0), dtor = 0;
    if (vt && swofty::mem::read(vt, dtor) && swofty::mem::in_image(reinterpret_cast<void *>(dtor)))
        reinterpret_cast<RowDtor>(dtor)(r.object, 0);
    else
        swofty::log("[pit rows] %s: the row's destructor is not in the game image; its handles are left", NAMES[k]);
    r.real = 0;
}

void build(size_t k, uintptr_t real, void *list, uint8_t vehicle)
{
    Row &r = rows[k];
    release(k);
    std::memcpy(r.object, reinterpret_cast<const void *>(real), ROW_BYTES);
    for (uintptr_t h : ROW_HANDLES)
        std::memset(r.object + h, 0, 0x18);
    std::memset(r.system, 0, sizeof r.system);
    for (uint32_t c = 0; c < CARS; c++)
        *reinterpret_cast<uintptr_t *>(r.system + SYSTEM_CARS + c * 0x10) = reinterpret_cast<uintptr_t>(r.car);
    std::memset(r.car, 0, sizeof r.car);
    store_u32(r.car + CAR_STATE, locked[k].load() ? LOCKED : 0);
    at<uintptr_t>(r, ROW_SYSTEM) = reinterpret_cast<uintptr_t>(r.system);
    at<uint32_t>(r, ROW_STATE_CACHE) = 0xffffffffu;
    reinterpret_cast<Init>(a_base_init)(r.object, list, vehicle, NAMES[k]);
    at<uint8_t>(r, ROW_VEHICLE) = vehicle;
    r.built = true;
    r.real = real;
    void *registry = nullptr;
    swofty::mem::read(a_registry, registry);
    uint32_t hash = DATA_HASH;
    void **child = registry ? reinterpret_cast<ListChild>(a_list_child)(registry, parent_of(at<uintptr_t>(r, ROW_NODE)), &hash)
                            : nullptr;
    uintptr_t data = child ? reinterpret_cast<uintptr_t>(*child) : 0;
    if (!data) {
        swofty::log("[pit rows] %s: its data list did not build; the row is left out", NAMES[k]);
        release(k);
        return;
    }
    uintptr_t handle[4] = {0, data, 0, 0};
    reinterpret_cast<AddOption>(a_add_option)(handle, "SwoftyPitNo", "lng_no", 0);
    reinterpret_cast<AddOption>(a_add_option)(handle, "SwoftyPitYes", "lng_yes", 1);
    link(r.object + ROW_DATA, data);
    reinterpret_cast<Sync>(a_sync)(r.object);
    enabled_original(r.object, true);
    vehicles[k] = vehicle;
    swofty::log("[pit rows] %s built for %s beside the repair row of vehicle %u", NAMES[k], settings_pit_row_name(k).c_str(),
                unsigned(vehicle));
}

void init(void *row, void *list, uint8_t vehicle, const char *name)
{
    init_original(row, list, vehicle, name);
    size_t n = settings_pit_rows();
    for (size_t k = 0; k < ROWS && k < n; k++)
        build(k, reinterpret_cast<uintptr_t>(row), list, vehicle);
}

uint32_t car_state(uintptr_t real, uint8_t vehicle)
{
    uintptr_t system = 0, car = 0;
    uint32_t state = NO_CAR_STATE;
    if (vehicle < CARS && swofty::mem::read(real + ROW_SYSTEM, system) && system &&
        swofty::mem::read(system + SYSTEM_CARS + uintptr_t(vehicle) * 0x10, car) && car)
        swofty::mem::read(car + CAR_STATE, state);
    return state;
}

void update_enabled(void *row, bool force)
{
    enabled_original(row, force);
    uintptr_t real = reinterpret_cast<uintptr_t>(row);
    for (size_t k = 0; k < ROWS; k++) {
        Row &r = rows[k];
        if (!r.built || r.real != real)
            continue;
        uint8_t vehicle = 0xff;
        swofty::mem::read(real + ROW_VEHICLE, vehicle);
        at<uint8_t>(r, ROW_VEHICLE) = vehicle;
        vehicles[k].store(vehicle, std::memory_order_relaxed);
        bool lock = locked[k].load(std::memory_order_relaxed);
        store_u32(r.car + CAR_STATE, lock ? LOCKED : car_state(real, vehicle));
        bool dirty = false;
        int want = wanted[k].exchange(-1);
        if (want >= 0 && uint32_t(want) != load_u32(r.car + CAR_CHOICE)) {
            store_u32(r.car + CAR_CHOICE, uint32_t(want));
            dirty = true;
        }
        if (lock && load_u32(r.car + CAR_CHOICE) != 0) {
            store_u32(r.car + CAR_CHOICE, 0);
            dirty = true;
        }
        if (dirty || force)
            reinterpret_cast<Sync>(a_sync)(r.object);
        enabled_original(r.object, force || dirty);
    }
}

void destroy(void *vm)
{
    uintptr_t real = reinterpret_cast<uintptr_t>(vm) + VM_REPAIR_ROW;
    for (size_t k = 0; k < ROWS; k++)
        if (rows[k].built && rows[k].real == real)
            release(k);
    destroy_original(vm);
}

class RowsGuard final : public guards::SiteGuard {
public:
    using SiteGuard::SiteGuard;

protected:
    bool admit() override
    {
        if (settings_pit_rows())
            return true;
        swofty::log("[pit rows] off: no enabled addon declares a pit row");
        return false;
    }
};

void armed() { live = true; }

RowsGuard guard{guards::CoreSetId::pit_rows,
                {
                    .set = SET,
                    .name = "[pit rows]",
                    .env = "SWOFTY_PIT_ROWS",
                    .keeps = {{SYNC, &a_sync},
                              {BASE_INIT, &a_base_init},
                              {ADD_OPTION, &a_add_option},
                              {LIST_CHILD, &a_list_child},
                              {REGISTRY, &a_registry},
                              {EMPTY, &a_empty}},
                    .needs = {SYNC, BASE_INIT, ADD_OPTION, LIST_CHILD, REGISTRY, EMPTY, INIT, ENABLED, DESTROY},
                    .mixins = {{INIT, SWOFTY_MIXIN_WRAP, 0, (void *)&init, (void **)&init_original},
                               {ENABLED, SWOFTY_MIXIN_WRAP, 0, (void *)&update_enabled, (void **)&enabled_original},
                               {DESTROY, SWOFTY_MIXIN_WRAP, 0, (void *)&destroy, (void **)&destroy_original}},
                    .armed = "the addons' NO / YES rows on the MFD pit page follow the game's repair row",
                    .on_armed = armed,
                }};

int find(const char *id, uint32_t *handle)
{
    if (!id || !handle)
        return SWOFTY_ASSET_INVALID;
    std::string dll;
    std::string addon = settings_addon(__builtin_return_address(0), dll);
    int k = addon.empty() ? -1 : settings_pit_row(addon, id);
    if (k < 0 || !live.load())
        return SWOFTY_ASSET_UNAVAILABLE;
    *handle = uint32_t(k) + 1;
    return SWOFTY_ASSET_OK;
}

bool slot(uint32_t handle, size_t &k)
{
    k = size_t(handle) - 1;
    return handle >= 1 && k < ROWS && k < settings_pit_rows();
}

int value(uint32_t handle, uint32_t *out)
{
    size_t k = 0;
    if (!out || !slot(handle, k))
        return SWOFTY_ASSET_INVALID;
    int want = wanted[k].load();
    uint32_t v = want >= 0 ? uint32_t(want) : load_u32(rows[k].car + CAR_CHOICE);
    *out = locked[k].load() ? 0 : v == 1 ? 1 : 0;
    return SWOFTY_ASSET_OK;
}

int set_value(uint32_t handle, uint32_t v)
{
    size_t k = 0;
    if (v > 1 || !slot(handle, k))
        return SWOFTY_ASSET_INVALID;
    wanted[k] = int(v);
    return SWOFTY_ASSET_OK;
}

int set_locked(uint32_t handle, int on)
{
    size_t k = 0;
    if (!slot(handle, k))
        return SWOFTY_ASSET_INVALID;
    if (locked[k].exchange(on ? 1 : 0) != (on ? 1 : 0))
        swofty::log("[pit rows] %s %s", NAMES[k], on ? "locked on NO" : "unlocked");
    return SWOFTY_ASSET_OK;
}

int vehicle(uint32_t handle, uint32_t *out)
{
    size_t k = 0;
    if (!out || !slot(handle, k))
        return SWOFTY_ASSET_INVALID;
    uint32_t v = vehicles[k].load(std::memory_order_relaxed);
    if (v >= CARS)
        return SWOFTY_ASSET_LATE;
    *out = v;
    return SWOFTY_ASSET_OK;
}

const SwoftyPitRowOps table = {SWOFTY_PIT_ROWS_VERSION, find, value, set_value, set_locked, vehicle};

}

}

extern "C" __declspec(dllexport) const SwoftyPitRowOps *SwoftyCorePitRows(void) { return &frontend::pit_rows::table; }
