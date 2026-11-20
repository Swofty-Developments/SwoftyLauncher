#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/runtime.hpp"

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
