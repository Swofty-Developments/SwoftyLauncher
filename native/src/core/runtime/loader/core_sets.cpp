#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"

#include <array>

namespace runtime::loader {

namespace {

std::array<CoreSet *, CORE_SET_COUNT> &registered()
{
    static std::array<CoreSet *, CORE_SET_COUNT> sets{};
    return sets;
}

}

CoreSet::CoreSet(CoreSetId id) : id_(id) { registered()[size_t(id)] = this; }

FunctionSet::FunctionSet(CoreSetId id, Offer offer, Commit commit) : CoreSet(id), offer_(offer), commit_(commit) {}

std::vector<runtime::CoreMixins> FunctionSet::offer() { return offer_(); }

void FunctionSet::commit()
{
    if (commit_)
        commit_();
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
