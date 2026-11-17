#include "core/guards/guard.hpp"
#include "core/guards/install.hpp"
#include "core/guards/lines.hpp"
#include "core/services/lap_host.hpp"

#include <windows.h>

#include <array>
#include <cstring>
#include <utility>

namespace swofty_core::guards {

namespace {

std::array<CoreSet *, CORE_SET_COUNT> &registered()
{
    static std::array<CoreSet *, CORE_SET_COUNT> sets{};
    return sets;
}

std::array<const HookGuard *, HOOK_GUARD_COUNT> &hook_guards()
{
    static std::array<const HookGuard *, HOOK_GUARD_COUNT> guards{};
    return guards;
}

}

CoreSet::CoreSet(CoreSetId id) : id_(id) { registered()[size_t(id)] = this; }

FunctionSet::FunctionSet(CoreSetId id, Offer offer, Commit commit) : CoreSet(id), offer_(offer), commit_(commit) {}

bool env_off(const char *name, char (&value)[16], bool (*off_value)(const char *))
{
    value[0] = 0;
    DWORD n = GetEnvironmentVariableA(name, value, sizeof value);
    if (!n || n >= sizeof value)
        return false;
    return off_value ? off_value(value) : switch_off(value);
}

std::vector<runtime::CoreMixins> Guard::offer()
{
    std::vector<runtime::CoreMixins> out;
    if (offered_ || !runtime::code_ready() || !ready())
        return out;
    offered_ = true;
    build(out);
    return out;
}

void Guard::commit()
{
    if (!offered_ || settled_)
        return;
    settled_ = true;
    settle();
}

bool resolve(const std::vector<Keep> &keeps, const std::vector<const char *> &needs)
{
    bool found = true;
    for (auto &k : keeps)
        found &= (*k.address = swofty::api->resolve(k.binding)) != 0;
    for (const char *need : needs) {
        if (!found)
            break;
        bool keep = false;
        for (auto &k : keeps)
            keep |= !strcmp(k.binding, need);
        if (!keep)
            found = swofty::api->resolve(need) != 0;
    }
    if (!found)
        for (auto &k : keeps)
            *k.address = 0;
    return found;
}

SiteGuard::SiteGuard(CoreSetId id, SiteGuardSpec spec) : Guard(id), spec_(std::move(spec)) {}

void SiteGuard::build(std::vector<runtime::CoreMixins> &out)
{
    char value[16];
    if (env_off(spec_.env, value)) {
        swofty::log("%s", lines::off(spec_.name, spec_.env, value).c_str());
        return;
    }
    if (!resolve(spec_.keeps, spec_.needs)) {
        swofty::log("%s", lines::unresolved(spec_.name, spec_.needs).c_str());
        return;
    }
    if (!admit())
        return;
    wanted_ = true;
    runtime::CoreMixins set{spec_.set, spec_.mixins};
    extend(set);
    out.push_back(std::move(set));
}

void SiteGuard::settle()
{
    if (!wanted_ && !spec_.settles_when_off)
        return;
    if (runtime::core_mixins_active(spec_.set))
        swofty::log("%s", lines::armed(spec_.name, spec_.armed).c_str());
    else
        swofty::log("%s", lines::not_installed(spec_.name).c_str());
}

namespace {

// The core sets whose subsystems still export them as function pairs.
FunctionSet function_sets[] = {
    {CoreSetId::save_sandbox, runtime::save_sandbox_mixins, runtime::save_sandbox_commit},
    {CoreSetId::career_content, runtime::career_content_mixins, runtime::career_content_commit},
    {CoreSetId::characters, runtime::characters_mixins, runtime::characters_commit},
    {CoreSetId::wizard, runtime::wizard_mixins, runtime::wizard_commit},
    {CoreSetId::tile_route, runtime::tile_route_mixins, runtime::tile_route_commit},
    {CoreSetId::era, runtime::era_mixins, runtime::era_commit},
    {CoreSetId::tt_guard, runtime::tt_guard_mixins, runtime::tt_guard_commit},
    {CoreSetId::career_alias, runtime::career_alias_mixins, runtime::career_alias_commit},
    {CoreSetId::car_category, runtime::car_category_mixins, runtime::car_category_commit},
    {CoreSetId::save_splice, runtime::save_splice_mixins, runtime::save_splice_commit},
    {CoreSetId::car_levers, runtime::car_levers_mixins, runtime::car_levers_commit},
    {CoreSetId::garage_placement, runtime::garage_placement_mixins},
    {CoreSetId::tyre_models, runtime::tyre_models_mixins, runtime::tyre_models_commit},
    {CoreSetId::loading, runtime::loading_mixins, runtime::loading_commit},
    {CoreSetId::lap_host, runtime::lap_host_mixins, runtime::lap_host_commit},
    {CoreSetId::box_grid, runtime::box_grid_mixins, runtime::box_grid_commit},
};

}

HookGuard::HookGuard(HookGuardId id, const char *what, void (*setup)()) : id_(id), what_(what), setup_(setup)
{
    hook_guards()[size_t(id)] = this;
}

void HookGuard::arm() const { runtime::when_code_ready(what_, setup_); }

void arm_next_hook_guard()
{
    static size_t next = 0;
    while (next < HOOK_GUARD_COUNT)
        if (const HookGuard *guard = hook_guards()[next++]) {
            guard->arm();
            return;
        }
}

void offer_core_sets(void (*add)(std::vector<runtime::CoreMixins>))
{
    for (CoreSet *set : registered())
        if (set)
            add(set->offer());
}

void commit_core_sets()
{
    for (CoreSetId id : COMMIT_ORDER)
        if (CoreSet *set = registered()[size_t(id)])
            set->commit();
}

}
