#pragma once
#include <windows.h>

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

namespace render::postfx {

// The resources the barrier hook watches, read without the lock. Writers hold the lock; readers only
// load. A key keeps its slot and erase only clears its bits, so lookups never walk over tombstones.
// The game names short-lived UI resources all the time, so once half the slots hold keys the writer
// copies the live keys into a new table and publishes it, which keeps probe runs short. A replaced
// table is freed a few seconds later, long after any reader that loaded it has returned.
class PtrSet {
    size_t mask;
    std::unique_ptr<std::atomic<uintptr_t>[]> key;
    std::unique_ptr<std::atomic<uint8_t>[]> on;
    size_t used = 0, live = 0;

    size_t hash(uintptr_t k) const { return size_t((k * 0x9E3779B97F4A7C15ull) >> 40) & mask; }

    size_t find(uintptr_t k, bool claim)
    {
        for (size_t i = hash(k), n = 0; n <= mask; i = (i + 1) & mask, n++) {
            uintptr_t v = key[i].load(std::memory_order_acquire);
            if (v == k)
                return i;
            if (v == 0) {
                if (!claim)
                    return mask + 1;
                key[i].store(k, std::memory_order_release);
                used++;
                return i;
            }
        }
        return mask + 1;
    }

public:
    explicit PtrSet(size_t n) : mask(n - 1), key(new std::atomic<uintptr_t>[n]()), on(new std::atomic<uint8_t>[n]()) {}

    uint8_t get(const void *p) const
    {
        uintptr_t k = uintptr_t(p);
        for (size_t i = hash(k), n = 0; n <= mask; i = (i + 1) & mask, n++) {
            uintptr_t v = key[i].load(std::memory_order_acquire);
            if (v == k)
                return on[i].load(std::memory_order_acquire);
            if (v == 0)
                return 0;
        }
        return 0;
    }
    size_t size() const { return mask + 1; }
    size_t count() const { return live; }
    bool half_full() const { return used * 2 >= mask + 1; }
    void set(const void *p, uint8_t bits)
    {
        size_t i = find(uintptr_t(p), bits != 0);
        if (i > mask)
            return;
        uint8_t was = on[i].load(std::memory_order_relaxed);
        live += (!was && bits) - (was && !bits);
        on[i].store(bits, std::memory_order_release);
    }
    template <class F> void each(F f) const
    {
        for (size_t i = 0; i <= mask; i++) {
            uintptr_t k = key[i].load(std::memory_order_relaxed);
            uint8_t b = on[i].load(std::memory_order_relaxed);
            if (k && b)
                f(reinterpret_cast<const void *>(k), b);
        }
    }
};

// Freed resources leave dead slots behind, so once half a table's slots are taken it is rebuilt from its
// live keys, at least four times as large as they are; a lookup for an unwatched resource, nearly every
// barrier, stays a probe or two.
template <size_t MIN> class WatchTable {
    std::atomic<PtrSet *> cur{new PtrSet(MIN)};
    std::vector<std::pair<PtrSet *, ULONGLONG>> retired;
    const char *what;
    size_t told = MIN;

public:
    explicit WatchTable(const char *name) : what(name) {}

    uint8_t get(const void *p) const { return cur.load(std::memory_order_acquire)->get(p); }

    void put(const void *p, uint8_t bits)
    {
        PtrSet *w = cur.load(std::memory_order_relaxed);
        if (!bits || !w->half_full()) {
            w->set(p, bits);
            return;
        }
        size_t n = MIN;
        while (n < (w->count() + 1) * 4)
            n *= 2;
        auto *fresh = new PtrSet(n);
        w->each([&](const void *k, uint8_t b) {
            if (k != p)
                fresh->set(k, b);
        });
        fresh->set(p, bits);
        if (n > told) {
            told = n;
            swofty::log("[postfx] the %s table grew to %zu slots for %zu live resource(s)", what, n, fresh->count());
        }
        cur.store(fresh, std::memory_order_release);
        ULONGLONG now = GetTickCount64();
        std::erase_if(retired, [&](const std::pair<PtrSet *, ULONGLONG> &r) {
            if (now - r.second < 5000)
                return false;
            delete r.first;
            return true;
        });
        retired.emplace_back(w, now);
    }
};

enum Watched : uint8_t { W_TONEMAP = 1, W_BACK = 2, W_HUDLESS = 4, W_UI = 8, W_DEPTH = 16 };

// The few resources the passes key on and the game's many UI images live in separate tables, so UI
// churn can never push the tonemap or the back buffers out.
class ResourceWatch {
    WatchTable<256> key{"key resource"};
    WatchTable<4096> ui{"UI"};

public:
    uint8_t bits(const void *p) const { return key.get(p) | ui.get(p); }

    void watch(const void *p, uint8_t bits)
    {
        key.put(p, uint8_t(bits & ~W_UI));
        ui.put(p, uint8_t(bits & W_UI));
    }
};

}
