#pragma once

#include <atomic>
#include <cstdint>
#include <mutex>

// The addon callbacks on ID3D12Device::CreateShaderResourceView (swofty/d3d12.hpp): a fixed table the hook reads
// without a lock. add and remove take the lock; a callback is published by storing its function last, and removed by
// clearing it, so the hook sees either the whole entry or none. A token names the slot and the generation it was
// added in, so a stale token never removes a later callback in the same slot.

namespace render::srv_hook {

constexpr uint32_t kMax = 16;

// The hook is wanted when the clamp is on or an addon registered a callback.
inline bool wanted(bool clamp_on, bool any_callback) { return clamp_on || any_callback; }

template <class Fn> class Callbacks {
public:
    // 0 when the table is full or fn is null; otherwise the token for remove.
    uint32_t add(Fn fn, void *user)
    {
        if (!fn)
            return 0;
        std::lock_guard<std::mutex> g(lock_);
        for (uint32_t i = 0; i < kMax; i++) {
            Entry &e = entries_[i];
            if (e.fn.load(std::memory_order_relaxed))
                continue;
            uint32_t gen = (e.gen + 1) & 0xffffffu;
            e.gen = gen ? gen : 1;
            e.user.store(user, std::memory_order_relaxed);
            e.fn.store(fn, std::memory_order_release);
            count_.fetch_add(1, std::memory_order_relaxed);
            return token(i, e.gen);
        }
        return 0;
    }

    // False for a token that names no live callback.
    bool remove(uint32_t t)
    {
        uint32_t i = (t & 0xffu) - 1, gen = t >> 8;
        if (!t || i >= kMax)
            return false;
        std::lock_guard<std::mutex> g(lock_);
        Entry &e = entries_[i];
        if (e.gen != gen || !e.fn.load(std::memory_order_relaxed))
            return false;
        e.fn.store(nullptr, std::memory_order_release);
        count_.fetch_sub(1, std::memory_order_relaxed);
        return true;
    }

    bool any() const { return count_.load(std::memory_order_relaxed) > 0; }

    // Calls call(fn, user) for every live callback, in slot order.
    template <class Call> void each(Call &&call) const
    {
        for (const Entry &e : entries_) {
            Fn fn = e.fn.load(std::memory_order_acquire);
            if (fn)
                call(fn, e.user.load(std::memory_order_relaxed));
        }
    }

private:
    static uint32_t token(uint32_t i, uint32_t gen) { return (gen << 8) | (i + 1); }

    struct Entry {
        std::atomic<Fn> fn{nullptr};
        std::atomic<void *> user{nullptr};
        uint32_t gen = 0;
    };
    Entry entries_[kMax];
    std::atomic<uint32_t> count_{0};
    std::mutex lock_;
};

}
