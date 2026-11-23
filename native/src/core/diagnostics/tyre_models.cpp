#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/diagnostics/tyre_models_logic.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <safetyhook.hpp>

#include <atomic>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

// Fitted tyre model check (read-only diagnostic; every offset is L1, read live on F1 1990 2026-11-03).
//
// Sess_TyreAllocation_OnSessionStart's component is kept (+0x270 session provider, +0x278 career with the entrants,
// +0x2d8 car list). TYRE_FIRST_MS after the session starts, and again at TYRE_SECOND_MS, each car's tyre component is
// read: the slot table (+0x9bd0, count +0x9c48, class +0x9bc0) names the entry of the fitted option, and the three
// models of that entry (component +0x30 + entry * 0xde0 + 0 / 0x4a0 / 0x940) are checked for the spawn defaults.
// A car whose spawned fitted models all still have them was never placed (Game_CompoundModelSet_PlaceFittedTyre,
// driven per vehicle by Game_TyreSystem_UpdateVehicle) and drives without visible tyres; known_problems explains it.
// A car found unplaced also gets one line with the activation fields Game_TyreSystem_UpdateVehicle needs before it
// places anything (+0x9920..+0x9948, set by Car_Tyre_ActivateCompounds only while the car's scene state, the row at
// [[SceneContext+0xa0]+0x68+vehicle*0x10]+4, is 2 or 3). Nothing is written and no placement is attempted. SWOFTY_TYRE_MODELS_CHECK=0 turns it off.

namespace {

namespace mem = swofty::mem;
namespace tmod = runtime::tyre_models;

constexpr const char *MIXIN = "swofty_core.dll (tyre model check)";
constexpr const char *SITE = "Sess_TyreAllocation_OnSessionStart";
constexpr int64_t TYRE_FIRST_MS = 10000, TYRE_SECOND_MS = 60000;
constexpr size_t COMP_PROVIDER = 0x270, COMP_CAREER = 0x278, COMP_CAR_LIST = 0x2d8;
constexpr size_t PROVIDER_SESSION_SLOT = 0xa8 / 8, SESSION_ID = 0x08;
constexpr size_t CAREER_ENTRANTS = 0x2e0, ENTRANT_SIZE = 0x88, ENTRANT_MAX = 256;
constexpr size_t ENTRANT_FLAGS = 0x4b, ENTRANT_TEAM = 0x5a, ENTRANT_VEHICLE = 0x82;
constexpr size_t CARS_BEGIN = 0x28, CARS_END = 0x30, CAR_ENTITY = 0x10;
constexpr size_t TC_PROBE = 0x9c68, TC_CLASS = 0x9bc0, TC_SLOTS = 0x9bd0, TC_COUNT = 0x9c48, TC_FITTED = 0x9c98;
constexpr size_t TC_MODEL_COUNT = 0x98d0, TC_MODELS = 0x30, MODEL_ENTRY_STRIDE = 0xde0;
constexpr size_t SLOT_STRIDE = 0x18, SLOT_ROW = 0x10, CLASS_VISUAL_MAP = 0x1d8;
constexpr ptrdiff_t FITTED_OPTION = -0x6c;
constexpr size_t MODEL_SUB[] = {0, 0x4a0, 0x940};
constexpr size_t MODEL_RENDER = 0x78, RENDER_INSTANCE = 0x80, INSTANCE_TRANSLATION = 0x2c, INSTANCE_STATE = 0x128;
constexpr size_t MODEL_DIMENSION_A = 0x278, MODEL_DIMENSION_B = 0x27c;
constexpr uint64_t MAX_ENTRIES = 11;
constexpr size_t TC_ACTIVE_FITTED = 0x9938, TC_ACTIVE_FITTED_A = 0x9940, TC_ACTIVE_FITTED_B = 0x9948;
constexpr size_t TC_ACTIVE_PENDING = 0x9920, TC_ACTIVE_PENDING_A = 0x9928, TC_ACTIVE_PENDING_B = 0x9930;
constexpr size_t TC_PENDING_OPTION = 0x9c70, TC_LOCKED = 0x9912, TC_APPLIED = 0x9913, TC_BUSY = 0x9918, MODEL_GATE = 0x80;
constexpr size_t SCENE_STATES = 0xa0, SCENE_STATE_ROWS = 0x68, SCENE_STATE_STRIDE = 0x10, SCENE_STATE_VALUE = 0x4;
constexpr uint8_t SCENE_STATE_LAST_VEHICLE = 0x1a;
constexpr uintptr_t SCENE_CONTEXT_VA = 0x14601f5c8;
constexpr size_t TC_CAR = 0x20, CAR_LIVE = 0x58, LIVE_VEHICLE_ID = 0xe8;

const runtime::Sig S_UPDATE_VEHICLE{
    "Game_TyreSystem_UpdateVehicle",
    "41 57 41 56 41 54 56 57 55 53 48 81 EC 50 01 00 00 C5 78 29 BC 24 40 01 00 00 C5 78 29 B4 24 30 01 00 00 C5 78 29 AC 24 20 "
    "01 00 00 C5 78 29 A4 24 10 01 00 00 C5 78 29 9C 24 00 01 00 00 C5 78 29 94 24 F0 00 00 00 C5 78 29 8C 24 E0 00 00 00 C5 78 "
    "29 84 24 D0 00 00 00 C5 F8 29 BC 24 C0 00 00 00 C5 F8 29 B4 24 B0 00 00 00 48 89 CF 44 0F B6 FA 48 8B 81 10 01 00 00 48 8B "
    "89 18 01 00 00",
    0x2e93110};
constexpr size_t SYSTEM_LIST = 0x110, SYSTEM_LIST_END = 0x118, SYSTEM_LIST_CAP = 0x120, COUNT_IDS = 32;
std::atomic<uint32_t> update_counts[COUNT_IDS];
std::atomic<uintptr_t> update_system{0};
safetyhook::InlineHook update_hook_handle;
bool update_counting = false;

uint32_t session = 0;
std::atomic<uint32_t> list_extra_total{0}, list_extra_mask{0};
std::atomic<bool> list_fix{true};
std::atomic<uint32_t> seen_max{0}, seen_calls{0}, learned{0};
std::atomic<int> list_logged{0};

void update_extras(void *system, float dt, void *camera, float scale, uint32_t cam)
{
    uintptr_t sys = uintptr_t(system), begin = 0, end = 0;
    if (!mem::read(sys + SYSTEM_LIST, begin) || !mem::read(sys + SYSTEM_LIST_END, end) || !begin || end <= begin)
        return;
    uint64_t count = (end - begin) / sizeof(uintptr_t);
    if (count > tmod::MAX_LIST)
        return;
    bool has_car[tmod::MAX_LIST] = {};
    for (uint64_t i = learned.load(); i < count; i++) {
        uintptr_t entry = 0, car = 0;
        has_car[i] = mem::read(begin + i * sizeof(uintptr_t), entry) && entry && mem::read(entry + TC_CAR, car) && car;
    }
    uint32_t todo[tmod::MAX_LIST];
    size_t n = tmod::extra_updates(count, learned.load(), has_car, todo, tmod::MAX_LIST);
    for (size_t k = 0; k < n; k++) {
        update_hook_handle.call<void>(system, uint8_t(todo[k]), dt, camera, scale, cam);
        uintptr_t entry = 0, car = 0, live = 0;
        uint8_t id = 0xff;
        if (mem::read(begin + todo[k] * sizeof(uintptr_t), entry) && mem::read(entry + TC_CAR, car) &&
            mem::read(car + CAR_LIVE, live) && live && mem::read(live + LIVE_VEHICLE_ID, id) && id < 32)
            list_extra_mask.fetch_or(1u << id, std::memory_order_relaxed);
        list_extra_total.fetch_add(1, std::memory_order_relaxed);
    }
    if (n && list_logged.exchange(1) == 0) {
        std::string ids;
        for (unsigned v = 0; v < 32; v++)
            if (list_extra_mask.load() & (1u << v))
                ids += (ids.empty() ? "" : ", ") + std::to_string(v);
        swofty::log("[tyres] list fix, session %u: the caller updates %u of %llu tyre system entries; %zu more are updated after "
                    "it (vehicle(s) %s)",
                    session, learned.load(), (unsigned long long)count, n, ids.c_str());
    }
}

void update_vehicle_hook(void *system, uint8_t idx, float dt, void *camera, float scale, uint32_t cam)
{
    update_system.store(uintptr_t(system), std::memory_order_relaxed);
    if (update_counting && idx < COUNT_IDS)
        update_counts[idx].fetch_add(1, std::memory_order_relaxed);
    update_hook_handle.call<void>(system, idx, dt, camera, scale, cam);
    if (!list_fix.load(std::memory_order_relaxed))
        return;
    uint32_t top = seen_max.load(std::memory_order_relaxed);
    while (idx > top && !seen_max.compare_exchange_weak(top, idx, std::memory_order_relaxed)) {
    }
    uint32_t calls = seen_calls.fetch_add(1, std::memory_order_relaxed) + 1;
    uint32_t bound = learned.load(std::memory_order_relaxed);
    if (!bound && calls >= tmod::LEARN_CALLS) {
        bound = tmod::learned_bound(seen_max.load(), calls);
        learned.store(bound);
    }
    if (tmod::is_last_vehicle(bound, idx))
        update_extras(system, dt, camera, scale, cam);
}

std::string env_value(const char *name)
{
    char v[32] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n && n < sizeof v ? std::string(v) : std::string();
}

void (*start_orig)(void *) = nullptr;
runtime::loader::MixinGate mixin_gate;
bool wanted = false;
std::mutex lock;
uintptr_t component = 0;
int64_t started_ms = 0;
std::atomic<int> stage{2};

int64_t now_ms()
{
    return std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

bool read_ptr(uintptr_t at, uintptr_t &out) { return mem::read(at, out) && out; }

uintptr_t tyre_component_of(uintptr_t car_list, uint8_t v)
{
    uintptr_t begin = 0, end = 0, slot = 0, obj = 0;
    if (!car_list || !mem::read(car_list + CARS_BEGIN, begin) || !mem::read(car_list + CARS_END, end) || !begin || end <= begin ||
        (end - begin) / sizeof(uintptr_t) <= v || !read_ptr(begin + v * sizeof(uintptr_t), slot) ||
        !read_ptr(slot + CAR_ENTITY, obj))
        return 0;
    auto type = reinterpret_cast<const uint8_t *>(swofty::api->resolve("k_Sess_TyreComponentType"));
    auto base = reinterpret_cast<const uint8_t *>(swofty::api->resolve("k_Ne_TypeRoot"));
    auto chain_end = reinterpret_cast<const uint8_t *>(swofty::api->resolve("k_Ne_TypeChainEnd"));
    auto find = reinterpret_cast<void *(*)(void *, const void *)>(swofty::api->resolve("Ne_Entity_FindComponent"));
    if (!type || !base || !chain_end || !find)
        return 0;
    bool is_component = type == base || type == chain_end;
    for (const uint8_t *n = type + 8; !is_component && n && mem::readable(n + 0x10, 8);) {
        if (!mem::read(n + 0x10, n))
            break;
        is_component = n == base + 8;
        if (n == chain_end + 8)
            break;
    }
    if (is_component)
        obj = uintptr_t(find(reinterpret_cast<void *>(obj), type));
    uint64_t probe = 0;
    return obj && mem::read(obj + TC_PROBE, probe) ? obj : 0;
}

uint32_t fitted_entry(uintptr_t tc)
{
    uint64_t count = 0;
    uintptr_t fitted = 0, cls = 0;
    uint32_t option = 0;
    if (!mem::read(tc + TC_COUNT, count) || !read_ptr(tc + TC_FITTED, fitted) || !read_ptr(tc + TC_CLASS, cls) ||
        !mem::read(fitted + FITTED_OPTION, option))
        return 0xff;
    for (uint64_t i = 0; i < count && i < 8; i++) {
        uintptr_t e = tc + TC_SLOTS + i * SLOT_STRIDE, row = 0, map = 0;
        uint32_t o = 0, visual = 0;
        uint8_t slot = 0xff;
        if (!mem::read(e, o) || o != option)
            continue;
        if (!read_ptr(e + SLOT_ROW, row) || !mem::read(row, visual) || !read_ptr(cls + CLASS_VISUAL_MAP, map) ||
            !mem::read(map + (visual & 0xff), slot))
            return 0xff;
        return slot;
    }
    return 0xff;
}

tmod::Model read_model(uintptr_t m)
{
    tmod::Model out;
    uintptr_t render = 0, instance = 0;
    if (!read_ptr(m + MODEL_RENDER, render) || !read_ptr(render + RENDER_INSTANCE, instance))
        return out;
    out.spawned = mem::read(instance + INSTANCE_TRANSLATION, out.translation) &&
                  mem::read(instance + INSTANCE_STATE, out.state) && mem::read(m + MODEL_DIMENSION_A, out.dimension_a) &&
                  mem::read(m + MODEL_DIMENSION_B, out.dimension_b);
    return out;
}

int64_t scene_state_of(uint8_t vehicle)
{
    uintptr_t ctx = 0, states = 0, row = 0;
    uint32_t value = 0;
    if (vehicle > SCENE_STATE_LAST_VEHICLE)
        return -1;
    if (!read_ptr(SCENE_CONTEXT_VA, ctx))
        return -3;
    if (!read_ptr(ctx + SCENE_STATES, states))
        return -4;
    if (!read_ptr(states + SCENE_STATE_ROWS + vehicle * SCENE_STATE_STRIDE, row))
        return -5;
    return mem::read(row + SCENE_STATE_VALUE, value) ? int64_t(value) : -6;
}

void log_activation(uintptr_t tc, uint8_t vehicle, const char *tag, uint32_t entry)
{
    uint32_t fitted = 0xffffffff, pending = 0xffffffff, option = 0xffffffff;
    uint64_t fitted_a = 0, fitted_b = 0, pending_a = 0, pending_b = 0;
    uint8_t locked = 0xff, applied = 0xff, busy = 0xff, live_id = 0xff, gate[3] = {0xff, 0xff, 0xff};
    uintptr_t car = 0, live = 0;
    mem::read(tc + TC_ACTIVE_FITTED, fitted);
    mem::read(tc + TC_ACTIVE_FITTED_A, fitted_a);
    mem::read(tc + TC_ACTIVE_FITTED_B, fitted_b);
    mem::read(tc + TC_ACTIVE_PENDING, pending);
    mem::read(tc + TC_ACTIVE_PENDING_A, pending_a);
    mem::read(tc + TC_ACTIVE_PENDING_B, pending_b);
    mem::read(tc + TC_PENDING_OPTION, option);
    mem::read(tc + TC_LOCKED, locked);
    mem::read(tc + TC_APPLIED, applied);
    mem::read(tc + TC_BUSY, busy);
    if (mem::read(tc + TC_CAR, car) && car && mem::read(car + CAR_LIVE, live) && live)
        mem::read(live + LIVE_VEHICLE_ID, live_id);
    if (entry < MAX_ENTRIES)
        for (size_t i = 0; i < 3; i++)
            mem::read(tc + TC_MODELS + entry * MODEL_ENTRY_STRIDE + MODEL_GATE + i, gate[i]);
    swofty::log("[tyres] car %u activation (%s): live id %u, fitted compound %u (%s%s), set to apply %u (%s%s), pending option "
                "%u, flags %u/%u/%u, entry %u gate %u/%u/%u, scene state %lld",
                unsigned(vehicle), tag, unsigned(live_id), fitted, fitted_a ? "a" : "-", fitted_b ? "b" : "-", pending,
                pending_a ? "a" : "-", pending_b ? "b" : "-", option, unsigned(locked), unsigned(applied), unsigned(busy),
                unsigned(entry), unsigned(gate[0]), unsigned(gate[1]), unsigned(gate[2]), (long long)scene_state_of(vehicle));
}

tmod::Car check_car(uintptr_t car_list, uintptr_t row)
{
    tmod::Car car;
    uint8_t flags = 0;
    mem::read(row + ENTRANT_VEHICLE, car.vehicle);
    mem::read(row + ENTRANT_FLAGS, flags);
    mem::read(row + ENTRANT_TEAM, car.team);
    car.human = flags & 0x10;
    uintptr_t tc = tyre_component_of(car_list, car.vehicle);
    uint64_t entries = 0;
    if (!tc || !mem::read(tc + TC_MODEL_COUNT, entries))
        return car;
    uint32_t entry = fitted_entry(tc);
    if (entry >= entries || entry >= MAX_ENTRIES)
        return car;
    tmod::Model models[3];
    for (size_t i = 0; i < 3; i++)
        models[i] = read_model(tc + TC_MODELS + entry * MODEL_ENTRY_STRIDE + MODEL_SUB[i]);
    car.readable = true;
    car.verdict = tmod::judge(models, 3, &car.spawned, &car.unplaced);
    if (car.verdict == tmod::Verdict::unplaced)
        log_activation(tc, car.vehicle, "unplaced", entry);
    else if (car.vehicle == 0 || car.vehicle == tmod::PROBE_VEHICLE)
        log_activation(tc, car.vehicle, "placed", entry);
    return car;
}

void log_update_counts(const char *when, uintptr_t car_list)
{
    uintptr_t sys = update_system.load(), begin = 0, end = 0, cap = 0;
    std::string counts;
    unsigned seen = 0;
    for (size_t i = 0; i < COUNT_IDS; i++) {
        uint32_t n = update_counts[i].load();
        if (!n && i >= 28)
            continue;
        counts += (counts.empty() ? "" : " ") + std::to_string(i) + ":" + std::to_string(n);
        seen += n != 0;
    }
    swofty::log("[tyres] UpdateVehicle calls (%s), session %u: %s (%u vehicle(s) updated)", when, session, counts.c_str(), seen);
    if (!sys || !mem::read(sys + SYSTEM_LIST, begin) || !mem::read(sys + SYSTEM_LIST_END, end) ||
        !mem::read(sys + SYSTEM_LIST_CAP, cap)) {
        swofty::log("[tyres] tyre system list (%s): the system %p could not be read", when, (void *)sys);
        return;
    }
    swofty::log("[tyres] tyre system list (%s): %p, %llu entr(ies), capacity %llu", when, (void *)sys,
                (unsigned long long)((end - begin) / sizeof(uintptr_t)), (unsigned long long)((cap - begin) / sizeof(uintptr_t)));
    for (uint64_t i = 0; begin && end > begin && i < tmod::MAX_LIST && i < (end - begin) / sizeof(uintptr_t); i++) {
        uintptr_t entry = 0, car = 0, live = 0;
        uint8_t id = 0xff;
        mem::read(begin + i * sizeof(uintptr_t), entry);
        if (entry && mem::read(entry + TC_CAR, car) && car && mem::read(car + CAR_LIVE, live) && live)
            mem::read(live + LIVE_VEHICLE_ID, id);
        swofty::log("[tyres] tyre system list (%s): idx %llu entry %p car %p vehicle %u", when, (unsigned long long)i,
                    (void *)entry, (void *)car, unsigned(id));
    }
    swofty::log("[tyres] list fix (%s): bound %u, %u extra update(s) so far", when, learned.load(), list_extra_total.load());
    for (uint8_t v = 22; v < 26; v++) {
        uintptr_t entry = 0, comp = tyre_component_of(car_list, v), parent = 0;
        bool in = begin && v < (end - begin) / sizeof(uintptr_t) && mem::read(begin + v * sizeof(uintptr_t), entry);
        if (entry)
            mem::read(entry + TC_CAR, parent);
        swofty::log("[tyres] tyre system list (%s): vehicle %u entry %s%p, car list component %p, entry car %p", when,
                    unsigned(v), in ? "" : "(none) ", (void *)entry, (void *)comp, (void *)parent);
    }
}

void check(const char *when)
{
    uintptr_t career = 0, car_list = 0, begin = 0, end = 0;
    if (!read_ptr(component + COMP_CAREER, career) || !read_ptr(component + COMP_CAR_LIST, car_list) ||
        !mem::read(career + CAREER_ENTRANTS, begin) || !mem::read(career + CAREER_ENTRANTS + 8, end) || !begin || end <= begin ||
        (end - begin) % ENTRANT_SIZE || (end - begin) / ENTRANT_SIZE > ENTRANT_MAX || !mem::readable(begin, end - begin)) {
        swofty::log("[tyres] fitted tyre check (%s), session %u: the entrants could not be read", when, session);
        return;
    }
    std::vector<tmod::Car> cars;
    for (uintptr_t row = begin; row < end && cars.size() < tmod::MAX_CARS; row += ENTRANT_SIZE) {
        uint8_t v = 0xff;
        if (!mem::read(row + ENTRANT_VEHICLE, v) || v == 0xff)
            continue;
        cars.push_back(check_car(car_list, row));
        if (cars.back().verdict == tmod::Verdict::unplaced)
            swofty::log("%s", tmod::car_line(cars.back(), session, when).c_str());
    }
    swofty::log("%s", tmod::summary_line(cars, session, when).c_str());
    if (update_counting)
        log_update_counts(when, car_list);
}

void on_start(void *c)
{
    start_orig(c);
    if (!c)
        return;
    uintptr_t provider = 0, vt = 0, getter = 0, s = 0;
    uint32_t id = 0;
    if (read_ptr(uintptr_t(c) + COMP_PROVIDER, provider) && read_ptr(provider, vt) &&
        read_ptr(vt + PROVIDER_SESSION_SLOT * sizeof(uintptr_t), getter)) {
        s = uintptr_t(reinterpret_cast<void *(*)(void *)>(getter)(reinterpret_cast<void *>(provider)));
        if (s)
            mem::read(s + SESSION_ID, id);
    }
    std::lock_guard<std::mutex> g(lock);
    component = uintptr_t(c);
    session = id;
    started_ms = now_ms();
    for (auto &n : update_counts)
        n.store(0);
    list_extra_total = 0;
    list_extra_mask = 0;
    seen_max = 0;
    seen_calls = 0;
    learned = 0;
    list_logged = 0;
    stage = 0;
}

}

namespace runtime {

static std::vector<CoreMixins> tyre_models_mixins()
{
    std::vector<CoreMixins> out;
    if (mixin_gate.offered() || !code_ready())
        return out;
    mixin_gate.offer();
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_TYRE_MODELS_CHECK", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[tyres] model check off (SWOFTY_TYRE_MODELS_CHECK=%s)", v);
        return out;
    }
    if (!swofty::api->resolve(SITE)) {
        swofty::log("[tyres] model check off: needs %s, which this build does not resolve", SITE);
        return out;
    }
    wanted = true;
    out.push_back({MIXIN, {{SITE, SWOFTY_MIXIN_WRAP, 0, (void *)&on_start, (void **)&start_orig}}});
    return out;
}

static void tyre_models_commit()
{
    if (!wanted || !mixin_gate.settle())
        return;
    swofty::log(
        "[tyres] model check %s",
        loader::install_state(core_mixins_active(MIXIN), "armed: fitted tyre models are read 10 s and 60 s into each session"));
    bool count_on = !tmod::switched_off(env_value("SWOFTY_TYRE_UPDATE_COUNT").c_str());
    list_fix = !tmod::switched_off(env_value("SWOFTY_TYRE_LIST_FIX").c_str());
    update_counting = false;
    if (count_on || list_fix) {
        uintptr_t at = runtime::find(S_UPDATE_VEHICLE, "tyre system update", "tyres");
        void *target = reinterpret_cast<void *>(at), *detour = reinterpret_cast<void *>(&update_vehicle_hook);
        if (!at) {
            swofty::log("[tyres] update hook off: Game_TyreSystem_UpdateVehicle did not resolve on this build");
        } else if (auto r = safetyhook::InlineHook::create(target, detour)) {
            update_hook_handle = std::move(*r);
            update_counting = count_on;
            swofty::log("[tyres] update hook armed: counts %s, list fix %s", count_on ? "on" : "off (SWOFTY_TYRE_UPDATE_COUNT=0)",
                        list_fix ? "on (cars past the caller's vehicle count are updated too)" : "off (SWOFTY_TYRE_LIST_FIX=0)");
        } else {
            swofty::log("[tyres] update hook off: the hook on Game_TyreSystem_UpdateVehicle failed");
        }
    }
}

static loader::FunctionSet tyre_models_core_set{loader::CoreSetId::tyre_models, tyre_models_mixins, tyre_models_commit};

void tyre_models_tick()
{
    if (stage >= 2)
        return;
    std::lock_guard<std::mutex> g(lock);
    if (!component)
        return;
    if (stage >= 2)
        return;
    int64_t due = stage == 0 ? TYRE_FIRST_MS : TYRE_SECOND_MS;
    if (now_ms() - started_ms < due)
        return;
    const char *when = stage == 0 ? "10 s in" : "60 s in";
    stage++;
    check(when);
}

void tyre_models_drop()
{
    std::lock_guard<std::mutex> g(lock);
    component = 0;
    stage = 2;
}

}
