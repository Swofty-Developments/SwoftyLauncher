#include "core/apply/report_queue_logic.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
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
    std::atomic<bool> writing{false}, stop{false};
    std::thread writer([&] {
        std::string when;
        while (!stop) {
            if (!q.take_for(5ms, when))
                continue;
            writing = true;
            std::this_thread::sleep_for(30ms);
            written.push_back(when);
            writing = false;
        }
    });
    q.want("first");
    while (!writing)
        std::this_thread::yield();
    q.want("second");
    q.want("third");
    std::this_thread::sleep_for(150ms);
    stop = true;
    writer.join();
    check(written.size() == 2, "an ask made while a write runs gives exactly one more write");
    check(written.size() == 2 && written[0] == "first" && written[1] == "third",
          "the write after it is as of the newest ask");
}

void the_asker_never_waits_for_a_write()
{
    ReportQueue q;
    std::atomic<bool> stop{false};
    std::thread writer([&] {
        std::string when;
        while (!stop)
            if (q.take_for(5ms, when))
                std::this_thread::sleep_for(100ms);
    });
    q.want("a");
    std::this_thread::sleep_for(10ms);
    auto t0 = std::chrono::steady_clock::now();
    for (int i = 0; i < 100; i++)
        q.want("b");
    double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
    stop = true;
    writer.join();
    check(ms < 20, "asking while a write runs returns at once");
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
