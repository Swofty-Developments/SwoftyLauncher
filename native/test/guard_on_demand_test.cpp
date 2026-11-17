// Host-native unit test for the guards that hook game code when their owner first arms them
// (native/src/core/guards/on_demand.hpp). Built and run by native/test/run_tests.sh guard_on_demand.

#include "core/guards/on_demand.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <thread>
#include <vector>

namespace guards = swofty_core::guards;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

class Counting final : public guards::OnDemandGuard {
public:
    constexpr explicit Counting(bool result) : result_(result) {}

    std::atomic<int> installs{0};
    std::atomic<int> running{0};
    std::atomic<int> overlapped{0};

protected:
    bool install() override
    {
        if (running.fetch_add(1) != 0)
            overlapped++;
        installs++;
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
        running--;
        return result_;
    }

private:
    bool result_;
};

constinit Counting hooks{true};
constinit Counting refused{false};

static void race(Counting &guard, bool want, const char *one, const char *same, const char *after)
{
    constexpr int THREADS = 16;
    std::atomic<bool> go{false};
    std::atomic<int> agreed{0}, finished_first{0};
    std::vector<std::thread> threads;
    for (int i = 0; i < THREADS; i++)
        threads.emplace_back([&] {
            while (!go.load())
                std::this_thread::yield();
            bool got = guard.arm();
            agreed += got == want;
            finished_first += guard.installs.load() == 1 && guard.running.load() == 0;
        });
    go = true;
    for (auto &t : threads)
        t.join();
    expect(guard.installs.load() == 1 && guard.overlapped.load() == 0, one);
    expect(agreed.load() == THREADS && finished_first.load() == THREADS, same);
    for (int i = 0; i < 100; i++)
        guard.arm();
    expect(guard.installs.load() == 1 && guard.armed() == want, after);
}

int main()
{
    expect(!hooks.armed(), "a guard nobody armed is not armed");
    race(hooks, true, "sixteen threads arming at once run one install",
         "every thread sees the install's result once it has finished", "later arms do not install again");
    race(refused, false, "a refused install also runs once under sixteen threads",
         "every thread sees the refusal once it has finished", "a refused guard is not retried and stays unarmed");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
