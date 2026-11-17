#include "core/guards/guard.hpp"
#include "core/guards/install.hpp"
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

std::string needs_text(const std::vector<const char *> &needs)
{
    std::string out;
    for (size_t i = 0; i < needs.size(); i++) {
        if (i)
            out += i + 1 == needs.size() ? " and " : ", ";
        out += needs[i];
    }
    return out;
}

}

CoreSet::CoreSet(CoreSetId id) : id_(id)
{
    registered()[size_t(id)] = this;
}

FunctionSet::FunctionSet(CoreSetId id, Offer offer, Commit commit) : CoreSet(id), offer_(offer), commit_(commit) {}

bool env_off(const char *name, char (&value)[16])
{
    value[0] = 0;
    DWORD n = GetEnvironmentVariableA(name, value, sizeof value);
    return n && n < sizeof value && (!strcmp(value, "0") || !strcmp(value, "off"));
}

SiteGuard::SiteGuard(CoreSetId id, SiteGuardSpec spec) : CoreSet(id), spec_(std::move(spec)) {}

bool SiteGuard::resolve()
{
    bool kept = true;
    for (auto &k : spec_.keeps)
        kept &= (*k.address = swofty::api->resolve(k.binding)) != 0;
    bool found = kept;
    for (const char *need : spec_.needs) {
        if (!found)
            break;
        bool keep = false;
        for (auto &k : spec_.keeps)
            keep |= !strcmp(k.binding, need);
        if (!keep)
            found = swofty::api->resolve(need) != 0;
    }
    if (found)
        return true;
    for (auto &k : spec_.keeps)
        *k.address = 0;
    swofty::log("%s off: needs %s, which this build does not resolve", spec_.name, needs_text(spec_.needs).c_str());
    return false;
}

std::vector<runtime::CoreMixins> SiteGuard::offer()
{
    std::vector<runtime::CoreMixins> out;
    if (offered_ || !runtime::code_ready())
        return out;
    offered_ = true;
    char value[16];
    if (env_off(spec_.env, value)) {
        swofty::log("%s off (%s=%s)", spec_.name, spec_.env, value);
        return out;
    }
    if (!resolve() || !admit())
        return out;
    wanted_ = true;
    out.push_back({spec_.set, spec_.mixins});
    return out;
}

bool SiteGuard::active() const
{
    return runtime::core_mixins_active(spec_.set);
}

void SiteGuard::commit()
{
    if (!offered_ || settled_ || (!wanted_ && !spec_.settles_when_off))
        return;
    settled_ = true;
    if (active())
        swofty::log("%s armed: %s", spec_.name, spec_.armed);
    else
        swofty::log("%s did not install (see the [core] lines)", spec_.name);
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
    {CoreSetId::garage_crew, runtime::garage_crew_mixins, runtime::garage_crew_commit},
    {CoreSetId::garage_placement, runtime::garage_placement_mixins},
    {CoreSetId::pit_slots, runtime::pit_slots_mixins, runtime::pit_slots_commit},
    {CoreSetId::pit_box, runtime::pit_box_mixins, runtime::pit_box_commit},
    {CoreSetId::standings_guard, runtime::standings_guard_mixins, runtime::standings_guard_commit},
    {CoreSetId::replay_grid_guard, runtime::replay_grid_guard_mixins, runtime::replay_grid_guard_commit},
    {CoreSetId::fe_tile_guard, runtime::fe_tile_guard_mixins, runtime::fe_tile_guard_commit},
    {CoreSetId::myteam_vehicle_guard, runtime::myteam_vehicle_guard_mixins, runtime::myteam_vehicle_guard_commit},
    {CoreSetId::myteam_vehicle_heal, runtime::myteam_vehicle_heal_mixins, runtime::myteam_vehicle_heal_commit},
    {CoreSetId::myteam_vehicle_keep, runtime::myteam_vehicle_keep_mixins, runtime::myteam_vehicle_keep_commit},
    {CoreSetId::tyre_models, runtime::tyre_models_mixins, runtime::tyre_models_commit},
    {CoreSetId::track_acclim, runtime::track_acclim_mixins, runtime::track_acclim_commit},
    {CoreSetId::audio_queue, runtime::audio_queue_mixins, runtime::audio_queue_commit},
    {CoreSetId::heap_sizes, runtime::heap_sizes_mixins, runtime::heap_sizes_commit},
    {CoreSetId::team_slots, runtime::team_slots_mixins, runtime::team_slots_commit},
    {CoreSetId::track_select, runtime::track_select_mixins, runtime::track_select_commit},
    {CoreSetId::leaderboard_guard, runtime::leaderboard_guard_mixins, runtime::leaderboard_guard_commit},
    {CoreSetId::lap_host, runtime::lap_host_mixins, runtime::lap_host_commit},
    {CoreSetId::quali_tyres, runtime::quali_tyres_mixins, runtime::quali_tyres_commit},
    {CoreSetId::box_grid, runtime::box_grid_mixins, runtime::box_grid_commit},
    {CoreSetId::gp_continue, runtime::gp_continue_mixins, runtime::gp_continue_commit},
    {CoreSetId::track_list_guard, runtime::track_list_guard_mixins, runtime::track_list_guard_commit},
    {CoreSetId::watchdog_grace, runtime::watchdog_grace_mixins, runtime::watchdog_grace_commit},
    {CoreSetId::theatre_list, runtime::theatre_list_mixins, runtime::theatre_list_commit},
    {CoreSetId::tile_list_select, runtime::tile_list_select_mixins, runtime::tile_list_select_commit},
    {CoreSetId::session_enter, runtime::session_enter_mixins, runtime::session_enter_commit},
};

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
