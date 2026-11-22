#include "core/runtime/protection_spans_logic.hpp"

#include <chrono>
#include <cstdio>
#include <random>
#include <vector>

namespace {

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what);
        failures++;
    }
}

struct Region {
    uintptr_t lo, hi;
    bool readable;
};

struct FakeMemory {
    std::vector<Region> regions;
    size_t queries = 0;

    bool query(uintptr_t p, uintptr_t &lo, uintptr_t &hi, bool &readable)
    {
        queries++;
        for (const Region &r : regions)
            if (p >= r.lo && p < r.hi) {
                lo = p & ~uintptr_t(0xfff);
                hi = r.hi;
                readable = r.readable;
                return true;
            }
        return false;
    }
};

bool fresh(FakeMemory &mem, uintptr_t p, size_t n)
{
    uintptr_t lo, hi;
    bool readable;
    if (!mem.query(p, lo, hi, readable))
        return false;
    return readable && p + n <= hi;
}

bool cached(FakeMemory &mem, runtime::ProtectionSpans &spans, uintptr_t p, size_t n)
{
    auto known = spans.lookup(p, n);
    if (known != runtime::ProtectionSpans::Answer::unknown)
        return known == runtime::ProtectionSpans::Answer::readable;
    uintptr_t lo, hi;
    bool readable;
    if (!mem.query(p, lo, hi, readable))
        return false;
    spans.remember(lo, hi, readable);
    return readable && p + n <= hi;
}

FakeMemory game_image()
{
    constexpr uintptr_t base = 0x140000000;
    FakeMemory mem;
    mem.regions = {
        {base, base + 0x1000, true},
        {base + 0x1000, base + 0x5380000, true},
        {base + 0x5380000, base + 0x5f6d000, true},
        {base + 0x5f6d000, base + 0x5f6e000, false},
        {base + 0x5f6e000, base + 0x6ceb000, true},
        {base + 0x6ceb000, base + 0x6faf000, true},
        {base + 0x6faf000, base + 0x1924d000, true},
    };
    return mem;
}

void same_answers_as_virtual_query()
{
    FakeMemory a = game_image(), b = game_image();
    runtime::ProtectionSpans spans;
    std::mt19937_64 rng(7);
    std::uniform_int_distribution<uintptr_t> addr(0x13fff0000, 0x15a000000);
    std::uniform_int_distribution<size_t> len(0, 64);
    size_t mismatches = 0;
    for (int i = 0; i < 200000; i++) {
        uintptr_t p = addr(rng);
        size_t n = len(rng);
        if (fresh(a, p, n) != cached(b, spans, p, n))
            mismatches++;
    }
    for (const Region &r : game_image().regions)
        for (uintptr_t p : {r.lo, r.hi - 1, r.hi - 16, r.lo + 0xfff})
            for (size_t n : {size_t(0), size_t(1), size_t(16), size_t(32)})
                if (fresh(a, p, n) != cached(b, spans, p, n))
                    mismatches++;
    check(mismatches == 0, "cached answers equal a fresh VirtualQuery for every address and length");
    check(spans.size() <= game_image().regions.size(), "one span per region");
}

void spans_never_cross_regions()
{
    runtime::ProtectionSpans spans;
    spans.remember(0x2000, 0x8000, true);
    check(spans.lookup(0x1fff, 1) == runtime::ProtectionSpans::Answer::unknown, "below a span is unknown");
    check(spans.lookup(0x8000, 1) == runtime::ProtectionSpans::Answer::unknown, "the span end is unknown");
    check(spans.lookup(0x7ff0, 16) == runtime::ProtectionSpans::Answer::readable, "bytes ending at the span end read");
    check(spans.lookup(0x7ff0, 17) == runtime::ProtectionSpans::Answer::refused, "bytes past the region end refuse");
    spans.remember(0x1000, 0x8000, true);
    check(spans.size() == 1 && spans.lookup(0x1000, 1) == runtime::ProtectionSpans::Answer::readable,
          "a lower query of the same region widens its span");
    spans.remember(0x8000, 0x9000, false);
    check(spans.lookup(0x8000, 1) == runtime::ProtectionSpans::Answer::refused, "an unreadable region refuses");
    spans.remember(0x9000, 0x9000, true);
    check(spans.size() == 2, "an empty region is not kept");
}

void capacity_bounds_the_table()
{
    runtime::ProtectionSpans spans;
    for (uintptr_t i = 0; i < runtime::ProtectionSpans::capacity * 2; i++)
        spans.remember(0x10000 * (i + 1), 0x10000 * (i + 1) + 0x1000, true);
    check(spans.size() == runtime::ProtectionSpans::capacity, "the table stops growing at its capacity");
    check(spans.lookup(0x10000 * (runtime::ProtectionSpans::capacity + 5), 1) == runtime::ProtectionSpans::Answer::unknown,
          "past capacity an address is asked again");
}

void binding_pass_queries()
{
    constexpr size_t bindings = 3656;
    FakeMemory before = game_image(), after = game_image();
    std::mt19937_64 rng(11);
    std::uniform_int_distribution<uintptr_t> code(0x140001000, 0x145380000 - 64);
    std::vector<uintptr_t> sites(bindings);
    for (auto &s : sites)
        s = code(rng);
    for (uintptr_t s : sites) {
        fresh(before, s, 32);
        fresh(before, s, 16);
    }
    runtime::ProtectionSpans spans;
    auto t0 = std::chrono::steady_clock::now();
    for (uintptr_t s : sites) {
        cached(after, spans, s, 32);
        cached(after, spans, s, 16);
    }
    double us = std::chrono::duration<double, std::micro>(std::chrono::steady_clock::now() - t0).count();
    std::printf("binding pass of %zu bindings, 2 checks each: VirtualQuery calls %zu -> %zu; cached checks %.1f us in "
                "total\n",
                bindings, before.queries, after.queries, us);
    check(after.queries * 100 < before.queries, "a binding pass asks VirtualQuery at most 1% as often");
}

}

int main()
{
    same_answers_as_virtual_query();
    spans_never_cross_regions();
    capacity_bounds_the_table();
    binding_pass_queries();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("protection spans: ok\n");
    return 0;
}
