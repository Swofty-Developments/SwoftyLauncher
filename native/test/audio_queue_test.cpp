// Host-native unit test for the core's audio job queue fix (native/src/core/audio_queue_logic.hpp): the queue
// layout, moving stranded requests from slots 12-23 into free slots 0-11, a model of the stock workers' drain
// that strands a request and of the fixed drain that runs every one, and the worker thunk registry.
// Built and run by native/test/run_audio_queue_test.sh.

#include "audio_queue_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace aq = runtime::audio_queue;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

struct Queue {
    alignas(16) uint8_t table[aq::kTableBytes]{};
    int pending = 0;

    bool flag(size_t i) const { return table[i * aq::kStride + aq::kFlag] != 0; }

    void *fn(size_t i) const
    {
        void *p;
        std::memcpy(&p, table + i * aq::kStride + aq::kFn, sizeof p);
        return p;
    }

    uint32_t type(size_t i) const
    {
        uint32_t t;
        std::memcpy(&t, table + i * aq::kStride + aq::kType, sizeof t);
        return t;
    }

    bool enqueue(void *f, uint32_t t)
    {
        for (size_t i = 0; i < aq::kSlots; i++) {
            uint8_t *s = aq::slot(table, i);
            if (s[aq::kFlag])
                continue;
            std::memcpy(s + aq::kFn, &f, sizeof f);
            std::memcpy(s + aq::kType, &t, sizeof t);
            s[aq::kFlag] = 1;
            pending++;
            return true;
        }
        return false;
    }

    bool take(void **f, uint32_t *t)
    {
        for (size_t i = 0; i < aq::kDrained; i++) {
            if (!flag(i))
                continue;
            *f = fn(i);
            *t = type(i);
            table[i * aq::kStride + aq::kFlag] = 0;
            pending--;
            return true;
        }
        return false;
    }
};

static void *tag(uintptr_t v)
{
    return reinterpret_cast<void *>(v);
}

static size_t run(Queue &q, bool fix, std::vector<uint32_t> &ran)
{
    size_t spins = 0;
    while (q.pending) {
        void *f;
        uint32_t t;
        if (!q.take(&f, &t)) {
            if (++spins > 1000)
                break;
            continue;
        }
        ran.push_back(t);
        if (fix)
            aq::move_stranded(q.table);
    }
    return spins;
}

static void layout()
{
    CHECK(aq::kTableBytes == 0x480);
    CHECK(aq::kDrained * aq::kStride == 0x240);
    CHECK(aq::kFlag + 1 <= aq::kStride && aq::kType + 4 <= aq::kFlag && aq::kFn + 8 <= aq::kType);
}

static void nothing_to_move()
{
    Queue q;
    CHECK(!aq::stranded(q.table));
    CHECK(aq::move_stranded(q.table) == 0);
    for (uint32_t i = 0; i < 5; i++)
        q.enqueue(tag(0x1000), i);
    CHECK(!aq::stranded(q.table));
    CHECK(aq::move_stranded(q.table) == 0);
    for (size_t i = 0; i < 5; i++)
        CHECK(q.flag(i) && q.type(i) == i);
}

static void full_low_slots_wait()
{
    Queue q;
    for (uint32_t i = 0; i < 13; i++)
        q.enqueue(tag(0x1000 + i), 100 + i);
    CHECK(q.flag(12) && aq::stranded(q.table));
    CHECK(aq::move_stranded(q.table) == 0);
    CHECK(q.flag(12) && q.pending == 13);
    void *f;
    uint32_t t;
    CHECK(q.take(&f, &t) && t == 100 && f == tag(0x1000));
    CHECK(aq::move_stranded(q.table) == 1);
    CHECK(!aq::stranded(q.table) && !q.flag(12));
    CHECK(q.flag(0) && q.fn(0) == tag(0x100c) && q.type(0) == 112);
    CHECK(q.pending == 12);
}

static void moves_in_order_into_free_slots()
{
    Queue q;
    for (uint32_t i = 0; i < 16; i++)
        q.enqueue(tag(0x2000 + i), i);
    uint8_t *s = q.table;
    s[3 * aq::kStride + aq::kFlag] = 0;
    s[7 * aq::kStride + aq::kFlag] = 0;
    q.pending -= 2;
    CHECK(aq::move_stranded(q.table) == 2);
    CHECK(q.fn(3) == tag(0x200c) && q.type(3) == 12);
    CHECK(q.fn(7) == tag(0x200d) && q.type(7) == 13);
    CHECK(!q.flag(12) && !q.flag(13) && q.flag(14) && q.flag(15));
    int set = 0;
    for (size_t i = 0; i < aq::kSlots; i++)
        set += q.flag(i);
    CHECK(set == q.pending && q.pending == 14);
}

static void stock_drain_strands_a_request()
{
    Queue q;
    for (uint32_t i = 0; i < 13; i++)
        q.enqueue(tag(0x3000), i);
    std::vector<uint32_t> ran;
    size_t spins = run(q, false, ran);
    CHECK(ran.size() == 12 && q.pending == 1 && spins > 1000);
    CHECK(q.flag(12) && q.type(12) == 12);
}

static void fixed_drain_runs_every_request()
{
    for (uint32_t burst : {13u, 18u, 24u}) {
        Queue q;
        for (uint32_t i = 0; i < burst; i++)
            CHECK(q.enqueue(tag(0x4000), i));
        std::vector<uint32_t> ran;
        size_t spins = run(q, true, ran);
        CHECK(spins == 0 && q.pending == 0 && ran.size() == burst);
        std::vector<int> seen(burst, 0);
        for (uint32_t t : ran)
            if (t < burst)
                seen[t]++;
        for (uint32_t i = 0; i < burst; i++)
            CHECK(seen[i] == 1);
        CHECK(!aq::stranded(q.table));
    }
}

static void requests_arriving_while_draining()
{
    Queue q;
    std::vector<uint32_t> ran;
    uint32_t next = 0;
    for (int round = 0; round < 50; round++) {
        for (int k = 0; k < 7; k++)
            q.enqueue(tag(0x5000), next++);
        void *f;
        uint32_t t;
        for (int k = 0; k < 4 && q.take(&f, &t); k++) {
            ran.push_back(t);
            aq::move_stranded(q.table);
        }
    }
    run(q, true, ran);
    CHECK(q.pending == 0 && !aq::stranded(q.table));
    CHECK(ran.size() + 0 <= next);
    std::vector<int> seen(next, 0);
    for (uint32_t t : ran)
        seen[t]++;
    size_t once = 0;
    for (uint32_t i = 0; i < next; i++) {
        CHECK(seen[i] <= 1);
        once += seen[i] == 1;
    }
    CHECK(once == ran.size());
}

static void workers_registry()
{
    aq::Workers<3> w;
    int a, b, c, d;
    CHECK(w.index(&a) == 0);
    CHECK(w.index(&b) == 1);
    CHECK(w.index(&a) == 0);
    CHECK(w.index(&c) == 2);
    CHECK(w.index(&d) == 3);
    CHECK(w.index(&b) == 1);
    CHECK(w.at(0) == &a && w.at(1) == &b && w.at(2) == &c);
}

int main()
{
    layout();
    nothing_to_move();
    full_low_slots_wait();
    moves_in_order_into_free_slots();
    stock_drain_strands_a_request();
    fixed_drain_runs_every_request();
    requests_arriving_while_draining();
    workers_registry();
    if (g_fail) {
        std::printf("audio_queue_test: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("audio_queue_test: ok\n");
    return 0;
}
