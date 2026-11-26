// The driver-moves finalise in several messages (native/src/core/career/ops/finalise_chunks.hpp) against a fake of the game's
// Career_Build_LineupChanges, which keeps the first 40 changed drivers and drops the rest.

#include "core/career/ops/finalise_chunks.hpp"

#include <cstdio>
#include <map>
#include <set>
#include <vector>

using namespace career::ops::chunks;

namespace {

int failures = 0;

void expect(bool ok, const char *what, int line)
{
    if (!ok) {
        std::printf("FAIL line %d: %s\n", line, what);
        ++failures;
    }
}

#define EXPECT(x) expect((x), #x, __LINE__)

using Lineup = std::map<uint32_t, uint32_t>;

struct Change {
    uint32_t driver;
    uint32_t team;
};

struct Fake {
    Lineup current;
    Lineup baseline;
    std::vector<std::vector<Change>> posted;
    std::set<uint32_t> modifiers;

    std::vector<Change> build()
    {
        std::vector<Change> out;
        for (auto &[driver, team] : current) {
            auto b = baseline.find(driver);
            if (b != baseline.end() && b->second == team)
                continue;
            if (out.size() == 40)
                break;
            out.push_back({driver, team});
        }
        return out;
    }

    bool post(const std::vector<Change> &chunk)
    {
        posted.push_back(chunk);
        for (auto &c : chunk)
            modifiers.insert(c.driver);
        return true;
    }

    void settle(const std::vector<Change> &chunk)
    {
        for (auto &c : chunk)
            baseline[c.driver] = current[c.driver];
    }

    Result run(size_t max_passes = kMaxPasses)
    {
        return post_all([&] { return build(); }, [&](const std::vector<Change> &c) { return post(c); },
                        [&](const std::vector<Change> &c) { settle(c); }, kMessageCapacity, max_passes);
    }
};

Fake lineup_of(uint32_t moved)
{
    Fake f;
    for (uint32_t d = 0; d < 120; ++d) {
        f.baseline[d] = 0;
        f.current[d] = d < moved ? 1 + d % 11 : 0;
    }
    return f;
}

void under_the_cap_is_one_message()
{
    Fake f = lineup_of(25);
    Result r = f.run();
    EXPECT(r.ok);
    EXPECT(r.messages == 1);
    EXPECT(r.changes == 25);
    EXPECT(f.posted.size() == 1 && f.posted[0].size() == 25);
}

void no_change_still_posts_once()
{
    Fake f = lineup_of(0);
    Result r = f.run();
    EXPECT(r.ok);
    EXPECT(r.messages == 1);
    EXPECT(r.changes == 0);
}

void over_the_cap_reaches_every_driver()
{
    for (uint32_t moved : {41u, 44u, 80u, 100u}) {
        Fake f = lineup_of(moved);
        Result r = f.run();
        EXPECT(r.ok);
        EXPECT(r.changes == moved);
        EXPECT(f.modifiers.size() == moved);
        for (uint32_t d = 0; d < moved; ++d)
            EXPECT(f.modifiers.count(d) == 1);
        for (auto &m : f.posted)
            EXPECT(m.size() <= kMessageCapacity);
    }
}

void exactly_at_the_cap_ends_on_an_empty_build()
{
    Fake f = lineup_of(40);
    Result r = f.run();
    EXPECT(r.ok);
    EXPECT(r.messages == 1);
    EXPECT(r.changes == 40);
    EXPECT(f.modifiers.size() == 40);
}

void the_unfixed_builder_alone_misses_drivers()
{
    Fake f = lineup_of(44);
    std::vector<Change> once = f.build();
    f.post(once);
    EXPECT(f.modifiers.size() == 40);
    EXPECT(f.modifiers.count(43) == 0);
}

void a_failed_post_fails()
{
    Fake f = lineup_of(60);
    Result r = post_all([&] { return f.build(); }, [&](const std::vector<Change> &) { return false; },
                        [&](const std::vector<Change> &c) { f.settle(c); });
    EXPECT(!r.ok);
    EXPECT(r.messages == 0);
}

void too_many_passes_fail()
{
    Fake f = lineup_of(100);
    Result r = f.run(2);
    EXPECT(!r.ok);
    EXPECT(r.messages == 2);
}

} // namespace

int main()
{
    under_the_cap_is_one_message();
    no_change_still_posts_once();
    over_the_cap_reaches_every_driver();
    exactly_at_the_cap_ends_on_an_empty_build();
    the_unfixed_builder_alone_misses_drivers();
    a_failed_post_fails();
    too_many_passes_fail();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("finalise_chunks_test: ok\n");
    return 0;
}
