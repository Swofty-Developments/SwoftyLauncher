#include "core/runtime/loader/core_rank.hpp"
#include "core/runtime/loader/core_rank_logic.hpp"

#include <swofty/mixin.hpp>

#include <array>

namespace runtime::loader {

namespace {

constinit std::array<RankMarkAt, CORE_RANK_COUNT> marks{};
constinit size_t marked = 0;

size_t listed()
{
    size_t n = 0;
    for (auto *m = swofty::detail::mixins; m; m = m->next)
        n++;
    return n;
}

}

RankMark::RankMark(CoreRank rank) { marks[size_t(rank)] = {listed(), ++marked, true}; }

void rank_core_mixins()
{
    static bool ranked = false;
    if (!ranked)
        swofty::detail::mixins = rank_list(swofty::detail::mixins, marks.data(), marks.size());
    ranked = true;
}

}
