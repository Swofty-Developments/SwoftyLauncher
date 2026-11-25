// Host-native unit test for the model part pool log (native/src/core/diagnostics/part_pool_logic.hpp): the allocator
// layout checks, the set-bit count over a bitmap read in chunks, the reads that fail, and the log line.
// Built and run by native/test/run_tests.sh part_pool.

#include "core/diagnostics/part_pool_logic.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

namespace pp = runtime::part_pool;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

namespace {

struct Fake {
    std::vector<uint8_t> bitmap;
    alignas(8) uint8_t object[0x80] = {};
    uint64_t fail_at = 0;

    explicit Fake(size_t bytes) : bitmap(bytes) { set_ptrs(); }

    void set_ptrs()
    {
        uint64_t b = uint64_t(reinterpret_cast<uintptr_t>(bitmap.data()));
        uint64_t e = b + bitmap.size();
        std::memcpy(object + pp::kBegin, &b, 8);
        std::memcpy(object + pp::kEnd, &e, 8);
    }
    uint64_t addr() const { return uint64_t(reinterpret_cast<uintptr_t>(object)); }
    void field32(uint64_t off, uint32_t v) { std::memcpy(object + off, &v, 4); }
};

Fake *current;

bool read(uint64_t at, void *out, size_t n)
{
    if (current->fail_at && at == current->fail_at)
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(uintptr_t(at)), n);
    return true;
}

}

int main()
{
    CHECK(pp::popcount(nullptr, 0) == 0);
    uint8_t b[3] = {0xff, 0x01, 0x80};
    CHECK(pp::popcount(b, 3) == 10);

    Fake f(0x2e00);
    current = &f;
    for (size_t i = 0; i < 1000; i++)
        f.bitmap[i / 8] |= uint8_t(1u << (i % 8));
    f.field32(pp::kCursor, 1000);
    f.field32(pp::kHigh, 999);
    pp::Reading r = pp::capture(read, f.addr());
    CHECK(r.verdict == pp::Verdict::ok);
    CHECK(r.capacity == 94208);
    CHECK(r.used == 1000);
    CHECK(r.high == 999);
    CHECK(!r.aborted);
    char text[256];
    pp::line(r, "StateLoadSession", text, sizeof text);
    CHECK(!std::strcmp(text, "[render] model part pool: 1000 of 94208 slots used (high-water 999) at StateLoadSession"));

    std::memset(f.bitmap.data(), 0xff, f.bitmap.size());
    f.field32(pp::kHigh, 0x16fff);
    f.object[pp::kAbort] = 1;
    r = pp::capture(read, f.addr());
    CHECK(r.verdict == pp::Verdict::ok && r.used == 94208 && r.high == 0x16fff && r.aborted);
    pp::line(r, "x", text, sizeof text);
    CHECK(std::strstr(text, "94208 of 94208") && std::strstr(text, "abort flag set"));

    f.object[pp::kAbort] = 0;
    f.fail_at = uint64_t(reinterpret_cast<uintptr_t>(f.bitmap.data())) + 512;
    r = pp::capture(read, f.addr());
    CHECK(r.verdict == pp::Verdict::unread);
    pp::line(r, "x", text, sizeof text);
    CHECK(std::strstr(text, "is not readable"));
    f.fail_at = 0;

    f.field32(pp::kHigh, 94208);
    r = pp::capture(read, f.addr());
    CHECK(r.verdict == pp::Verdict::shape);
    pp::line(r, "x", text, sizeof text);
    CHECK(std::strstr(text, "does not look like the allocator"));
    f.field32(pp::kHigh, 5);

    f.field32(pp::kCursor, 94208);
    CHECK(pp::capture(read, f.addr()).verdict == pp::Verdict::shape);
    f.field32(pp::kCursor, 0);

    uint64_t zero = 0;
    std::memcpy(f.object + pp::kBegin, &zero, 8);
    CHECK(pp::capture(read, f.addr()).verdict == pp::Verdict::shape);
    f.set_ptrs();
    CHECK(pp::capture(read, f.addr()).verdict == pp::Verdict::ok);

    Fake big(size_t(pp::kMaxBytes) + 1);
    current = &big;
    CHECK(pp::capture(read, big.addr()).verdict == pp::Verdict::shape);

    pp::Reading none = pp::capture(read, 0);
    CHECK(none.verdict == pp::Verdict::unread);
    pp::line(none, "x", text, sizeof text);
    CHECK(std::strstr(text, "not seen yet"));

    CHECK(!pp::shaped(100, 100, 0, 0));
    CHECK(pp::shaped(100, 101, 7, 7));
    CHECK(!pp::shaped(100, 101, 8, 0));

    if (g_fail)
        return 1;
    std::printf("part_pool: ok\n");
    return 0;
}
