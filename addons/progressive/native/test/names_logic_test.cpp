// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The label cache: built once, reused, rebuilt on a new database fingerprint, never duplicated
// across threads, lookups unchanged (names_logic.hpp).

#include "names_logic.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <thread>
#include <vector>

using namespace pg::names_logic;

namespace {

int failures = 0;

void expect(bool ok, const char *what, int line)
{
    if (!ok) {
        std::printf("FAIL line %d: %s\n", line, what);
        ++failures;
    }
}

#define EXPECT(x) expect(static_cast<bool>(x), #x, __LINE__)

Tables sample(uint32_t first)
{
    Tables t;
    t.kinds[0].add(first, "driver_a");
    t.kinds[0].add(first + 1, "driver_b");
    t.kinds[1].add(7, "team_a");
    t.kinds[2].add(9, "vehicle_a");
    return t;
}

} // namespace

int main()
{
    Cache c;
    EXPECT(!c.ready());
    EXPECT(c.claim(1, 0, 5000));
    EXPECT(c.building());
    EXPECT(!c.claim(1, 0, 5000));
    EXPECT(!c.claim(2, 0, 5000));
    c.finish(1, sample(10), 0);
    auto first = c.ready();
    EXPECT(first && !c.building());
    EXPECT(first->kinds[0].by_stat.at(10) == "driver_a");
    EXPECT(first->kinds[0].by_label.at("driver_b") == 11);
    EXPECT(first->kinds[1].by_stat.at(7) == "team_a");
    EXPECT(first->kinds[2].by_label.at("vehicle_a") == 9);

    EXPECT(!c.claim(1, 100, 5000));
    EXPECT(c.ready() == first);

    EXPECT(c.claim(2, 100, 5000));
    EXPECT(c.ready() == first);
    EXPECT(c.ready()->kinds[0].by_stat.count(10) == 1);
    c.finish(2, sample(20), 100);
    auto second = c.ready();
    EXPECT(second != first);
    EXPECT(second->kinds[0].by_stat.at(20) == "driver_a");
    EXPECT(second->kinds[0].by_stat.count(10) == 0);
    EXPECT(first->kinds[0].by_stat.at(10) == "driver_a");

    Cache f;
    EXPECT(f.claim(5, 1000, 5000));
    f.finish(5, Tables{}, 1000);
    EXPECT(!f.ready() && !f.building());
    EXPECT(!f.claim(5, 3000, 5000));
    EXPECT(f.claim(5, 6000, 5000));
    EXPECT(f.claim(6, 6000, 5000) == false);
    f.finish(5, sample(1), 6000);
    EXPECT(f.ready());

    Cache g;
    std::atomic<int> builds{0};
    std::vector<std::thread> threads;
    for (int i = 0; i < 8; ++i)
        threads.emplace_back([&] {
            if (g.claim(3, 0, 5000)) {
                ++builds;
                std::this_thread::sleep_for(std::chrono::milliseconds(30));
                g.finish(3, sample(1), 0);
            }
            g.wait();
            if (!g.ready())
                ++builds;
        });
    for (auto &t : threads)
        t.join();
    EXPECT(builds == 1);
    EXPECT(g.ready());

    if (failures)
        return 1;
    std::printf("names_logic_test: ok\n");
    return 0;
}
