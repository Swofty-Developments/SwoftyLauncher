#include "heap_check.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

namespace hc = f1990::heap;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

struct Heap {
    alignas(16) uint8_t mem[0x4000]{};
    uint64_t guard = 0;
    uint8_t *newest = nullptr;
    hc::Range range{reinterpret_cast<uintptr_t>(mem), reinterpret_cast<uintptr_t>(mem) + sizeof mem};

    static void put(uint8_t *h, int i, int32_t v) { std::memcpy(h + 4 * i, &v, sizeof v); }

    static int32_t rel(const uint8_t *from, const uint8_t *to) { return to ? int32_t((to - from) / 16) : 0; }

    uint8_t *alloc(size_t at, uint32_t req)
    {
        uint8_t *h = mem + at;
        uint32_t size = uint32_t(hc::expected_size(req, guard));
        std::memcpy(h + 8, &size, 4);
        std::memcpy(h + 12, &req, 4);
        put(h, 0, rel(h, newest));
        put(h, 1, 0);
        if (newest)
            put(newest, 1, rel(newest, h));
        newest = h;
        return h;
    }

    void free(uint8_t *h)
    {
        uint8_t *older = hc::target(h, hc::link(h, 0)), *newer = hc::target(h, hc::link(h, 1));
        if (older)
            put(older, 1, rel(older, newer));
        if (newer)
            put(newer, 0, rel(newer, older));
        if (newest == h)
            newest = older;
    }

    bool consistent() const
    {
        size_t n = 0;
        for (uint8_t *h = newest; h; h = hc::target(h, hc::link(h, 0))) {
            uint8_t *older = hc::target(h, hc::link(h, 0));
            if (older && hc::target(older, hc::link(older, 1)) != h)
                return false;
            if (++n > 64)
                return false;
        }
        return true;
    }
};

static bool always(const uint8_t *, size_t)
{
    return true;
}

static void clean_list(uint64_t guard)
{
    Heap hp;
    hp.guard = guard;
    uint32_t total = uint32_t(hc::expected_size(0x460, guard));
    uint8_t *a = hp.alloc(0, 0x460), *b = hp.alloc(total, 0x460), *q = hp.alloc(2 * total, 0x460);
    CHECK(hc::allocated(a, guard) && hc::allocated(b, guard) && hc::allocated(q, guard));
    CHECK(hc::block_size(b) == total);
    for (uint8_t *h : {a, b, q})
        CHECK(hc::check(h, hp.range).ok());
    CHECK(hc::predecessor(q, hp.range, guard, 0x10000, always) == b);
    CHECK(hc::predecessor(b, hp.range, guard, 0x10000, always) == a);
    CHECK(hc::predecessor(a, hp.range, guard, 0x10000, always) == nullptr);
}

static void overrun_into_the_next_header_is_repaired()
{
    Heap hp;
    uint8_t *a = hp.alloc(0, 0x460), *b = hp.alloc(0x470, 0x460), *q = hp.alloc(0x8e0, 0x460),
            *r = hp.alloc(0xd50, 0x100);
    CHECK(hp.consistent());
    float spill = 1.0f;
    std::memcpy(b + hc::kHeader + 0x460, &spill, sizeof spill);
    CHECK(hc::link(q, 0) == 0x3f800000);
    hc::Check c = hc::check(q, hp.range);
    CHECK(!c.older_ok && c.newer_ok);
    uint8_t *pred = hc::predecessor(q, hp.range, 0, 0x10000, always);
    CHECK(pred == b);
    CHECK(hc::repair_older(q, pred));
    CHECK(hc::link(q, 0) == -0x47);
    CHECK(hc::check(q, hp.range).ok());
    hp.free(q);
    CHECK(hp.consistent());
    CHECK(hc::target(b, hc::link(b, 1)) == r && hc::target(r, hc::link(r, 0)) == b);
    hp.free(b);
    CHECK(hp.consistent());
    CHECK(hc::target(a, hc::link(a, 1)) == r);
}

static void unrepaired_overrun_breaks_the_list()
{
    Heap hp;
    hp.alloc(0, 0x460);
    uint8_t *b = hp.alloc(0x470, 0x460), *q = hp.alloc(0x8e0, 0x460);
    hp.alloc(0xd50, 0x100);
    int32_t wild = 0x7f;
    std::memcpy(q, &wild, sizeof wild);
    CHECK(hc::target(q, hc::link(q, 0)) != b);
    hp.free(q);
    CHECK(hc::target(b, hc::link(b, 1)) == q);
    CHECK(!hp.consistent() || hc::target(b, hc::link(b, 1)) == q);
}

static void no_repair_when_the_block_before_links_elsewhere()
{
    Heap hp;
    uint8_t *a = hp.alloc(0, 0x460);
    uint8_t *q = hp.alloc(0x8e0, 0x460);
    uint8_t *b = hp.alloc(0x470, 0x460);
    int32_t wild = 0x55;
    std::memcpy(q, &wild, sizeof wild);
    CHECK(!hc::check(q, hp.range).older_ok);
    uint8_t *pred = hc::predecessor(q, hp.range, 0, 0x10000, always);
    CHECK(pred == b);
    CHECK(!hc::repair_older(q, pred));
    CHECK(hc::link(q, 0) == 0x55);
    (void)a;
}

static void out_of_range_link_is_broken()
{
    Heap hp;
    uint8_t *a = hp.alloc(0, 0x460);
    hp.put(a, 1, 0x7fffffff);
    hc::Check c = hc::check(a, hp.range);
    CHECK(!c.newer_ok && c.older_ok);
    CHECK(!hc::Range{}.holds(a, 16));
}

static void free_blocks_are_not_predecessors()
{
    Heap hp;
    uint8_t *a = hp.alloc(0, 0x460);
    uint8_t *q = hp.alloc(0x470, 0x460);
    uint32_t free_size = 0x47;
    std::memcpy(a + 8, &free_size, 4);
    CHECK(hc::predecessor(q, hp.range, 0, 0x10000, always) == nullptr);
}

static void zeroed_links_of_a_neighbour_are_repaired()
{
    Heap hp;
    uint8_t *o = hp.alloc(0, 0x100), *a = hp.alloc(0x200, 0x460), *s = hp.alloc(0x800, 0x5b0),
            *t = hp.alloc(0xe00, 0x1a40 - 0x1000), *x = hp.alloc(0x680, 0x100);
    CHECK(hp.consistent());
    std::memset(s, 0, 8);
    hc::Check c = hc::check(a, hp.range);
    CHECK(c.older_ok && !c.newer_ok);
    CHECK(hc::sane(c.newer, hp.range, 0, always) && !hc::link_holds(c.newer, 0, hp.range, 0, always));
    hc::Neighbours nb = hc::repair_neighbours(a, c, hp.range, 0, always);
    CHECK(nb.newer == s && !nb.older);
    CHECK(hc::target(s, hc::link(s, 0)) == a);
    hc::Walk w = hc::repair_list(hp.newest, hp.range, 0, 64, 16, always);
    CHECK(w.complete && w.fixed == 1 && w.first_fixed == s && w.blocks == 5);
    CHECK(hc::target(s, hc::link(s, 1)) == t);
    CHECK(hp.consistent());
    hp.free(a);
    CHECK(hp.consistent());
    CHECK(hc::check(t, hp.range).ok() && hc::check(s, hp.range).ok());
    hp.free(s);
    CHECK(hp.consistent());
    CHECK(hc::target(o, hc::link(o, 1)) == t && hc::target(t, hc::link(t, 0)) == o);
    (void)x;
}

static void damaged_older_neighbour_is_linked_back()
{
    Heap hp;
    hp.alloc(0, 0x100);
    uint8_t *s = hp.alloc(0x800, 0x5b0), *t = hp.alloc(0xe00, 0x800);
    uint8_t *x = hp.alloc(0x200, 0x100);
    hp.put(s, 1, 0);
    hc::Check c = hc::check(t, hp.range);
    CHECK(!c.older_ok && c.newer_ok);
    hc::Neighbours nb = hc::repair_neighbours(t, c, hp.range, 0, always);
    CHECK(nb.older == s && !nb.newer);
    CHECK(hc::check(t, hp.range).ok());
    hp.free(t);
    CHECK(hp.consistent());
    CHECK(hc::target(s, hc::link(s, 1)) == x && hc::target(x, hc::link(x, 0)) == s);
}

static void walk_leaves_a_consistent_block_alone()
{
    Heap hp;
    uint8_t *z = hp.alloc(0, 0x100), *w = hp.alloc(0x200, 0x100), *q = hp.alloc(0x400, 0x100);
    uint8_t *n = hp.alloc(0x600, 0x100);
    hp.put(q, 0, Heap::rel(q, z));
    CHECK(hc::target(q, hc::link(q, 0)) == z);
    hc::Walk walk = hc::repair_list(hp.newest, hp.range, 0, 64, 16, always);
    CHECK(!walk.complete && walk.fixed == 0 && walk.stopped_at == q);
    CHECK(hc::target(z, hc::link(z, 1)) == w);
    hc::Check c = hc::check(q, hp.range);
    CHECK(!c.older_ok);
    hc::Neighbours nb = hc::repair_neighbours(q, c, hp.range, 0, always);
    CHECK(!nb.older && !nb.newer);
    CHECK(hc::target(z, hc::link(z, 1)) == w);
    (void)n;
}

static void walk_stops_at_a_wild_link_and_caps_its_fixes()
{
    Heap hp;
    uint8_t *a = hp.alloc(0, 0x100), *b = hp.alloc(0x200, 0x100), *q = hp.alloc(0x400, 0x100);
    hp.alloc(0x600, 0x100);
    hp.put(q, 0, 0x3f800000);
    hc::Walk walk = hc::repair_list(hp.newest, hp.range, 0, 64, 16, always);
    CHECK(!walk.complete && walk.stopped_at == q && walk.fixed == 0);
    hp.put(q, 0, Heap::rel(q, b));
    hp.put(a, 1, 0);
    hp.put(b, 1, 0);
    walk = hc::repair_list(hp.newest, hp.range, 0, 64, 1, always);
    CHECK(walk.fixed == 1 && walk.first_fixed == b && !walk.complete);
    CHECK(hc::link(a, 1) == 0);
    walk = hc::repair_list(hp.newest, hp.range, 0, 64, 16, always);
    CHECK(walk.complete && walk.fixed == 1 && walk.first_fixed == a);
    CHECK(hp.consistent());
}

int main()
{
    clean_list(0);
    clean_list(0x10);
    overrun_into_the_next_header_is_repaired();
    unrepaired_overrun_breaks_the_list();
    no_repair_when_the_block_before_links_elsewhere();
    out_of_range_link_is_broken();
    free_blocks_are_not_predecessors();
    zeroed_links_of_a_neighbour_are_repaired();
    damaged_older_neighbour_is_linked_back();
    walk_leaves_a_consistent_block_alone();
    walk_stops_at_a_wild_link_and_caps_its_fixes();
    if (g_fail) {
        std::printf("heap_check_test: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("heap_check_test: ok\n");
    return 0;
}
