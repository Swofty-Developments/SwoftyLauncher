// career::report_season from off the flow thread (career_ops.cpp). The game runs its flow state machine on
// several threads, and an addon's career.loaded handler often runs on one that is not the thread of the frame's
// flow tick, so the report would come back late and wait for an idle Team HQ that a career resumed straight into
// a race weekend never reaches. The report is queued instead and run at the next flow tick, on the flow thread,
// before the session loads. The latest report of each caller is kept; one queued while another career was
// loaded is dropped. Pure logic, so native/test builds it on the host.

#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace career::ops::reports {

struct Queued {
    std::string caller, career;
    int year = 0, index = 0;
    uint32_t thread = 0;
};

struct Ran {
    Queued report;
    bool dropped = false;
    int result = 0;
};

class Queue {
public:
    static constexpr size_t MAX = 8;

    bool put(Queued q)
    {
        std::lock_guard<std::mutex> g(mu_);
        bool held = false;
        for (auto &h : q_)
            if (h.caller == q.caller) {
                h = std::move(q);
                held = true;
                break;
            }
        if (!held) {
            if (q_.size() >= MAX)
                return false;
            q_.push_back(std::move(q));
        }
        pending_.store(true, std::memory_order_release);
        return true;
    }

    std::vector<Queued> take()
    {
        if (!pending_.load(std::memory_order_acquire))
            return {};
        std::lock_guard<std::mutex> g(mu_);
        pending_.store(false, std::memory_order_release);
        return std::exchange(q_, {});
    }

    bool pending() const { return pending_.load(std::memory_order_acquire); }

private:
    std::mutex mu_;
    std::vector<Queued> q_;
    std::atomic<bool> pending_{false};
};

template <class Run> std::vector<Ran> drain(Queue &q, const std::string &career_now, Run &&run)
{
    std::vector<Ran> out;
    for (auto &r : q.take()) {
        Ran ran{std::move(r)};
        ran.dropped = !ran.report.career.empty() && ran.report.career != career_now;
        if (!ran.dropped)
            ran.result = run(ran.report.caller, ran.report.year, ran.report.index);
        out.push_back(std::move(ran));
    }
    return out;
}

inline std::string head(const Queued &q)
{
    return "report_season(" + std::to_string(q.year) + ", " + std::to_string(q.index) + ") from " + q.caller;
}

inline std::string queued_line(const Queued &q)
{
    return "[career-ops] " + head(q) + ": called off the flow thread (thread " + std::to_string(q.thread) +
           "); queued, the next flow tick runs it";
}

inline std::string full_line(const Queued &q)
{
    return "[career-ops] " + head(q) + ": called off the flow thread (thread " + std::to_string(q.thread) +
           "); late: " + std::to_string(Queue::MAX) + " other callers' reports are already queued";
}

inline std::string ran_line(const Ran &r, const char *result)
{
    if (r.dropped)
        return "[career-ops] " + head(r.report) + ", queued off the flow thread for career " + r.report.career +
               ": dropped, that career is no longer loaded";
    return "[career-ops] " + head(r.report) + ", queued off the flow thread: run at the flow tick: " + result;
}

}
