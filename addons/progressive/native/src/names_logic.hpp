// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Progressive Career: the label tables and the build-once cache behind names.hpp. A table set is
// built off the game thread, published as an immutable snapshot and replaced only when the game
// database fingerprint changes; readers keep the last good snapshot while a rebuild runs.

#pragma once
#include <condition_variable>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>

namespace pg::names_logic {

struct Table {
    std::unordered_map<uint32_t, std::string> by_stat;
    std::unordered_map<std::string, uint32_t> by_label;

    void add(uint32_t stat, const std::string &name)
    {
        by_stat.emplace(stat, name);
        by_label.emplace(name, stat);
    }
};

struct Tables {
    Table kinds[4];
};

inline bool usable(const Tables &t)
{
    return !t.kinds[0].by_stat.empty() && !t.kinds[2].by_stat.empty();
}

class Cache {
public:
    using Ptr = std::shared_ptr<const Tables>;

    Ptr ready() const
    {
        std::lock_guard<std::mutex> g(m_);
        return cur_;
    }

    bool claim(uint64_t id, int64_t now_ms, int64_t retry_ms)
    {
        std::lock_guard<std::mutex> g(m_);
        if (building_ || (cur_ && cur_id_ == id))
            return false;
        if (failed_ && failed_id_ == id && now_ms - failed_at_ < retry_ms)
            return false;
        building_ = true;
        return true;
    }

    void finish(uint64_t id, Tables tables, int64_t now_ms)
    {
        {
            std::lock_guard<std::mutex> g(m_);
            if (usable(tables)) {
                cur_ = std::make_shared<const Tables>(std::move(tables));
                cur_id_ = id;
                failed_ = false;
            } else {
                failed_ = true;
                failed_id_ = id;
                failed_at_ = now_ms;
            }
            building_ = false;
        }
        done_.notify_all();
    }

    void wait()
    {
        std::unique_lock<std::mutex> l(m_);
        done_.wait(l, [this] { return !building_; });
    }

    bool building() const
    {
        std::lock_guard<std::mutex> g(m_);
        return building_;
    }

private:
    mutable std::mutex m_;
    std::condition_variable done_;
    Ptr cur_;
    uint64_t cur_id_ = 0;
    bool building_ = false;
    bool failed_ = false;
    uint64_t failed_id_ = 0;
    int64_t failed_at_ = 0;
};

} // namespace pg::names_logic
