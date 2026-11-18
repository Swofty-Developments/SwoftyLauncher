// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"
#include "core/apply/moments.hpp"
#include "core/garage/garage_edits.hpp"
#include <swofty/mixin.hpp>
#include <swofty/mem.hpp>
#include <atomic>
#include <memory>
#include <mutex>

namespace garage::edits {

std::atomic<unsigned> active_edits{};
Refresh refresh{};
Node node{};
garage::crew_pose::FindNode find_node{};

namespace {

constexpr const char *mixin = "swofty_core.dll (garage placement)";
constexpr const char *scene_site = "site_Game_GarageCrew_SceneRead";
constexpr const char *sequence_fn = "Game_GarageCrewInstance_PlaySequence";
constexpr const char *update_fn = "Game_PitBoxCarActor_Update";
Play play_original{};
Update update_original{};
std::atomic<bool> placement_enabled{false};

std::mutex state_mutex;
std::array<LivePtr, 64> bays;
struct Sequence {
    Address instance{};
    ULONGLONG tick{};
    bool idle{};
};
std::array<Sequence, 64> early_sequences{};
thread_local LivePtr updating;
thread_local Visit *visiting = nullptr;
std::atomic<uint64_t> tickets{0};
uint64_t generations{}; // Protected by state_mutex and monotonic across unloads.

}

bool current(const LivePtr &live)
{
    std::lock_guard lock(state_mutex);
    const auto &o = live->owner;
    return bays[o.garage * 2 + o.native_slot] == live;
}
bool same_track(const std::string &id) { return swofty_core::apply::moments::session_mode_track().second == id; }
bool scope(const LivePtr &live, const backend::Bank &bank)
{
    return current(live) && backend::owns(read_value, live->owner) && backend::matches(read_value, live->owner, bank);
}
Visit *context(uint64_t ticket)
{
    return visiting && visiting->ticket == ticket && visiting->live == updating && current(updating) &&
                   backend::owns(read_value, updating->owner)
               ? visiting
               : nullptr;
}
bool page(Address at, bool code)
{
    MEMORY_BASIC_INFORMATION p{};
    if (!VirtualQuery(reinterpret_cast<void *>(at), &p, sizeof(p)) || p.State != MEM_COMMIT || p.Type != MEM_IMAGE ||
        p.AllocationBase != GetModuleHandleW(nullptr) || (p.Protect & PAGE_GUARD))
        return false;
    DWORD access = p.Protect & 0xff;
    return code ? (access == PAGE_EXECUTE || access == PAGE_EXECUTE_READ || access == PAGE_EXECUTE_READWRITE ||
                   access == PAGE_EXECUTE_WRITECOPY)
                : (access == PAGE_READONLY || access == PAGE_EXECUTE_READ);
}

namespace {

bool restore_departure(const LivePtr &live);

void capture(SwoftyContext *c)
{
    Owner owner{};
    bool captured = owner_at_scene_read(read_value, c->rbp, c->rdi, c->rsp, owner);
    std::lock_guard lock(state_mutex);
    for (auto &old : bays)
        if (old && old->owner.instance == c->rbp)
            old.reset();
    if (!captured)
        return;
    auto live = std::make_shared<Live>();
    live->owner = owner;
    live->generation = ++generations;
    for (auto &e : early_sequences)
        if (e.instance == owner.instance) {
            live->idle = GetTickCount64() - e.tick <= 1000 && e.idle;
            e = {};
            break;
        }
    bays[owner.garage * 2 + owner.native_slot] = std::move(live);
}

uint64_t play(void *instance, const char *name, int32_t arg, float start, float blend)
{
    std::string sequence;
    bool idle = swofty::mem::c_string(name, sequence, 96) &&
                (sequence == "InGarageIdle" || sequence == "InGarageIdleInstant" || sequence == "InGarageIdleMirrored" ||
                 sequence == "InGarageIdleInstantMirrored");
    uint64_t result = play_original(instance, name, arg, start, blend);
    LivePtr live;
    {
        std::lock_guard lock(state_mutex);
        const Address key = reinterpret_cast<Address>(instance);
        for (const auto &b : bays)
            if (b && b->owner.instance == key) {
                live = b;
                break;
            }
        if (live) {
            live->idle = idle;
        } else {
            auto place = std::find_if(early_sequences.begin(), early_sequences.end(),
                                      [=](const Sequence &e) { return e.instance == key; });
            if (place == early_sequences.end())
                place = std::min_element(early_sequences.begin(), early_sequences.end(),
                                         [](const auto &a, const auto &b) { return a.tick < b.tick; });
            *place = {key, GetTickCount64(), idle};
        }
    }
    if (live && !idle) {
        live->departure = true;
        std::unique_lock operation(live->operation, std::try_to_lock);
        if (operation && restore_departure(live))
            live->departure = false;
    }
    return result;
}

void update(void *actor)
{
    update_original(actor);
    const bool listened = runtime::garage_placement_listening();
    if (updating || (!listened && !active_edits.load(std::memory_order_relaxed)))
        return;
    LivePtr live;
    {
        std::lock_guard lock(state_mutex);
        for (const auto &b : bays)
            if (b && b->owner.car_actor == reinterpret_cast<Address>(actor)) {
                if (live)
                    return; // Two live bays claim the same update. Neither is authorized.
                live = b;
            }
    }
    if (!live)
        return;
    std::unique_lock operation(live->operation, std::try_to_lock);
    if (!operation)
        return;
    updating = live;
    struct End {
        ~End() { updating.reset(); }
    } end;
    if (live->vehicle)
        check_car(live);
    if (live->departure && restore_departure(live))
        live->departure = false;
    if (listened)
        runtime::events_fire(SWOFTY_EVENT_GARAGE_PARKED_CAR, live->owner.garage, live->owner.native_slot, nullptr);
}

bool restore_departure(const LivePtr &live)
{
    bool ok = true;
    if (live->crew) {
        if (same_track(live->crew->track) && adjust(live, *live->crew, true) == R::ok)
            live->forget();
        else
            ok = false;
    }
    if (!live->edit_scope)
        return ok;
    if (!same_track(live->edit_scope->track) || !scope(live, live->edit_scope->bank))
        return false;
    for (unsigned i = 0; i < members::role_count; ++i)
        if (live->individual[i] && restore_one(live, members::Role(i)) != R::ok && live->individual[i])
            ok = false;
    if (live->vehicle && !(live->vehicle->step == CarStep::restore && live->car_result == R::pending))
        if (begin_car_restore(live, false) != R::pending)
            ok = false;
    return ok;
}

R visit(const char *track, const sdk::Pit *pits, uint32_t count, uint32_t garage, uint32_t slot, sdk::Visitor callback,
        void *user)
{
    if (!track || !*track || !callback || slot > 1 || garage >= 32)
        return R::invalid;
    if (visiting || !updating || updating->owner.garage != garage || updating->owner.native_slot != slot)
        return R::unavailable;
    Visit v{++tickets, updating, track, {}};
    if (!backend::authored(pits, count, garage, v.bank))
        return R::invalid;
    if (!same_track(v.track) || !scope(v.live, v.bank))
        return R::unavailable;
    visiting = &v;
    struct End {
        ~End() { visiting = nullptr; }
    } end;
    callback(sdk::Bay{v.ticket, v.live->generation, garage, slot, v.live->idle.load()}, user);
    return R::ok;
}

}

}

using namespace garage::edits;

namespace runtime {
static std::vector<CoreMixins> garage_placement_mixins()
{
    if (placement_enabled || !code_ready() || !backend::supported(build_distribution(), build_exe_sha()))
        return {};
    auto resolve = swofty::api->resolve;
    if (!resolve(scene_site) || !resolve(sequence_fn) || !resolve(update_fn))
        return {};
    refresh = reinterpret_cast<Refresh>(resolve("Game_Scene_UpdateInstanceTransformsBatch"));
    node = reinterpret_cast<Node>(resolve("Game_Get_ModelNodeId"));
    find_node = reinterpret_cast<garage::crew_pose::FindNode>(resolve("Ne_NodeTable_FindIndexByHash"));
    if (!refresh || !node)
        return {};
    placement_enabled = true;
    return {{mixin,
             {{scene_site, SWOFTY_MIXIN_SITE, 0, (void *)&capture, nullptr},
              {sequence_fn, SWOFTY_MIXIN_WRAP, 0, (void *)&play, (void **)&play_original},
              {update_fn, SWOFTY_MIXIN_WRAP, 0, (void *)&update, (void **)&update_original}}}};
}

static loader::FunctionSet garage_placement_core_set{loader::CoreSetId::garage_placement, garage_placement_mixins};

void garage_placement_reset()
{
    std::lock_guard lock(state_mutex);
    bays = {};
    early_sequences = {};
}
}

extern "C" __declspec(dllexport) const sdk::detail::Ops *SwoftyCoreGaragePlacement()
{
    static const sdk::detail::Ops api{1, &visit, &root, &car, &set_root, &read_crew, &set_crew, &restore_crew, &read_pelvis};
    return placement_enabled ? &api : nullptr;
}

extern "C" __declspec(dllexport) const sdk::detail::OpsV2 *SwoftyCoreGaragePlacementV2()
{
    static const sdk::detail::OpsV2 api{
        2,        &list_members, &read_member, &read_member_pelvis, &set_member, &offset_member, &restore_member,
        &set_car, &restore_car,  &car_result};
    return placement_enabled ? &api : nullptr;
}
