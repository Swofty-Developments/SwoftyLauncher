#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/diagnostics/tyre_models_logic.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

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
constexpr size_t TC_GROUP_OLD = 0x9920, TC_GROUP_NEW = 0x9938, GROUP_SIZE = 0x18;
constexpr int64_t FIX_FIRST_MS = 2000, FIX_LAST_MS = 90000, FIX_PERIOD_MS = 1000;

void (*start_orig)(void *) = nullptr;
runtime::loader::MixinGate mixin_gate;
bool wanted = false;
std::mutex lock;
uintptr_t component = 0;
uint32_t session = 0;
int64_t started_ms = 0;
std::atomic<int> stage{2};
bool fix_armed = false, fix_logged = false;
int64_t fix_last_ms = 0;

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
    else if (car.vehicle == 0 || car.vehicle == tmod::FIRST_UNGARAGED_VEHICLE)
        log_activation(tc, car.vehicle, "placed", entry);
    return car;
}

bool read_group(uintptr_t at, tmod::Group &g)
{
    return mem::read(at, g.option) && mem::read(at + 8, g.physical) && mem::read(at + 0x10, g.visual);
}

void write_group(uintptr_t at, const void *from) { std::memcpy(reinterpret_cast<void *>(at), from, GROUP_SIZE); }

bool repair_car(uintptr_t car_list, uintptr_t row, unsigned &done)
{
    uint8_t vehicle = 0xff;
    mem::read(row + ENTRANT_VEHICLE, vehicle);
    if (vehicle < tmod::FIRST_UNGARAGED_VEHICLE || vehicle == 0xff)
        return false;
    uintptr_t tc = tyre_component_of(car_list, vehicle), fitted = 0;
    uint64_t count = 0;
    uint32_t option = 0xffffffff;
    tmod::Group old_group, new_group;
    if (!tc || !mem::object_like(tc) || !read_group(tc + TC_GROUP_OLD, old_group) || !read_group(tc + TC_GROUP_NEW, new_group) ||
        !mem::read(tc + TC_COUNT, count) || count > runtime::tyre_slots::CAPACITY || !read_ptr(tc + TC_FITTED, fitted) ||
        !mem::read(fitted + FITTED_OPTION, option) || option > 4)
        return false;
    runtime::tyre_slots::Slot slots[runtime::tyre_slots::CAPACITY] = {};
    for (uint64_t i = 0; i < count; i++) {
        uintptr_t s = tc + TC_SLOTS + i * SLOT_STRIDE;
        if (!mem::read(s, slots[i].option) || !mem::read(s + 8, slots[i].physical) || !mem::read(s + 0x10, slots[i].visual))
            return false;
    }
    int pick = runtime::tyre_slots::find(slots, count, option);
    bool slot_usable = pick >= 0 && runtime::tyre_slots::usable(slots[pick]);
    auto plan = tmod::plan_activation(vehicle, old_group, new_group, slot_usable);
    if (plan == tmod::Repair::none)
        return false;
    uintptr_t source = plan == tmod::Repair::copy_new_to_old ? tc + TC_GROUP_NEW : tc + TC_SLOTS + size_t(pick) * SLOT_STRIDE;
    uint8_t copy[GROUP_SIZE];
    for (size_t i = 0; i < GROUP_SIZE; i++)
        if (!mem::read(source + i, copy[i]))
            return false;
    write_group(tc + TC_GROUP_OLD, copy);
    if (plan == tmod::Repair::copy_slot_to_both)
        write_group(tc + TC_GROUP_NEW, copy);
    done |= 1u << (vehicle & 31);
    return true;
}

void repair_pass()
{
    uintptr_t career = 0, car_list = 0, begin = 0, end = 0;
    if (!read_ptr(component + COMP_CAREER, career) || !read_ptr(component + COMP_CAR_LIST, car_list) ||
        !mem::read(career + CAREER_ENTRANTS, begin) || !mem::read(career + CAREER_ENTRANTS + 8, end) || !begin || end <= begin ||
        (end - begin) % ENTRANT_SIZE || (end - begin) / ENTRANT_SIZE > ENTRANT_MAX || !mem::readable(begin, end - begin))
        return;
    unsigned done = 0;
    for (uintptr_t row = begin; row < end; row += ENTRANT_SIZE)
        repair_car(car_list, row, done);
    if (!done || fix_logged)
        return;
    fix_logged = true;
    std::string list;
    for (unsigned v = tmod::FIRST_UNGARAGED_VEHICLE; v < 32; v++)
        if (done & (1u << v))
            list += (list.empty() ? "" : ", ") + std::to_string(v);
    swofty::log("[tyres] activation fix, session %u: cars %s had no previous tyre compound slot; the fitted compound's slot was "
                "copied into it so the tyre models are placed",
                session, list.c_str());
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
    fix_logged = false;
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
    DWORD f = GetEnvironmentVariableA("SWOFTY_TYRE_ACTIVATE_FIX", v, sizeof v);
    fix_armed = f && f < sizeof v && !strcmp(v, "1");
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
    if (fix_armed)
        swofty::log(
            "[tyres] activation fix armed (SWOFTY_TYRE_ACTIVATE_FIX=1): cars 23 and up with no previous compound slot get "
            "the fitted compound's slot");
}

static loader::FunctionSet tyre_models_core_set{loader::CoreSetId::tyre_models, tyre_models_mixins, tyre_models_commit};

void tyre_models_tick()
{
    if (stage >= 2 && !fix_armed)
        return;
    std::lock_guard<std::mutex> g(lock);
    if (!component)
        return;
    if (fix_armed) {
        int64_t since = now_ms() - started_ms;
        if (since >= FIX_FIRST_MS && since <= FIX_LAST_MS && now_ms() - fix_last_ms >= FIX_PERIOD_MS) {
            fix_last_ms = now_ms();
            repair_pass();
        }
    }
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
