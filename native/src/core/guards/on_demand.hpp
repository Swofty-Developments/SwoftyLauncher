#pragma once

#include <atomic>
#include <mutex>

namespace swofty_core::guards {

// A guard that hooks game code through safetyhook itself when its owner first arms it, outside the core mixin sets
// and the on-load entries. install() runs once, on the first arm(), however many threads arm the guard at once; every
// arm() returns what that run returned, after it has finished.
class OnDemandGuard {
public:
    constexpr OnDemandGuard() = default;
    OnDemandGuard(const OnDemandGuard &) = delete;
    OnDemandGuard &operator=(const OnDemandGuard &) = delete;

    bool arm()
    {
        std::call_once(once_, [this] { armed_.store(install(), std::memory_order_release); });
        return armed_.load(std::memory_order_acquire);
    }

    bool armed() const { return armed_.load(std::memory_order_acquire); }

protected:
    ~OnDemandGuard() = default;

    virtual bool install() = 0;

private:
    std::once_flag once_;
    std::atomic<bool> armed_{false};
};

}
