// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#include "core/garage/garage_edits.hpp"

namespace garage::edits {

namespace {

CrewIds ids() { return {node("MB"), node("EB1"), node("MB:")}; }

R pose(uint64_t ticket, sdk::WorldMatrix *out, bool car)
{
    auto v = context(ticket);
    Matrix m{};
    if (!v || !out)
        return R::unavailable;
    const auto &o = v->live->owner;
    if (!(car ? parked_car_world(read_value, o, m) : read_value(o.instance + 0x1c0, m)) || !rig_detail::affine(m))
        return R::unavailable;
    copy(m, *out);
    return R::ok;
}

}

R adjust(const LivePtr &live, const SavedCrew &saved, bool restore)
{
    auto scoped = [&](const Owner &) { return scope(live, saved.bank) && (restore || live->idle.load()); };
    auto push = [&](CrewRole role, const Matrix &world) {
        return push_central_world(
            live->owner, saved.placement, role, world, read_value, [](Address p) { return page(p, false); },
            [](Address p) { return page(p, true); });
    };
    switch (set_central_crew(live->owner, saved.placement, restore, read_value, scoped, push)) {
    case CrewResult::applied:
    case CrewResult::restored:
        return R::ok;
    case CrewResult::rolled_back:
        return R::rolled_back;
    case CrewResult::rollback_failed:
        return R::rollback_failed;
    default:
        return R::unavailable;
    }
}

R root(uint64_t t, sdk::WorldMatrix *out) { return pose(t, out, false); }

R car(uint64_t t, sdk::WorldMatrix *out) { return pose(t, out, true); }

R set_root(uint64_t t, const sdk::WorldMatrix *m)
{
    auto v = context(t);
    if (!v || !m || !v->live->idle || v->live->departure || v->live->crew)
        return R::unavailable;
    if (v->live->vehicle || v->live->has_individual())
        return R::conflict;
    Matrix after{};
    std::copy_n(m->values, 16, after.begin());
    auto scoped = [&](const Owner &) { return scope(v->live, v->bank) && v->live->idle.load(); };
    auto propagate = [](Address scene) {
        refresh(reinterpret_cast<void *>(scene), 1);
        return true;
    };
    switch (set_garage_root(v->live->owner, after, read_value, write_field, scoped, propagate)) {
    case RootResult::applied:
        return R::ok;
    case RootResult::invalid_transform:
        return R::invalid;
    case RootResult::rolled_back:
        return R::rolled_back;
    case RootResult::rollback_failed:
        return R::rollback_failed;
    default:
        return R::unavailable;
    }
}

R read_crew(uint64_t t, sdk::ActorRoots *out)
{
    auto v = context(t);
    if (!v || !out)
        return R::unavailable;
    CentralCrew roles{};
    Matrix mb{}, eb1{};
    if (!central_crew(read_value, v->live->owner, ids(), roles) || !crew_detail::model_world(read_value, roles.mb, mb) ||
        !crew_detail::model_world(read_value, roles.eb1, eb1))
        return R::unavailable;
    copy(mb, out->mb);
    copy(eb1, out->eb1);
    return R::ok;
}

bool actor_pelvis(Address actor, sdk::Position &out)
{
    if (!find_node)
        return false;
    auto read = [](uint64_t p, void *dst, size_t n, void *) {
        if (!swofty::mem::readable(static_cast<Address>(p), n))
            return false;
        std::memcpy(dst, reinterpret_cast<const void *>(p), n);
        return true;
    };
    garage::crew_pose::Position absolute{};
    if (!garage::crew_pose::read_pelvis(read, nullptr, actor, node("Bip01"), find_node, absolute))
        return false;
    out = {absolute.x, absolute.y, absolute.z};
    return true;
}

R read_pelvis(uint64_t t, sdk::CrewPositions *out)
{
    auto v = context(t);
    if (!v || !out || !find_node)
        return R::unavailable;
    CentralCrew roles{};
    if (!central_crew(read_value, v->live->owner, ids(), roles))
        return R::unavailable;
    sdk::CrewPositions both{};
    if (!actor_pelvis(roles.mb, both.mb) || !actor_pelvis(roles.eb1, both.eb1))
        return R::unavailable;
    *out = both;
    return R::ok;
}

R set_crew(uint64_t t, const sdk::Offsets *offsets)
{
    auto v = context(t);
    if (!v || !offsets || !v->live->idle || v->live->departure)
        return R::unavailable;
    if (members::conflicts_with_member(v->live->individual))
        return R::conflict;
    Matrix car_world{};
    const auto &o = v->live->owner;
    std::array<CrewDelta, 2> deltas;
    std::copy_n(offsets->mb, 3, deltas[0].begin());
    std::copy_n(offsets->eb1, 3, deltas[1].begin());
    SavedCrew saved{{}, v->track, v->bank};
    auto scoped = [&](const Owner &) { return scope(v->live, v->bank) && v->live->idle.load(); };
    if (!parked_car_world(read_value, o, car_world) ||
        !prepare_central_crew(o, ids(), car_world, deltas, read_value, scoped, saved.placement))
        return R::unavailable;
    auto r = adjust(v->live, saved, false);
    if (r == R::ok || r == R::rollback_failed)
        v->live->remember(std::move(saved));
    else if (r == R::rolled_back)
        v->live->forget();
    return r;
}

R restore_crew(uint64_t t)
{
    auto v = context(t);
    if (!v || !v->live->crew)
        return R::unavailable;
    auto r = adjust(v->live, *v->live->crew, true);
    if (r == R::ok)
        v->live->forget();
    return r;
}

}
