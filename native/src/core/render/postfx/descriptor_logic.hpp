#pragma once
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <type_traits>

namespace render::postfx {

// A view a slot wrote into one entry of its descriptor tables: the resource and its format.
template <class R, class F> struct Written {
    R *r = nullptr;
    F f{};
    bool set = false;
};

// Whether the entry holding v must be written again for r in format f, noting it as written when it
// must: a frame whose views are the ones its slot wrote last time keeps them.
template <class R, class F> bool rewrite(Written<R, F> &v, R *r, F f)
{
    if (v.set && v.r == r && v.f == f)
        return false;
    v = {r, f, true};
    return true;
}

// The depth and UI barriers one list moved since the locked path last looked at it, written without a
// lock by the thread recording the list.
template <class L, class R, class S> struct SideMark {
    std::atomic<L *> list{nullptr};
    std::atomic<R *> depth{nullptr}, bound{nullptr};
    std::atomic<S> depth_state{};
    std::atomic<bool> ui{false}, marked{false};
};

// The lists' side marks. A list keeps its entry once it has one; a list that finds no room makes the
// table full for good, and from then on every batch goes through the lock.
template <class L, class R, class S, size_t N = 1024, size_t PROBES = 32> class SideMarks {
    SideMark<L, R, S> at_[N];
    std::atomic<bool> full_{false};

public:
    SideMark<L, R, S> *find(L *list, bool add)
    {
        size_t h = size_t((uint64_t(uintptr_t(list)) >> 4) * 0x9E3779B97F4A7C15ull >> 32);
        for (size_t i = 0; i < PROBES && i < N; i++) {
            SideMark<L, R, S> &s = at_[(h + i) % N];
            L *k = s.list.load(std::memory_order_acquire);
            if (k == list)
                return &s;
            if (k)
                continue;
            if (!add)
                return nullptr;
            if (s.list.compare_exchange_strong(k, list, std::memory_order_acq_rel) || k == list)
                return &s;
        }
        if (add)
            full_.store(true, std::memory_order_relaxed);
        return nullptr;
    }

    bool full() const { return full_.load(std::memory_order_relaxed); }
};

// Notes one barrier of a batch in a list's side mark the way the locked path notes it in the list's
// Mark: a depth barrier always, a UI barrier once the list has a Mark or a depth barrier.
template <class L, class R, class S> void note(SideMark<L, R, S> &s, bool &marked, bool depth, bool ui, R *r, S after)
{
    if (depth) {
        s.depth_state.store(after, std::memory_order_relaxed);
        s.depth.store(r, std::memory_order_relaxed);
        marked = true;
    }
    if (ui && marked)
        s.ui.store(true, std::memory_order_relaxed);
}

// Makes at point to r with a reference of its own on r and lets go of the one it held, so a resource
// the game releases stays alive while the core still points at it.
template <class T> void hold(T *&at, std::type_identity_t<T> *r)
{
    if (at == r)
        return;
    if (r)
        r->AddRef();
    if (at)
        at->Release();
    at = r;
}

}
