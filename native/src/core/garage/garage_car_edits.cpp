// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#include "core/garage/garage_edits.hpp"

namespace garage::edits {

namespace {

uint32_t vehicle_id()
{
    static const auto id = node("vehicle");
    return id;
}

R assign_vehicle(const LivePtr &live, const vehicle::Pose &prior, const Matrix &world)
{
    auto propagate = [](Address scene) {
        refresh(reinterpret_cast<void *>(scene), 1);
        return true;
    };
    return vehicle::assign(live->owner, vehicle_id(), prior, world, read_value, write_field, propagate);
}

}

R set_car(uint64_t t, const sdk::WorldMatrix *target)
{
    auto v = context(t);
    if (!v || !target || !v->live->idle || v->live->departure || !scope(v->live, v->bank))
        return R::unavailable;
    const auto &live = v->live;
    if (live->vehicle && live->vehicle->step != CarStep::active)
        return live->car_result;
    vehicle::Pose before{};
    if (!vehicle::capture(live->owner, vehicle_id(), read_value, before))
        return R::unavailable;
    Matrix world{}, relative{};
    std::copy_n(target->values, 16, world.begin());
    if (!vehicle::relative(before.car_world, world, relative))
        return R::invalid;
    const Matrix after = rig_detail::world_from_definition(before.scene_world, relative);
    const auto result = assign_vehicle(live, before, after);
    live->car_result = result;
    if (result == R::pending || result == R::rolled_back || result == R::rollback_failed) {
        const bool reverting = result == R::rolled_back;
        SavedCar saved{live->vehicle ? live->vehicle->native : before,
                       before,
                       reverting ? before.car_world : world,
                       reverting ? CarStep::rollback : CarStep::verify,
                       GetTickCount64(),
                       bool(live->vehicle),
                       0};
        if (!live->edit_scope)
            live->edit_scope = Scope{v->track, v->bank};
        live->vehicle = saved;
        live->car_result = reverting ? R::pending : result;
        live->sync();
    }
    return live->car_result;
}

R begin_car_restore(const LivePtr &live, bool rollback)
{
    auto &saved = *live->vehicle;
    vehicle::Pose before{};
    if (!vehicle::capture(live->owner, vehicle_id(), read_value, before) || before.scene != saved.native.scene)
        return R::unavailable;
    const auto &target = rollback ? saved.prior : saved.native;
    auto r = assign_vehicle(live, before, target.scene_world);
    if (r == R::pending) {
        saved.expected = target.car_world;
        saved.since = GetTickCount64();
        saved.observations = 0;
        saved.step = rollback ? CarStep::rollback : CarStep::restore;
    }
    live->car_result = r;
    return r;
}

R restore_car(uint64_t t)
{
    auto v = context(t);
    if (!v || !scope(v->live, v->bank))
        return R::unavailable;
    if (!v->live->vehicle)
        return R::ok;
    if (v->live->vehicle->step == CarStep::restore && v->live->car_result == R::pending)
        return R::pending;
    return begin_car_restore(v->live, false);
}

R car_result(uint64_t t)
{
    auto v = context(t);
    return v ? v->live->car_result : R::unavailable;
}

void check_car(const LivePtr &live)
{
    auto &saved = *live->vehicle;
    if (saved.step == CarStep::active || !live->idle || live->car_result != R::pending || !live->edit_scope ||
        !scope(live, live->edit_scope->bank))
        return;
    vehicle::Pose actual{};
    const bool observed = vehicle::capture(live->owner, vehicle_id(), read_value, actual) && actual.scene == saved.native.scene;
    switch (vehicle::verify(saved, observed ? &actual.car_world : nullptr, GetTickCount64())) {
    case vehicle::Verification::wait:
        return;
    case vehicle::Verification::active:
        live->car_result = R::ok;
        break;
    case vehicle::Verification::restored:
        live->vehicle.reset();
        live->car_result = R::ok;
        break;
    case vehicle::Verification::previous:
        if (!saved.prior_override)
            live->vehicle.reset();
        live->car_result = R::rolled_back;
        break;
    case vehicle::Verification::revert:
        if (begin_car_restore(live, true) != R::pending)
            live->car_result = R::rollback_failed;
        break;
    case vehicle::Verification::failed:
        live->car_result = R::rollback_failed;
        break;
    }
    live->sync();
}

}
