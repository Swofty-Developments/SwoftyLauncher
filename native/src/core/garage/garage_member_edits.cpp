// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#include "core/garage/garage_edits.hpp"

namespace garage::edits {

namespace {

const members::NamesByRole &role_ids()
{
    // The engine's model node intern table is process-global. Resolve only on the
    // first scoped game callback after the build and named model functions are verified.
    static const auto table = members::names(node);
    return table;
}

R move_member(const LivePtr &live, const members::Edit &change)
{
    auto push = [](Address actor, const Matrix &m) {
        return crew_detail::set_actor_world(
            actor, m, read_value, [](Address p) { return page(p, false); }, [](Address p) { return page(p, true); });
    };
    return members::apply(live->owner, role_ids(), change, read_value, push);
}

R edit_member(uint64_t t, members::Role role, const sdk::WorldMatrix *absolute, const sdk::Position *delta)
{
    auto v = context(t);
    if (!v || !v->live->idle || v->live->departure || !scope(v->live, v->bank))
        return R::unavailable;
    if (unsigned(role) >= members::role_count || (!absolute && !delta))
        return R::invalid;
    if (members::conflicts_with_pair(role, bool(v->live->crew)))
        return R::conflict;
    members::Edit change{};
    auto r = members::prepare(v->live->owner, role_ids(), role, read_value, change);
    if (r != R::ok)
        return r;
    // The allowlist comes from human scene macros. An actor also needs the
    // character skeleton expected by those authored roles before we move it.
    sdk::Position pelvis{};
    if (!actor_pelvis(change.actor.value, pelvis))
        return R::unavailable;
    auto &saved = v->live->individual[unsigned(role)];
    if (saved && saved->actor.value != change.actor.value) {
        saved.reset();
        v->live->sync();
    }
    if (saved)
        change.native = saved->native;
    if (absolute)
        std::copy_n(absolute->values, 16, change.target.begin());
    else {
        change.target = change.native;
        change.target[12] += delta->x;
        change.target[13] += delta->y;
        change.target[14] += delta->z;
    }
    r = move_member(v->live, change);
    if (r == R::ok || r == R::rollback_failed) {
        if (!v->live->edit_scope)
            v->live->edit_scope = Scope{v->track, v->bank};
        saved = change;
        v->live->sync();
    }
    return r;
}

}

R list_members(uint64_t t, sdk::Mechanics *out)
{
    auto v = context(t);
    members::Selection found{};
    if (!v || !out || !members::list(v->live->owner, role_ids(), read_value, found))
        return R::unavailable;
    *out = found.roles;
    return R::ok;
}

R read_member(uint64_t t, members::Role role, sdk::WorldMatrix *out)
{
    auto v = context(t);
    if (!v || !out)
        return R::unavailable;
    members::Actor actor{};
    auto r = members::select(v->live->owner, role_ids(), role, read_value, actor);
    if (r != R::ok)
        return r;
    Matrix world{};
    if (!crew_detail::model_world(read_value, actor.value, world))
        return R::unavailable;
    copy(world, *out);
    return R::ok;
}

R read_member_pelvis(uint64_t t, members::Role role, sdk::Position *out)
{
    auto v = context(t);
    if (!v || !out)
        return R::unavailable;
    members::Actor actor{};
    auto r = members::select(v->live->owner, role_ids(), role, read_value, actor);
    if (r != R::ok)
        return r;
    return actor_pelvis(actor.value, *out) ? R::ok : R::unavailable;
}

R set_member(uint64_t t, members::Role role, const sdk::WorldMatrix *m) { return edit_member(t, role, m, nullptr); }

R offset_member(uint64_t t, members::Role role, const sdk::Position *d) { return edit_member(t, role, nullptr, d); }

R restore_one(const LivePtr &live, members::Role role)
{
    auto &saved = live->individual[unsigned(role)];
    members::Edit now{};
    auto r = members::prepare(live->owner, role_ids(), role, read_value, now);
    if (r == R::missing || (r == R::ok && saved && saved->actor.value != now.actor.value)) {
        saved.reset();
        live->sync();
        return r == R::missing ? r : R::unavailable;
    }
    if (r != R::ok)
        return r;
    if (!saved)
        return R::ok;
    now.target = saved->native;
    r = move_member(live, now);
    if (r == R::ok) {
        saved.reset();
        live->sync();
    }
    return r;
}

R restore_member(uint64_t t, members::Role role)
{
    auto v = context(t);
    if (!v || !scope(v->live, v->bank))
        return R::unavailable;
    if (unsigned(role) >= members::role_count)
        return R::invalid;
    if (members::conflicts_with_pair(role, bool(v->live->crew)))
        return R::conflict;
    return restore_one(v->live, role);
}

}
