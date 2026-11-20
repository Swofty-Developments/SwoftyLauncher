#include "core/apply/report_queue_logic.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <future>
#include <string>
#include <thread>
#include <vector>

namespace {

using namespace std::chrono_literals;
using swofty_core::apply::ReportQueue;

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what);
        failures++;
    }
}

void nothing_wanted_writes_nothing()
{
    ReportQueue q;
    std::string when;
    check(!q.take_for(10ms, when), "a queue nobody asked gives no write");
}

void asks_fold_into_the_newest()
{
    ReportQueue q;
    q.want("StateLoadSession 1");
    q.want("StateLoadSession 2");
    q.want("StateLoadSession 3");
    std::string when;
    check(q.take_for(10ms, when) && when == "StateLoadSession 3", "asks made before the writer looks fold into the newest");
    check(!q.take_for(10ms, when), "and give one write");
}

void an_ask_during_a_write_writes_again()
{
    ReportQueue q;
    std::vector<std::string> written;
    std::atomic<int> writing{0};
    std::atomic<bool> finish{false};
    std::thread writer([&] {
        for (int i = 0; i < 2; i++) {
            std::string when = q.take();
            writing++;
            while (!finish)
                std::this_thread::yield();
            written.push_back(when);
        }
    });
    q.want("first");
    while (writing < 1)
        std::this_thread::yield();
    q.want("second");
    q.want("third");
    finish = true;
    writer.join();
    std::string more;
    check(written.size() == 2 && written[0] == "first" && written[1] == "third",
          "an ask made while a write runs gives one more write, as of the newest ask");
    check(!q.take_for(10ms, more), "and nothing after it");
}

void the_asker_never_waits_for_a_write()
{
    ReportQueue q;
    std::atomic<bool> writing{false}, finish{false};
    std::thread writer([&] {
        q.take();
        writing = true;
        while (!finish)
            std::this_thread::yield();
    });
    q.want("a");
    while (!writing)
        std::this_thread::yield();
    auto asks = std::async(std::launch::async, [&] {
        for (int i = 0; i < 100; i++)
            q.want("b");
    });
    bool returned = asks.wait_for(10s) == std::future_status::ready;
    finish = true;
    writer.join();
    asks.wait();
    check(returned, "asking while a write runs returns without waiting for it");
}

}

int main()
{
    nothing_wanted_writes_nothing();
    asks_fold_into_the_newest();
    an_ask_during_a_write_writes_again();
    the_asker_never_waits_for_a_write();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("report queue: ok\n");
    return 0;
}
