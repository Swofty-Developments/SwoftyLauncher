// Host-native unit test for the load-stall report's front end boot route section
// (native/src/core/diagnostics/boot_route_logic.hpp): the signature against the update's bytes from the KB build and
// Steam 1.0.145, the record and flag the update's two rip-relative operands give, StateFTUE's verdict for every
// branch of the update, the route name, the job's status words and listeners, an unproven build, unreadable memory,
// the raw rows and a buffer too small for the text. Built and run by native/test/run_tests.sh boot_route.

#include "core/diagnostics/boot_route_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace br = runtime::boot_route;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

struct Mem {
    std::map<uint64_t, std::vector<uint8_t>> regions;

    void map(uint64_t a, size_t n) { regions[a] = std::vector<uint8_t>(n); }

    uint8_t *at(uint64_t a, size_t n)
    {
        auto it = regions.upper_bound(a);
        if (it == regions.begin())
            return nullptr;
        --it;
        if (a < it->first || a + n > it->first + it->second.size())
            return nullptr;
        return it->second.data() + (a - it->first);
    }

    template <class T> void put(uint64_t a, T v)
    {
        uint8_t *p = at(a, sizeof v);
        if (!p) {
            std::printf("FAIL test setup: put outside memory at 0x%llx\n", (unsigned long long)a);
            g_fail++;
            return;
        }
        std::memcpy(p, &v, sizeof v);
    }

    void bytes(uint64_t a, const void *b, size_t n)
    {
        uint8_t *p = at(a, n);
        if (!p) {
            std::printf("FAIL test setup: bytes outside memory at 0x%llx\n", (unsigned long long)a);
            g_fail++;
            return;
        }
        std::memcpy(p, b, n);
    }

    bool read(uint64_t a, void *out, size_t n)
    {
        uint8_t *p = at(a, n);
        if (!p)
            return false;
        std::memcpy(out, p, n);
        return true;
    }

    auto reader()
    {
        return [this](uint64_t a, void *out, size_t n) { return read(a, out, n); };
    }
};

static const uint8_t KB_UPDATE[] = {
    0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x8b, 0x05, 0x92, 0xa1, 0x5d, 0x03, 0x89, 0x02, 0x80, 0x3d, 0x11,
    0x9e, 0xa2, 0x02, 0x00, 0x74, 0x42, 0x48, 0x8b, 0x81, 0xf8, 0x0b, 0x00, 0x00, 0x48, 0x8b, 0x50, 0x18, 0x4c, 0x8d,
    0x40, 0x30, 0x48, 0x39, 0x50, 0x20, 0x4c, 0x0f, 0x45, 0xc2, 0x41, 0x83, 0x78, 0x10, 0x0e, 0x75, 0x09, 0x48, 0x8d,
    0x05, 0x76, 0xa1, 0x5d, 0x03, 0xeb, 0x0e, 0x31, 0xd2, 0xe8, 0xb9, 0xeb, 0xd4, 0x00, 0x48, 0x8d, 0x05, 0x5a, 0xa1,
    0x5d, 0x03, 0x8b, 0x00, 0x89, 0x06, 0x48, 0x89, 0xf0, 0x48, 0x83, 0xc4, 0x20, 0x5e, 0xc3, 0x48, 0x8b, 0x05, 0x26,
    0x9d, 0xa2, 0x02, 0x48, 0x85, 0xc0, 0x74, 0xb2, 0x83, 0x78, 0x48, 0x00, 0x75, 0xe5, 0xeb, 0xaa,
};

static const uint8_t STEAM_145_UPDATE[] = {
    0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x8b, 0x05, 0x02, 0x9c, 0x5d, 0x03, 0x89, 0x02, 0x80, 0x3d, 0x81,
    0x98, 0xa2, 0x02, 0x00, 0x74, 0x42, 0x48, 0x8b, 0x81, 0xf8, 0x0b, 0x00, 0x00, 0x48, 0x8b, 0x50, 0x18, 0x4c, 0x8d,
    0x40, 0x30, 0x48, 0x39, 0x50, 0x20, 0x4c, 0x0f, 0x45, 0xc2, 0x41, 0x83, 0x78, 0x10, 0x0e, 0x75, 0x09, 0x48, 0x8d,
    0x05, 0xe6, 0x9b, 0x5d, 0x03, 0xeb, 0x0e, 0x31, 0xd2, 0xe8, 0xf9, 0xeb, 0xd4, 0x00, 0x48, 0x8d, 0x05, 0xca, 0x9b,
    0x5d, 0x03, 0x8b, 0x00, 0x89, 0x06, 0x48, 0x89, 0xf0, 0x48, 0x83, 0xc4, 0x20, 0x5e, 0xc3, 0x48, 0x8b, 0x05, 0x96,
    0x97, 0xa2, 0x02, 0x48, 0x85, 0xc0, 0x74, 0xb2, 0x83, 0x78, 0x48, 0x00, 0x75, 0xe5, 0xeb, 0xaa,
};

static bool matches(const char *pattern, const uint8_t *b, size_t n)
{
    size_t i = 0;
    for (const char *p = pattern; *p;) {
        if (*p == ' ') {
            p++;
            continue;
        }
        if (i >= n)
            return false;
        if (p[0] == '?' && p[1] == '?') {
            p += 2, i++;
            continue;
        }
        unsigned v = 0;
        if (std::sscanf(p, "%2x", &v) != 1 || b[i] != v)
            return false;
        p += 2, i++;
    }
    return i == n;
}

static std::string text_of(const br::Snapshot &s, br::Namer name = nullptr, size_t cap = 1 << 16)
{
    std::vector<char> b(cap);
    size_t n = br::text(s, b.data(), b.size(), name);
    return std::string(b.data(), n);
}

static void namer(char *out, size_t cap, uint64_t at)
{
    std::snprintf(out, cap, "F1_25.exe+0x%llx", (unsigned long long)(at - 0x140000000));
}

static bool has(const std::string &t, const char *s) { return t.find(s) != std::string::npos; }

static void test_signature_and_operands()
{
    CHECK(matches(br::kPattern, KB_UPDATE, sizeof KB_UPDATE));
    CHECK(matches(br::kPattern, STEAM_145_UPDATE, sizeof STEAM_145_UPDATE));
    uint8_t other[sizeof KB_UPDATE];
    std::memcpy(other, KB_UPDATE, sizeof other);
    other[0x6a] = 0x50;
    CHECK(!matches(br::kPattern, other, sizeof other));
    CHECK(KB_UPDATE[br::kFlagInsn] == 0x80 && KB_UPDATE[br::kFlagInsn + 1] == 0x3d);
    CHECK(KB_UPDATE[br::kJobInsn] == 0x48 && KB_UPDATE[br::kJobInsn + 1] == 0x8b && KB_UPDATE[br::kJobInsn + 2] == 0x05);
    CHECK(KB_UPDATE[0x69] == 0x48 && KB_UPDATE[0x6a] == 0x00);

    struct Case {
        const uint8_t *b;
        size_t n;
        uint64_t fn, record, flag;
    } cases[] = {{KB_UPDATE, sizeof KB_UPDATE, 0x1436d80e0, 0x146101e60, 0x146101f08},
                 {STEAM_145_UPDATE, sizeof STEAM_145_UPDATE, 0x1436da670, 0x146103e60, 0x146103f08}};
    for (const Case &c : cases) {
        Mem m;
        m.map(c.fn, 0x100);
        m.bytes(c.fn, c.b, c.n);
        br::Where w;
        CHECK(br::locate(m.reader(), c.fn, w));
        CHECK(w.fn == c.fn);
        CHECK(w.record == c.record);
        CHECK(w.flag == c.flag);
        CHECK(w.flag == w.record + br::kFlag);
    }

    Mem m;
    m.map(0x1000, 0x100);
    m.put<int32_t>(0x1000 + br::kFlagDisp, -0x40);
    m.put<int32_t>(0x1000 + br::kJobDisp, -0x20);
    br::Where w;
    CHECK(br::locate(m.reader(), 0x1000, w));
    CHECK(w.flag == 0x1000 + br::kFlagInsn + br::kFlagLen - 0x40);
    CHECK(w.record == 0x1000 + br::kJobInsn + br::kJobLen - 0x20 - br::kRecordJob);

    Mem none;
    CHECK(!br::locate(none.reader(), 0x1000, w));
    none.map(0x1000, 0x20);
    CHECK(!br::locate(none.reader(), 0x1000, w));
}

struct Game {
    Mem m;
    static constexpr uint64_t base = 0x140000000, record = 0x146103e60, job = 0x20000000, list = 0x21000000, vtable = 0x14587dec0;
    br::Snapshot s;

    Game()
    {
        m.map(record, 0x100);
        m.map(job, 0x400);
        m.map(list, 0x1000);
        m.put<uint64_t>(record, 0x14587dec0);
        m.put<uint64_t>(record + br::kRecordJob, job);
        m.put<uint64_t>(record + br::kNameCap, 0x80);
        m.put<uint64_t>(record + br::kName, record + br::kNameBuf);
        m.bytes(record + br::kNameBuf, "enter_boot", 11);
        m.put<uint8_t>(record + br::kFlag, 0);
        m.put<uint64_t>(job, vtable);
        m.put<uint32_t>(job + br::kStatus, 2);
        m.put<uint32_t>(job + br::kSubStatus, 0);
        listeners(3);
        s.base = base;
        s.at = {0x1436da670, record, record + br::kFlag};
        s.proven = true;
    }

    void listeners(size_t n)
    {
        for (size_t i = 0; i < n; i++) {
            m.put<uint64_t>(list + i * br::kListener, 0x143000000 + i * 0x10);
            m.put<uint64_t>(list + i * br::kListener + 8, 0x30000000 + i);
        }
        m.put<uint64_t>(job + br::kListeners, list);
        m.put<uint64_t>(job + br::kListenersEnd, list + n * br::kListener);
    }

    br::Snapshot &run()
    {
        br::capture(m.reader(), s);
        return s;
    }
};

static void test_pending_route()
{
    Game g;
    br::Snapshot &s = g.run();
    CHECK(s.job_read && s.job == Game::job);
    CHECK(s.flag_read && s.flag == 0);
    CHECK(s.status_read && s.status == 2 && s.sub == 0);
    CHECK(s.vtable_read && s.vtable == Game::vtable);
    CHECK(s.name_read && !std::strcmp(s.name, "enter_boot"));
    CHECK(s.name_cap == 0x80);
    CHECK(s.listeners_read && s.listeners_total == 3 && s.listeners_shown == 3);
    CHECK(s.listener_fn[2] == 0x143000020 && s.listener_arg[2] == 0x30000002);
    CHECK(br::verdict(s) == br::Stays);
    std::string t = text_of(s, namer);
    CHECK(has(t, "== Front end boot route"));
    CHECK(has(t, "FeFlow_StateFTUE_OnUpdate base+0x36da670 reads the route record at base+0x6103e60 and its flag at "
                 "base+0x6103f08\n"));
    CHECK(!has(t, "not at the record's +0xa8"));
    CHECK(has(t, "  route 'enter_boot' (record +0x20 0x146103e88, capacity 128)\n"));
    CHECK(has(t, "  +0x8    job 0x20000000, vtable 0x14587dec0 (F1_25.exe+0x587dec0)\n"));
    CHECK(has(t, "  job +0x48 status 2, +0x4c 0\n"));
    CHECK(has(t, "  +0xa8   flag 0\n"));
    CHECK(has(t, "  StateFTUE stays in Updating while the job's +0x48 is not 0 and the record's +0xa8 is 0\n"));
    CHECK(has(t, "  job +0x100 listeners: 3\n"));
    CHECK(has(t, "    #1 0x143000010 (F1_25.exe+0x3000010), argument 0x30000001\n"));
    CHECK(has(t, "  record bytes +0x00..+0xd0 at 0x146103e60:\n"));
    CHECK(has(t, "    +000  000000014587dec0 0000000020000000\n"));
    CHECK(has(t, "  job bytes +0x000..+0x200 at 0x20000000:\n"));
    CHECK(has(t, "    +040  0000000000000000 0000000000000002\n"));
}

static void test_verdicts()
{
    {
        Game g;
        g.m.put<uint8_t>(Game::record + br::kFlag, 1);
        br::Snapshot &s = g.run();
        CHECK(br::verdict(s) == br::Leaves);
        CHECK(has(text_of(s), "the record's +0xa8 is set: StateFTUE leaves on its next update"));
        CHECK(has(text_of(s), "  +0xa8   flag 1\n"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::record + br::kRecordJob, 0);
        br::Snapshot &s = g.run();
        CHECK(br::verdict(s) == br::LeavesNoJob);
        std::string t = text_of(s);
        CHECK(has(t, "  +0x8    job none\n"));
        CHECK(has(t, "the record has no job: StateFTUE leaves on its next update"));
        CHECK(!has(t, "job bytes"));
        CHECK(!has(t, "listeners"));
    }
    {
        Game g;
        g.m.put<uint32_t>(Game::job + br::kStatus, 0);
        br::Snapshot &s = g.run();
        CHECK(br::verdict(s) == br::LeavesIdle);
        CHECK(has(text_of(s), "the job's +0x48 is 0: StateFTUE leaves on its next update"));
    }
    {
        Game g;
        g.m.put<uint32_t>(Game::job + br::kStatus, 3);
        g.m.put<uint32_t>(Game::job + br::kSubStatus, 2);
        br::Snapshot &s = g.run();
        CHECK(br::verdict(s) == br::Stays);
        CHECK(has(text_of(s), "  job +0x48 status 3, +0x4c 2\n"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::record + br::kRecordJob, 0x50000000);
        br::Snapshot &s = g.run();
        CHECK(s.job_read && !s.status_read);
        CHECK(br::verdict(s) == br::Unread);
        std::string t = text_of(s);
        CHECK(has(t, "  job +0x48 not readable\n"));
        CHECK(has(t, "  not readable\n"));
        CHECK(has(t, "  job +0x100 listeners not readable\n"));
        CHECK(has(t, "    +000  not readable\n"));
    }
    {
        Game g;
        g.s.at.record = 0x60000000;
        g.s.at.flag = 0x60000000 + br::kFlag;
        br::Snapshot &s = g.run();
        CHECK(!s.flag_read && !s.job_read);
        CHECK(br::verdict(s) == br::Unread);
        std::string t = text_of(s);
        CHECK(has(t, "  +0x8    job not readable\n"));
        CHECK(has(t, "  +0xa8   flag not readable\n"));
        CHECK(has(t, "  route name not readable (record +0x20 0x0)\n"));
    }
    {
        Game g;
        g.s.at.flag = Game::record + 0x90;
        g.m.put<uint8_t>(Game::record + 0x90, 0);
        g.m.put<uint8_t>(Game::record + br::kFlag, 1);
        br::Snapshot &s = g.run();
        CHECK(br::verdict(s) == br::Stays);
        CHECK(has(text_of(s), "  the flag is not at the record's +0xa8 on this build\n"));
    }
}

static void test_name()
{
    {
        Game g;
        g.m.bytes(Game::record + br::kNameBuf,
                  "enter\x01"
                  "boot",
                  11);
        CHECK(!std::strcmp(g.run().name, "enter?boot"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::record + br::kName, 0x70000000);
        br::Snapshot &s = g.run();
        CHECK(!s.name_read && !s.name[0]);
        CHECK(has(text_of(s), "  route name not readable (record +0x20 0x70000000)\n"));
    }
    {
        Game g;
        g.m.map(0x71000000, 4);
        g.m.bytes(0x71000000, "abcd", 4);
        g.m.put<uint64_t>(Game::record + br::kName, 0x71000000);
        CHECK(!g.run().name_read);
    }
    {
        Game g;
        g.m.map(0x72000000, 0x100);
        std::vector<char> v(0x100, 'x');
        g.m.bytes(0x72000000, v.data(), v.size());
        g.m.put<uint64_t>(Game::record + br::kName, 0x72000000);
        br::Snapshot &s = g.run();
        CHECK(s.name_read && std::strlen(s.name) == br::kNameMax);
    }
}

static void test_listeners()
{
    {
        Game g;
        g.listeners(20);
        br::Snapshot &s = g.run();
        CHECK(s.listeners_total == 20 && s.listeners_shown == br::kMaxListeners);
        std::string t = text_of(s);
        CHECK(has(t, "  job +0x100 listeners: 20\n"));
        CHECK(has(t, "    #15 0x1430000f0, argument 0x3000000f\n"));
        CHECK(!has(t, "    #16 "));
        CHECK(has(t, "    (4 more not read)\n"));
    }
    {
        Game g;
        g.listeners(0);
        br::Snapshot &s = g.run();
        CHECK(s.listeners_read && s.listeners_total == 0);
        CHECK(has(text_of(s), "  job +0x100 listeners: 0\n"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::job + br::kListenersEnd, Game::list + 8);
        br::Snapshot &s = g.run();
        CHECK(s.listeners_total == 0);
        CHECK(has(text_of(s), "  job +0x100 listeners 0x21000000..0x21000008: not a list of 16-byte entries\n"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::job + br::kListeners, Game::list + 0x10);
        g.m.put<uint64_t>(Game::job + br::kListenersEnd, Game::list);
        CHECK(has(text_of(g.run()), "not a list of 16-byte entries"));
    }
    {
        Game g;
        g.m.put<uint64_t>(Game::job + br::kListeners, 0x7ffffff0);
        g.m.put<uint64_t>(Game::job + br::kListenersEnd, 0x80000010);
        br::Snapshot &s = g.run();
        CHECK(s.listeners_total == 2 && s.listeners_shown == 0);
        CHECK(has(text_of(s), "    (2 more not read)\n"));
    }
}

static void test_unproven_and_off()
{
    {
        Game g;
        g.s.proven = false;
        std::snprintf(g.s.proof, sizeof g.s.proof, "%s", "the layout is proven on the KB build only");
        br::Snapshot &s = g.run();
        CHECK(!s.name_read && !s.vtable_read && !s.listeners_read);
        CHECK(s.status_read && br::verdict(s) == br::Stays);
        std::string t = text_of(s);
        CHECK(has(t, "  route name, job vtable and listeners not decoded: the layout is proven on the KB build only\n"));
        CHECK(has(t, "  +0x8    job 0x20000000\n"));
        CHECK(!has(t, "listeners:"));
        CHECK(has(t, "  job bytes +0x000..+0x200"));
    }
    {
        br::Snapshot s;
        s.off = true;
        std::snprintf(s.why, sizeof s.why, "%s", "FeFlow_StateFTUE_OnUpdate did not match once on this build");
        std::string t = text_of(s);
        CHECK(t == "\n== Front end boot route (what StateFTUE #04fc8ffa waits on before StateFrontend; read while the game "
                   "runs)\n  off: FeFlow_StateFTUE_OnUpdate did not match once on this build\n");
    }
}

static void test_small_buffer()
{
    Game g;
    br::Snapshot &s = g.run();
    std::string full = text_of(s, namer);
    for (size_t cap : {size_t(1), size_t(2), size_t(64), size_t(500), full.size(), full.size() + 1}) {
        std::vector<char> b(cap + 8, '#');
        size_t n = br::text(s, b.data(), cap, namer);
        CHECK(n < cap);
        CHECK(b[n] == 0);
        CHECK(b[cap] == '#');
        CHECK(full.compare(0, n, b.data(), n) == 0);
    }
    CHECK(br::text(s, nullptr, 0, namer) == 0);
}

int main()
{
    test_signature_and_operands();
    test_pending_route();
    test_verdicts();
    test_name();
    test_listeners();
    test_unproven_and_off();
    test_small_buffer();
    if (g_fail) {
        std::printf("boot_route: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("boot_route: ok\n");
    return 0;
}
