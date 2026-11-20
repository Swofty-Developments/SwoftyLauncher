#pragma once

#include <chrono>
#include <condition_variable>
#include <mutex>
#include <string>

namespace swofty_core::apply {

// The newest wanted write of the registry reports (effective.json, conflicts.json). A session load asks for one and
// goes on; one writer thread takes the asks in turn. An ask made while a write runs is written once more after it,
// and asks made in the meantime fold into the newest, so the files end as of the last ask.
class ReportQueue {
public:
    void want(std::string when)
    {
        {
            std::lock_guard<std::mutex> g(mu_);
            when_ = std::move(when);
            wanted_ = true;
        }
        cv_.notify_one();
    }

    std::string take()
    {
        std::unique_lock<std::mutex> g(mu_);
        cv_.wait(g, [&] { return wanted_; });
        wanted_ = false;
        return std::move(when_);
    }

    bool take_for(std::chrono::milliseconds wait, std::string &when)
    {
        std::unique_lock<std::mutex> g(mu_);
        if (!cv_.wait_for(g, wait, [&] { return wanted_; }))
            return false;
        wanted_ = false;
        when = std::move(when_);
        return true;
    }

private:
    std::mutex mu_;
    std::condition_variable cv_;
    std::string when_;
    bool wanted_ = false;
};

}
