#pragma once

#include <atomic>
#include <cstdint>

// The counters and the timestamp clock the profiler's parts (perf.cpp and the files in perf/) share. A counter only its own
// thread writes is updated with a relaxed load and store (add, raise), one several threads write with a compare and
// swap (raise_shared).
namespace perf::detail {

using U64 = std::atomic<uint64_t>;
constexpr auto rlx = std::memory_order_relaxed;

inline uint64_t tsc() { return __builtin_ia32_rdtsc(); }

inline void add(U64 &a, uint64_t v) { a.store(a.load(rlx) + v, rlx); }

inline void raise(U64 &a, uint64_t v)
{
    if (v > a.load(rlx))
        a.store(v, rlx);
}

inline void raise_shared(U64 &a, uint64_t v)
{
    uint64_t was = a.load(rlx);
    while (v > was && !a.compare_exchange_weak(was, v, rlx))
        ;
}

// Milliseconds in `ticks` of the timestamp counter; 0 until the profiler has calibrated it.
double ms_of(uint64_t ticks);

}
