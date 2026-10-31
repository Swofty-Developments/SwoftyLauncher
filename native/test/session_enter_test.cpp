// Host-native unit test for the core's session enter guard (native/src/core/session_enter_logic.hpp).
// Built and run by native/test/run_session_enter_test.sh.

#include "session_enter_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>

namespace se = runtime::session_enter;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Memory {
    std::map<uint64_t, uint64_t> words;
};

static bool read(uint64_t address, void *out, size_t n, void *user)
{
    Memory *m = static_cast<Memory *>(user);
    auto it = m->words.find(address);
    if (n != 8 || it == m->words.end())
        return false;
    memcpy(out, &it->second, 8);
    return true;
}

static constexpr uint64_t RSP = 0x66e4e7d0;
static constexpr uint64_t MANAGER = 0x2c5d9518;

static Memory frame(uint64_t found, uint64_t manager, uint64_t index)
{
    Memory m;
    m.words[RSP + se::FOUND_AT] = found;
    m.words[RSP + se::MANAGER_AT] = manager;
    m.words[RSP + se::INDEX_AT] = index;
    return m;
}

int main()
{
    {
        Memory m = frame(0, MANAGER, 7);
        se::Skip s = se::decide(RSP, read, &m);
        expect(s.skip, "a null found vector skips the vehicle");
        expect(s.manager == MANAGER, "the skip carries the manager from [rsp+0x30]");
        expect(s.index == 7, "the skip carries the vehicle index from [rsp+0x60]");
    }
    {
        Memory m = frame(0x33060d810, MANAGER, 3);
        expect(!se::decide(RSP, read, &m).skip, "a vector with nodes runs the game's code");
    }
    {
        Memory m = frame(0, 0, 3);
        expect(!se::decide(RSP, read, &m).skip, "a null manager is left to the game");
    }
    {
        Memory m = frame(0, 0xffffffffffffffffull, 3);
        expect(!se::decide(RSP, read, &m).skip, "a non-canonical manager is left to the game");
    }
    {
        Memory m;
        m.words[RSP + se::FOUND_AT] = 0;
        m.words[RSP + se::MANAGER_AT] = MANAGER;
        se::Skip s = se::decide(RSP, read, &m);
        expect(s.skip && s.index == se::NO_INDEX, "an unreadable index still skips and reports no index");
    }
    {
        Memory m;
        expect(!se::decide(RSP, read, &m).skip, "an unreadable frame is left to the game");
    }
    {
        Memory m = frame(0, MANAGER, 0x1234ull);
        expect(se::decide(RSP, read, &m).index == 0x34, "the index is the low byte");
    }
    {
        se::Skips k;
        int a = k.note(), b = k.note();
        expect(k.logged(a) && k.logged(b) && !k.last(b), "early skips are logged");
        for (int i = 2; i < se::LOG_FIRST; i++)
            b = k.note();
        expect(k.last(b) && k.logged(b), "the last logged skip says so");
        expect(!k.logged(k.note()), "later skips are not logged");
    }
    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
