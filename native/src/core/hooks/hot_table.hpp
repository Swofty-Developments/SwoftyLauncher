#pragma once
#include <atomic>
#include <cstdint>
#include <functional>
#include <mutex>
#include <shared_mutex>
#include <string>
#include <string_view>
#include <unordered_map>

namespace swofty_core::hooks {

struct HotKeyHash {
    using is_transparent = void;
    size_t operator()(std::string_view s) const noexcept { return std::hash<std::string_view>{}(s); }
};

struct HotKeyEq {
    using is_transparent = void;
    bool operator()(std::string_view a, std::string_view b) const noexcept { return a == b; }
};

struct HotStats {
    size_t entries = 0, supplied = 0;
    uint64_t epoch = 0;
};

template <class V> class HotTable {
public:
    template <class Build>
    V resolve(std::string_view key, uint64_t epoch, Build &&build, std::function<void(const HotStats &)> reset = {})
    {
        {
            std::shared_lock<std::shared_mutex> g(lock_);
            if (epoch == epoch_) {
                auto it = map_.find(key);
                if (it != map_.end())
                    return it->second.value;
            }
        }
        std::pair<V, bool> built = build();
        HotStats old{};
        bool dropped = false;
        {
            std::unique_lock<std::shared_mutex> g(lock_);
            if (epoch != epoch_) {
                old = {map_.size(), supplied_, epoch_};
                dropped = !map_.empty();
                map_.clear();
                supplied_ = 0;
                epoch_ = epoch;
            }
            auto [it, fresh] = map_.try_emplace(std::string(key), Entry{built.first, built.second});
            if (fresh && built.second)
                supplied_++;
            built.first = it->second.value;
        }
        if (dropped && reset)
            reset(old);
        return built.first;
    }

    HotStats stats() const
    {
        std::shared_lock<std::shared_mutex> g(lock_);
        return {map_.size(), supplied_, epoch_};
    }

    void clear()
    {
        std::unique_lock<std::shared_mutex> g(lock_);
        map_.clear();
        supplied_ = 0;
        epoch_ = ~uint64_t(0);
    }

private:
    struct Entry {
        V value;
        bool supplied;
    };
    mutable std::shared_mutex lock_;
    std::unordered_map<std::string, Entry, HotKeyHash, HotKeyEq> map_;
    size_t supplied_ = 0;
    uint64_t epoch_ = ~uint64_t(0);
};

}
