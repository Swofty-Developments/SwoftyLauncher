// The driver-moves finalise in as many messages as the line-up needs. The
// game's Career_Build_LineupChanges (0x142457a90 on 1.0.141, 0x142459690 on 1.0.145) fills a message
// with at most 40 changes and silently drops every driver after the 40th (its append is guarded by
// `cmp count, 0x28`). The _upgrades handler makes the team modifiers of each driver in the message
// only, so a driver left out has none and the AI development at Team HQ's exit
// (Career_Calc_UpgradeCost, null modifier list) crashes the game. A 2010 start moves 41 or more
// rows. Pure logic, no game access, so native/test builds it on the host.
//
// post_all builds a message, posts it, settles the baseline for the drivers it carried and builds
// again, until a message is not full. A line-up of 40 changes or fewer posts one message, as before.

#pragma once
#include <cstddef>

namespace career::ops::chunks {

constexpr size_t kMessageCapacity = 40;
constexpr size_t kMaxPasses = 8;

struct Result {
    bool ok = false;
    size_t changes = 0;
    size_t messages = 0;
};

template <class Build, class Post, class Settle>
Result post_all(Build &&build, Post &&post, Settle &&settle, size_t capacity = kMessageCapacity, size_t max_passes = kMaxPasses)
{
    Result r;
    for (size_t pass = 0; pass < max_passes; ++pass) {
        auto chunk = build();
        if (pass > 0 && chunk.empty()) {
            r.ok = true;
            return r;
        }
        if (!post(chunk))
            return r;
        ++r.messages;
        r.changes += chunk.size();
        if (chunk.size() < capacity) {
            r.ok = true;
            return r;
        }
        settle(chunk);
    }
    return r;
}

} // namespace career::ops::chunks
