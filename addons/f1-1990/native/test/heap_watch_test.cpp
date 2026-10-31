#include "heap_watch.hpp"

#include <cstdio>
#include <cstring>

namespace hw = f1990::watch;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

int main()
{
    CHECK(hw::dr7(0) == 0);
    CHECK(hw::dr7(1) == 0x00090101);
    CHECK(hw::dr7(2) == 0x00990105);
    CHECK(hw::dr7(4) == 0x99990155);

    uint64_t a[4] = {};
    CHECK(hw::parse("2FD8487A0,0x2FD848798 2fd848761", a, 4) == 3);
    CHECK(a[0] == 0x2FD8487A0 && a[1] == 0x2FD848798 && a[2] == 0x2FD848760);
    CHECK(hw::parse("", a, 4) == 0);
    CHECK(hw::parse("zz", a, 4) == 0);
    CHECK(hw::parse("1,2,3,4,5,6", a, 4) == 0);
    CHECK(hw::parse("10,20,30,40,50,60", a, 4) == 4 && a[3] == 0x40);

    CHECK(hw::in_allocator(0x140000000 + 0x3963f5f, 0x140000000));
    CHECK(!hw::in_allocator(0x140000000 + 0x446d509, 0x140000000));
    CHECK(!hw::in_allocator(0x140000000 + 0x3970000, 0x140000000));

    uint64_t s[4] = {1, 2, 3, 4}, n[4] = {1, 9, 3, 0};
    CHECK(hw::changed_mask(s, n, 4) == 0xa);
    CHECK(hw::changed_mask(s, s, 4) == 0);

    static hw::Ring ring;
    CHECK(ring.next() == nullptr);
    for (uint32_t i = 0; i < hw::kRing; i++) {
        hw::Hit *h = ring.begin();
        CHECK(h != nullptr);
        h->rip = i;
        hw::Ring::publish(h);
    }
    CHECK(ring.begin() == nullptr);
    CHECK(ring.dropped.load() == 1);
    for (uint32_t i = 0; i < hw::kRing; i++) {
        hw::Hit *h = ring.next();
        CHECK(h && h->rip == i);
        ring.pop();
    }
    CHECK(ring.next() == nullptr);
    hw::Hit *h = ring.begin();
    CHECK(h != nullptr);
    CHECK(ring.next() == nullptr);
    hw::Ring::publish(h);
    CHECK(ring.next() == h);
    ring.pop();

    hw::Hit hit;
    hit.stack_words = 5;
    hit.stack[0] = 0x7000;
    hit.stack[1] = 0x140000000 + 0x3963f5f;
    hit.stack[2] = 0x140000000 + 0x20;
    hit.stack[3] = 0x140000000 + 0x14e13c05;
    hit.stack[4] = 0x146000000 + 0x1;
    char buf[256];
    hw::exe_stack(hit, 0x140000000, 0x160000000, buf, sizeof buf, 8);
    CHECK(std::strcmp(buf, "exe+0x3963f5f exe+0x14e13c05 exe+0x6000001") == 0);
    hw::exe_stack(hit, 0x140000000, 0x160000000, buf, sizeof buf, 1);
    CHECK(std::strcmp(buf, "exe+0x3963f5f") == 0);

    if (g_fail)
        return 1;
    std::printf("heap_watch_test ok\n");
}
