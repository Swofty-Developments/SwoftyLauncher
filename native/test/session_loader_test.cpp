// Host-native unit test for the load-stall report's session loader section (native/src/core/diagnostics/session_loader_logic.hpp):
// the id lookup against a robin-hood node map and an MSVC-style row tree built here the way the game builds them
// (found, missing, no row, unreadable, a looping tree and a probe that never ends), the loader fields and id list the
// capture reads, the first id the update stops at, the id list verdicts and cap, the raw rows and the text of every
// branch. Built and run by native/test/run_tests.sh session_loader.

#include "core/diagnostics/session_loader_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace sl = runtime::session_loader;

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
    uint64_t next = 0x10000000;
    size_t reads = 0;

    uint64_t alloc(size_t n)
    {
        uint64_t a = next;
        next += (n + 0x1fff) & ~uint64_t(0xfff);
        regions[a] = std::vector<uint8_t>(n);
        return a;
    }

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

    bool read(uint64_t a, void *out, size_t n)
    {
        reads++;
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

struct IdMap {
    Mem &m;
    uint64_t sys = 0, owner = 0, head = 0, slots = 0, info = 0;
    uint64_t mult = 0x9e3779b97f4a7c15ull, mask = 63;
    uint32_t inc = 32;
    uint8_t shift = 0;
    size_t total = 0;
    std::vector<uint64_t> keyvals;
    std::vector<uint8_t> infos;

    IdMap(Mem &mem, uint64_t mask_, uint32_t inc_, uint8_t shift_) : m(mem), mask(mask_), inc(inc_), shift(shift_)
    {
        total = size_t(mask) + 1 + 64;
        keyvals.assign(total, 0);
        infos.assign(total + 1, 0);
        infos[total] = 1;
        sys = m.alloc(0x100);
        owner = m.alloc(0x100);
        slots = m.alloc(total * 8);
        info = m.alloc(total + 8);
        head = m.alloc(0x30);
        m.put<uint64_t>(sys + sl::kOwner, owner);
        m.put<uint64_t>(sys + sl::kMultiplier, mult);
        m.put<uint64_t>(sys + sl::kSlots, slots);
        m.put<uint64_t>(sys + sl::kInfo, info);
        m.put<uint64_t>(sys + sl::kMask, mask);
        m.put<uint32_t>(sys + sl::kInfoInc, inc);
        m.put<uint8_t>(sys + sl::kInfoShift, shift);
        m.put<uint64_t>(owner + sl::kRows, head);
        m.put<uint64_t>(head + sl::kLeft, head);
        m.put<uint64_t>(head + sl::kParent, head);
        m.put<uint64_t>(head + sl::kRight, head);
        m.put<uint8_t>(head + sl::kNil, 1);
        flush();
    }

    void home(uint32_t id, uint64_t &idx, uint32_t &want) const
    {
        uint64_t h = uint64_t(id) * mult;
        h ^= h >> 33;
        want = inc + uint32_t((h & 0x1f) >> shift);
        idx = (h >> 5) & mask;
    }

    void insert(uint32_t id, uint32_t key)
    {
        uint64_t node = m.alloc(0x10);
        m.put<uint32_t>(node + sl::kNodeId, id);
        m.put<uint32_t>(node + sl::kNodeKey, key);
        uint64_t idx;
        uint32_t want;
        home(id, idx, want);
        while (want <= infos[idx]) {
            idx++;
            want += inc;
        }
        uint64_t spot = idx, end = idx;
        while (infos[end])
            end++;
        if (want > 0xff || end >= total) {
            std::printf("FAIL test setup: the id map overflows inserting 0x%x\n", id);
            g_fail++;
            return;
        }
        for (uint64_t i = end; i > spot; i--) {
            if (infos[i - 1] + inc > 0xff) {
                std::printf("FAIL test setup: the id map overflows shifting for 0x%x\n", id);
                g_fail++;
                return;
            }
            keyvals[i] = keyvals[i - 1];
            infos[i] = uint8_t(infos[i - 1] + inc);
        }
        keyvals[spot] = node;
        infos[spot] = uint8_t(want);
        flush();
    }

    int distance(uint32_t id) const
    {
        uint64_t idx;
        uint32_t want;
        home(id, idx, want);
        for (uint64_t i = idx; i < total; i++)
            if (keyvals[i]) {
                uint32_t got = 0;
                const_cast<Mem &>(m).read(keyvals[i] + sl::kNodeId, &got, 4);
                if (got == id)
                    return int(i - idx);
            }
        return -1;
    }

    void flush()
    {
        for (size_t i = 0; i < total; i++)
            m.put<uint64_t>(slots + i * 8, keyvals[i]);
        for (size_t i = 0; i <= total; i++)
            m.put<uint8_t>(info + i, infos[i]);
    }

    void row(uint32_t key, uint32_t state)
    {
        uint64_t n = m.alloc(0x30);
        m.put<uint64_t>(n + sl::kLeft, head);
        m.put<uint64_t>(n + sl::kRight, head);
        m.put<uint8_t>(n + sl::kNil, 0);
        m.put<uint32_t>(n + sl::kRowKey, key);
        m.put<uint32_t>(n + sl::kRowState, state);
        uint64_t root = 0;
        m.read(head + sl::kParent, &root, 8);
        if (root == head) {
            m.put<uint64_t>(head + sl::kParent, n);
            m.put<uint64_t>(n + sl::kParent, head);
            return;
        }
        uint64_t x = root;
        for (;;) {
            uint32_t k = 0;
            m.read(x + sl::kRowKey, &k, 4);
            uint64_t side = key < k ? sl::kLeft : sl::kRight, child = 0;
            m.read(x + side, &child, 8);
            if (child == head) {
                m.put<uint64_t>(x + side, n);
                m.put<uint64_t>(n + sl::kParent, x);
                return;
            }
            x = child;
        }
    }
};

static uint32_t id_of(int i) { return 0x5a000000u ^ (uint32_t(i) * 2654435761u); }

static void lookups(uint64_t mask, uint32_t inc, uint8_t shift)
{
    Mem m;
    IdMap map(m, mask, inc, shift);
    std::vector<uint32_t> ids;
    uint64_t want_idx = ~uint64_t(0);
    for (int i = 100000; ids.size() < 3; i++) {
        uint64_t idx;
        uint32_t w;
        map.home(id_of(i), idx, w);
        if (want_idx == ~uint64_t(0))
            want_idx = idx;
        if (idx == want_idx)
            ids.push_back(id_of(i));
    }
    for (int i = 0; int(ids.size()) < int(mask * 4 / 10) + 3; i++)
        ids.push_back(id_of(i));
    const int n = int(ids.size());
    for (int i = 0; i < n; i++) {
        map.insert(ids[size_t(i)], 0x1000u + uint32_t(i) * 3);
        if (i % 7 != 3)
            map.row(0x1000u + uint32_t(i) * 3, uint32_t(i % 5));
    }
    int far = 0, odd = 0;
    for (int i = 0; i < n; i++) {
        int d = map.distance(ids[size_t(i)]);
        CHECK(d >= 0);
        far += d >= 2;
        odd += d % 2 == 1;
        sl::IdState e = sl::lookup(m.reader(), map.sys, ids[size_t(i)]);
        CHECK(e.id == ids[size_t(i)]);
        CHECK(e.key == 0x1000u + uint32_t(i) * 3);
        if (i % 7 == 3)
            CHECK(e.where == sl::NoRow);
        else {
            CHECK(e.where == sl::Found);
            CHECK(e.state == uint32_t(i % 5));
            CHECK(e.ready() == (i % 5 == 2));
        }
    }
    CHECK(far > 0);
    CHECK(odd > 0);
    for (int i = 50000; i < 50400; i++)
        CHECK(sl::lookup(m.reader(), map.sys, id_of(i)).where == sl::Missing);
}

static void unreadable()
{
    Mem m;
    IdMap map(m, 15, 32, 0);
    map.insert(0x1234, 7);
    map.row(7, 2);
    CHECK(sl::lookup(m.reader(), map.sys, 0x1234).where == sl::Found);
    CHECK(sl::lookup(m.reader(), 0, 0x1234).where == sl::Unread);
    CHECK(sl::lookup(m.reader(), 0x7000000000ull, 0x1234).where == sl::Unread);

    m.put<uint64_t>(map.sys + sl::kOwner, 0x7000000000ull);
    sl::IdState e = sl::lookup(m.reader(), map.sys, 0x1234);
    CHECK(e.where == sl::RowUnread);
    CHECK(e.key == 7);
    m.put<uint64_t>(map.sys + sl::kOwner, map.owner);

    m.put<uint64_t>(map.sys + sl::kSlots, 0x7000000000ull);
    CHECK(sl::lookup(m.reader(), map.sys, 0x1234).where == sl::Unread);
    m.put<uint64_t>(map.sys + sl::kSlots, map.slots);

    m.put<uint64_t>(map.sys + sl::kMask, ~uint64_t(0));
    CHECK(sl::lookup(m.reader(), map.sys, 0x1234).where == sl::Unread);
    m.put<uint64_t>(map.sys + sl::kMask, map.mask);
    CHECK(sl::lookup(m.reader(), map.sys, 0x1234).where == sl::Found);
}

static void rows()
{
    Mem m;
    IdMap map(m, 15, 32, 0);
    map.insert(0x10, 50);
    map.insert(0x20, 60);
    map.insert(0x30, 70);
    CHECK(sl::lookup(m.reader(), map.sys, 0x10).where == sl::NoRow);
    map.row(55, 2);
    map.row(40, 1);
    map.row(70, 3);
    CHECK(sl::lookup(m.reader(), map.sys, 0x10).where == sl::NoRow);
    CHECK(sl::lookup(m.reader(), map.sys, 0x20).where == sl::NoRow);
    sl::IdState e = sl::lookup(m.reader(), map.sys, 0x30);
    CHECK(e.where == sl::Found && e.state == 3);
    map.row(50, 2);
    e = sl::lookup(m.reader(), map.sys, 0x10);
    CHECK(e.where == sl::Found && e.state == 2 && e.ready());
    map.row(80, 9);
    CHECK(sl::lookup(m.reader(), map.sys, 0x20).where == sl::NoRow);

    uint64_t root = 0;
    m.read(map.head + sl::kParent, &root, 8);
    m.put<uint64_t>(root + sl::kRight, root);
    m.put<uint64_t>(root + sl::kLeft, root);
    size_t before = m.reads;
    CHECK(sl::lookup(m.reader(), map.sys, 0x30).where == sl::RowUnread);
    CHECK(m.reads - before < 1000);
}

static void endless_probe()
{
    Mem m;
    IdMap map(m, 0, 0, 0);
    uint64_t info = m.alloc(1 << 16);
    std::memset(m.at(info, 1 << 16), 0x20, 1 << 16);
    m.put<uint64_t>(map.sys + sl::kInfo, info);
    size_t before = m.reads;
    sl::IdState e = sl::lookup(m.reader(), map.sys, 0x99);
    CHECK(e.where == sl::Unread);
    CHECK(m.reads - before < 4 * size_t(sl::kProbes));
    CHECK(m.reads - before > size_t(sl::kProbes) / 2);
}

struct Loader {
    Mem m;
    IdMap map{m, 63, 32, 0};
    uint64_t at = 0, ids = 0, batch = 0;

    Loader(const std::vector<std::pair<uint32_t, int>> &list, uint32_t state = 7, uint32_t assets = 2)
    {
        at = m.alloc(sl::kRaw);
        batch = m.alloc(sl::kBatchRaw);
        ids = m.alloc(list.size() * 4 + 4);
        for (size_t i = 0; i < list.size(); i++) {
            m.put<uint32_t>(ids + i * 4, list[i].first);
            if (list[i].second >= -1)
                map.insert(list[i].first, 0x100u + uint32_t(i));
            if (list[i].second >= 0)
                map.row(0x100u + uint32_t(i), uint32_t(list[i].second));
        }
        m.put<uint32_t>(at + sl::kState, state);
        m.put<uint32_t>(at + sl::kAssets, assets);
        m.put<uint32_t>(at + sl::kSubtasks, 0);
        m.put<uint64_t>(at + sl::kIdSystem, map.sys);
        m.put<uint64_t>(at + sl::kBatch, batch);
        m.put<uint64_t>(at + sl::kIdsBegin, ids);
        m.put<uint64_t>(at + sl::kIdsEnd, ids + list.size() * 4);
        m.put<uint64_t>(at + sl::kDone, 0x143709d90ull);
        m.put<uint64_t>(at + sl::kDoneArg, 0x2000);
        m.put<uint64_t>(batch, 0xb0b0);
    }
};

static sl::Snapshot g_snap;
static char g_out[1 << 20];

static void namer(char *out, size_t cap, uint64_t at)
{
    std::snprintf(out, cap, "F1_25.exe+0x%llx", (unsigned long long)(at - 0x140000000ull));
}

static sl::Snapshot &fresh(uint64_t at, bool proven = true)
{
    g_snap.off = false;
    g_snap.why[0] = 0;
    g_snap.update = {true, at, 1000, 50};
    g_snap.unload = {true, at, 400, 1};
    g_snap.now = 3000;
    g_snap.proven = proven;
    std::snprintf(g_snap.proof, sizeof g_snap.proof, "%s", proven ? "" : "the session loader's fields are proven on X only");
    return g_snap;
}

static bool has(const char *text, const char *what) { return std::strstr(text, what) != nullptr; }

static void capture_stuck()
{
    Loader l({{0x11111111, 2}, {0x22222222, 2}, {0x33333333, 1}, {0x44444444, 2}, {0x55555555, -1}, {0x66666666, -2}});
    sl::Snapshot &s = fresh(l.at);
    sl::capture(l.m.reader(), s);
    CHECK(s.at == l.at && !s.other);
    CHECK(s.loader.read && s.loader.state == 7 && s.loader.assets == 2);
    CHECK(s.loader.batch == l.batch && s.loader.id_system == l.map.sys && s.loader.done == 0x143709d90ull);
    CHECK(s.verdict == sl::IdsOk && s.ids_total == 6 && s.ids_read == 6 && s.looked);
    CHECK(s.ids[0].ready() && s.ids[1].ready() && !s.ids[2].ready() && s.ids[3].ready());
    CHECK(s.ids[2].where == sl::Found && s.ids[2].state == 1);
    CHECK(s.ids[4].where == sl::NoRow);
    CHECK(s.ids[5].where == sl::Missing);
    sl::Counts c = sl::count(s);
    CHECK(c.ready == 3 && c.not_ready == 3 && c.unread == 0 && c.stop == 2);
    CHECK(s.raw_row[0] && s.raw_row[sl::kRaw / sl::kRow - 1]);
    CHECK(s.batch_row[0]);
    size_t n = sl::text(s, g_out, sizeof g_out, namer);
    CHECK(n == std::strlen(g_out));
    CHECK(has(g_out, "\n== Session loader ("));
    CHECK(has(g_out, "  Game_StreamingAssetLoadTask_Update: last run 2.0 s ago on 0x"));
    CHECK(has(g_out, "  Sess_Loader_UnloadSession: last run 2.6 s ago on 0x"));
    CHECK(has(g_out, "  +0x8    state 7: its tick runs Game_StreamingAssetLoadTask_Update\n"));
    CHECK(has(g_out, "  +0x10   asset load 2: the update waits until every id below reports 2\n"));
    CHECK(has(g_out, "  +0x220  completion 0x143709d90 (F1_25.exe+0x3709d90), argument (+0x228) 0x2000\n"));
    CHECK(has(g_out, ": 6 id(s); 3 report 2, 3 do not, 0 not readable\n"));
    CHECK(has(g_out, "  the update stops at id #2 0x33333333: its row's state 1\n"));
    CHECK(has(g_out, "    #0    11111111=2      #1    22222222=2      #2    33333333=1      #3    44444444=2\n"));
    CHECK(has(g_out, "    #4    55555555=no-row #5    66666666=none\n"));
    CHECK(has(g_out, "    #2 33333333: node 0x"));
    CHECK(has(g_out, "in the id map with no state row (the game's lookup answers 0)\n"));
    CHECK(!has(g_out, "    #5 66666666: node"));
    CHECK(has(g_out, "  load batch bytes at 0x"));
    CHECK(has(g_out, "    +000  000000000000b0b0 0000000000000000\n"));
    CHECK(has(g_out, "  loader bytes +0x000..+0x300 at 0x"));
    CHECK(has(g_out, "    +000  0000000000000000 0000000000000007\n"));
    CHECK(has(g_out, "    +010  0000000000000002 0000000000000000\n"));
    CHECK(has(g_out, "    +2f0  0000000000000000 0000000000000000\n"));
}

static void capture_done_and_states()
{
    Loader l({{0x1, 2}, {0x2, 2}});
    sl::Snapshot &s = fresh(l.at);
    sl::capture(l.m.reader(), s);
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  every id reports 2: the update calls the completion on its next run\n"));
    CHECK(!has(g_out, "the update stops at"));

    Loader u({{0x1, 1}}, 5, 5);
    sl::Snapshot &t = fresh(u.at);
    sl::capture(u.m.reader(), t);
    sl::text(t, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  +0x8    state 5: its tick runs Sess_Module_OnSessionUnloaded\n"));
    CHECK(has(g_out, "  +0x10   asset load 5: the update returns at once\n"));
    CHECK(!has(g_out, "the update stops at"));
    CHECK(has(g_out, "    #0    00000001=1\n"));

    u.m.put<uint64_t>(u.at + sl::kIdSystem, 0);
    u.m.put<uint64_t>(u.at + sl::kDone, 0);
    u.m.put<uint64_t>(u.at + sl::kDoneArg, 0);
    sl::capture(u.m.reader(), t);
    sl::text(t, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  +0xc0   id system 0: the update does not wait on the ids\n"));
    CHECK(has(g_out, "  +0x220  completion none (+0x220 and +0x228 are 0)\n"));
    CHECK(has(g_out, "; states not looked up\n"));
    CHECK(has(g_out, "    #0    00000001=-\n"));

    CHECK(!std::strcmp(sl::state_text(1), "its tick runs Sess_Loader_Update"));
    CHECK(!std::strcmp(sl::state_text(2), "its tick does nothing"));
    CHECK(!std::strcmp(sl::state_text(4), "its tick does nothing"));
    CHECK(!std::strcmp(sl::state_text(0), "its tick does nothing"));
    CHECK(!std::strcmp(sl::state_text(8), "its tick does nothing"));
    CHECK(!std::strcmp(sl::state_text(6), "its tick runs Sess_Loader_UpdateFrontendLoad"));
    CHECK(has(sl::state_text(3), "sub-tasks"));
    CHECK(!std::strcmp(sl::assets_text(0), "the update starts the asset load and sets 1"));
    CHECK(!std::strcmp(sl::assets_text(1), "the update returns at once"));
}

static void blind_stop()
{
    Loader l({{0x1, 2}, {0x2, 0}, {0x3, 1}});
    sl::Snapshot &s = fresh(l.at);
    sl::capture(l.m.reader(), s);
    s.ids[0].where = sl::Unread;
    sl::Counts c = sl::count(s);
    CHECK(c.unread == 1 && c.stop == -1);
    s.ids[0].where = sl::Found;
    c = sl::count(s);
    CHECK(c.stop == 1);
}

static void verdicts()
{
    Loader l({{0x1, 2}, {0x2, 2}});
    sl::Snapshot &s = fresh(l.at);
    l.m.put<uint64_t>(l.at + sl::kIdsEnd, l.ids);
    sl::capture(l.m.reader(), s);
    CHECK(s.verdict == sl::IdsEmpty && s.ids_read == 0);
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, ": empty: the update asks for the thumbnails again"));
    l.m.put<uint64_t>(l.at + sl::kIdsEnd, l.ids - 4);
    sl::capture(l.m.reader(), s);
    CHECK(s.verdict == sl::IdsBackwards);
    l.m.put<uint64_t>(l.at + sl::kIdsEnd, l.ids + 6);
    sl::capture(l.m.reader(), s);
    CHECK(s.verdict == sl::IdsRagged);
    l.m.put<uint64_t>(l.at + sl::kIdsEnd, l.ids + (uint64_t(4) << 21));
    sl::capture(l.m.reader(), s);
    CHECK(s.verdict == sl::IdsHuge && s.ids_read == 0);
}

static void capped()
{
    std::vector<std::pair<uint32_t, int>> list;
    for (int i = 0; i < 5000; i++)
        list.push_back({uint32_t(0x70000000 + i), -2});
    Loader l(list);
    l.m.put<uint64_t>(l.at + sl::kIdSystem, 0);
    sl::Snapshot &s = fresh(l.at);
    sl::capture(l.m.reader(), s);
    CHECK(s.ids_total == 5000 && s.ids_read == sl::kMaxIds);
    size_t n = sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, ": 5000 id(s), the first 4096 read; states not looked up\n"));
    CHECK(has(g_out, "#4095 70000fff=-"));
    CHECK(!has(g_out, "#4096"));
    CHECK(n < 200000);

    char small[64];
    size_t k = sl::text(s, small, sizeof small, namer);
    CHECK(k < sizeof small && k == std::strlen(small));
}

static void not_proven()
{
    Loader l({{0x1, 1}});
    sl::Snapshot &s = fresh(l.at, false);
    sl::capture(l.m.reader(), s);
    CHECK(!s.loader.read && s.ids_read == 0 && s.raw_row[0]);
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  fields not decoded: the session loader's fields are proven on X only\n"));
    CHECK(!has(g_out, "+0x188"));
    CHECK(!has(g_out, "load batch"));
    CHECK(has(g_out, "  loader bytes +0x000..+0x300 at 0x"));
    CHECK(has(g_out, "    +000  0000000000000000 0000000000000007\n"));
}

static void unreadable_loader()
{
    Mem m;
    uint64_t at = m.alloc(0x100);
    sl::Snapshot &s = fresh(at);
    sl::capture(m.reader(), s);
    CHECK(!s.loader.read);
    CHECK(s.raw_row[0] && s.raw_row[0xf] && !s.raw_row[0x10] && !s.raw_row[0x2f]);
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  its fields are not readable\n"));
    CHECK(has(g_out, "    +0f0  0000000000000000 0000000000000000\n"));
    CHECK(has(g_out, "    +100  not readable\n"));
}

static void probes()
{
    sl::Snapshot &s = fresh(0);
    s.update = {true, 0, 0, 0};
    s.unload = {false, 0, 0, 0};
    sl::capture([](uint64_t, void *, size_t) { return false; }, s);
    CHECK(s.at == 0);
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  Game_StreamingAssetLoadTask_Update: not run since it was hooked\n"));
    CHECK(has(g_out, "  Sess_Loader_UnloadSession: not hooked\n"));
    CHECK(has(g_out, "  no session loader seen yet\n"));

    uint64_t other = 0;
    CHECK(sl::pick({true, 0xa, 100, 1}, {true, 0xb, 200, 1}, other) == 0xb && other == 0xa);
    CHECK(sl::pick({true, 0xa, 300, 1}, {true, 0xb, 200, 1}, other) == 0xa && other == 0xb);
    CHECK(sl::pick({true, 0xa, 300, 1}, {true, 0xa, 200, 1}, other) == 0xa && other == 0);
    CHECK(sl::pick({true, 0xa, 300, 1}, {true, 0, 0, 0}, other) == 0xa && other == 0);
    CHECK(sl::pick({false, 0, 0, 0}, {true, 0xb, 5, 1}, other) == 0xb && other == 0);

    s.off = true;
    std::snprintf(s.why, sizeof s.why, "SWOFTY_SESSION_LOADER_PROBE=0");
    sl::text(s, g_out, sizeof g_out, namer);
    CHECK(has(g_out, "  off: SWOFTY_SESSION_LOADER_PROBE=0\n"));
    CHECK(!has(g_out, "Sess_Loader_UnloadSession:"));
}

int main()
{
    lookups(63, 32, 0);
    lookups(255, 16, 1);
    lookups(31, 32, 0);
    unreadable();
    rows();
    endless_probe();
    capture_stuck();
    capture_done_and_states();
    blind_stop();
    verdicts();
    capped();
    not_proven();
    unreadable_loader();
    probes();
    if (g_fail) {
        std::printf("session_loader: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("session_loader: ok\n");
    return 0;
}
