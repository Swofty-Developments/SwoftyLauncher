// Host-native unit test for the core's session enter guard (native/src/core/session_enter_logic.hpp).
// Built and run by native/test/run_session_enter_test.sh.

#include "session_enter_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>
#include <vector>

namespace se = runtime::session_enter;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Memory {
    std::map<uint64_t, uint8_t> bytes;

    void put(uint64_t a, const void *p, size_t n)
    {
        for (size_t i = 0; i < n; i++)
            bytes[a + i] = static_cast<const uint8_t *>(p)[i];
    }

    void put64(uint64_t a, uint64_t v)
    {
        put(a, &v, 8);
    }
};

static bool read(uint64_t address, void *out, size_t n, void *user)
{
    Memory *m = static_cast<Memory *>(user);
    for (size_t i = 0; i < n; i++) {
        auto it = m->bytes.find(address + i);
        if (it == m->bytes.end())
            return false;
        static_cast<uint8_t *>(out)[i] = it->second;
    }
    return true;
}

static constexpr uint64_t MANAGER = 0x100000;
static constexpr uint64_t GROUPS = 0x110000;
static constexpr uint64_t GROUP = 0x120000;
static constexpr uint64_t HEAD = 0x130000;
static constexpr uint64_t RSP = 0x66e4e7d0;

struct Tree {
    uint64_t next = 0x200000;
    uint64_t build(Memory &m, const std::vector<uint64_t> &keys, size_t lo, size_t hi)
    {
        if (lo >= hi)
            return HEAD;
        size_t mid = (lo + hi) / 2;
        uint64_t n = next;
        next += 0x40;
        m.put64(n + se::NODE_LEFT, build(m, keys, lo, mid));
        m.put64(n + se::NODE_RIGHT, build(m, keys, mid + 1, hi));
        m.put64(n + se::NODE_KEY, keys[mid]);
        uint8_t zero = 0;
        m.put(n + se::NODE_ISNIL, &zero, 1);
        return n;
    }
};

static Memory world(std::vector<uint64_t> keys, uint64_t index = 5, bool groups = true)
{
    Memory m;
    Tree t;
    uint64_t root = t.build(m, keys, 0, keys.size());
    uint64_t leftmost = HEAD;
    if (!keys.empty()) {
        uint64_t n = root;
        for (;;) {
            uint64_t l = 0;
            read(n + se::NODE_LEFT, &l, 8, &m);
            if (l == HEAD)
                break;
            n = l;
        }
        leftmost = n;
    }
    uint8_t one = 1;
    m.put(HEAD + se::NODE_ISNIL, &one, 1);
    m.put64(HEAD + se::NODE_LEFT, leftmost);
    m.put64(HEAD + se::NODE_PARENT, root);
    m.put64(GROUP + se::GROUP_MAP_AT, HEAD);
    m.put64(MANAGER + se::GROUPS_BEGIN_AT, GROUPS);
    m.put64(MANAGER + se::GROUPS_END_AT, groups ? GROUPS + 8 : GROUPS);
    m.put64(GROUPS, GROUP);
    m.put64(RSP + se::INDEX_AT, index);
    return m;
}

int main()
{
    {
        Memory m = world({10, 20, 30, 40, 50});
        se::Fix f = se::decide(MANAGER, RSP, 30, read, &m);
        expect(!f.replace, "a hash group 0 lists runs the game's code");
        f = se::decide(MANAGER, RSP, 10, read, &m);
        expect(!f.replace, "the leftmost key itself is found");
        f = se::decide(MANAGER, RSP, 50, read, &m);
        expect(!f.replace, "the largest key is found");
    }
    {
        Memory m = world({10, 20, 30, 40, 50});
        se::Fix f = se::decide(MANAGER, RSP, 35, read, &m);
        expect(f.replace && f.key == 10, "a hash between keys borrows the leftmost key");
        expect(f.index == 5, "the fix carries the vehicle index");
        f = se::decide(MANAGER, RSP, 99, read, &m);
        expect(f.replace && f.key == 10, "a hash above every key borrows the leftmost key");
        f = se::decide(MANAGER, RSP, 1, read, &m);
        expect(f.replace && f.key == 10, "a hash below every key borrows the leftmost key");
    }
    {
        Memory m = world({77});
        expect(!se::decide(MANAGER, RSP, 77, read, &m).replace, "a one-node map finds its key");
        se::Fix f = se::decide(MANAGER, RSP, 78, read, &m);
        expect(f.replace && f.key == 77, "a one-node map lends its only key");
    }
    {
        Memory m = world({});
        expect(!se::decide(MANAGER, RSP, 5, read, &m).replace, "an empty map is left to the game");
    }
    {
        Memory m = world({10, 20}, 5, false);
        expect(!se::decide(MANAGER, RSP, 5, read, &m).replace, "no asset groups is left to the game");
    }
    {
        Memory m;
        expect(!se::decide(MANAGER, RSP, 5, read, &m).replace, "unreadable memory is left to the game");
        expect(!se::decide(0, RSP, 5, read, &m).replace, "a null manager is left to the game");
    }
    {
        Memory m = world({10, 20, 30});
        m.bytes.erase(RSP + se::INDEX_AT);
        se::Fix f = se::decide(MANAGER, RSP, 25, read, &m);
        expect(f.replace && f.index == se::NO_INDEX, "an unreadable index still fixes and reports no index");
    }
    {
        Memory m = world({10, 20, 30});
        uint64_t root = 0;
        read(HEAD + se::NODE_PARENT, &root, 8, &m);
        m.put64(root + se::NODE_LEFT, root);
        m.put64(root + se::NODE_KEY, 100);
        expect(!se::decide(MANAGER, RSP, 5, read, &m).replace, "a cyclic map is left to the game");
    }
    {
        se::Skips k;
        int a = k.note(), b = k.note();
        expect(k.logged(a) && k.logged(b) && !k.last(b), "early fixes are logged");
        for (int i = 2; i < se::LOG_FIRST; i++)
            b = k.note();
        expect(k.last(b) && k.logged(b), "the last logged fix says so");
        expect(!k.logged(k.note()), "later fixes are not logged");
    }
    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
