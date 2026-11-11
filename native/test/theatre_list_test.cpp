// Host-native unit test for the core's theatre list guard (native/src/core/theatre_list_logic.hpp).
// Built and run by native/test/run_theatre_list_test.sh.

#include "theatre_list_logic.hpp"

#include <cstdio>
#include <cstring>
#include <map>

namespace tl = runtime::theatre_list;
namespace gs = runtime::garage_setup;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Memory {
    std::map<uint64_t, uint64_t> words;
    std::map<uint64_t, uint32_t> dwords;
    int reads = 0;
};

static bool read(uint64_t address, void *out, size_t n, void *user)
{
    Memory *m = static_cast<Memory *>(user);
    m->reads++;
    if (n == 8) {
        auto it = m->words.find(address);
        if (it == m->words.end())
            return false;
        memcpy(out, &it->second, 8);
        return true;
    }
    auto it = m->dwords.find(address);
    if (it == m->dwords.end())
        return false;
    memcpy(out, &it->second, 4);
    return true;
}

int main()
{
    const uint64_t no_slot = 0x142bfc761, poison = 0xdeadbeefcafef00dull;
    Memory m;
    const uint64_t entry = 0x329bc98d0, node = 0x329e981d0, info = 0x32a000000;
    m.words[entry + gs::NODE_AT] = node;
    m.words[node + gs::INFO_AT] = info;
    m.dwords[info + gs::ID_AT] = 7;

    tl::Exit exit{poison, poison};
    expect(tl::entry_step(entry, no_slot, read, &m, &exit), "an entry that reads down to its id runs the game's code");
    expect(exit.rax == poison && exit.rip == poison, "a usable entry leaves the exit untouched");

    exit = {poison, poison};
    expect(!tl::entry_step(0x2a00002f33000020ull, no_slot, read, &m, &exit), "the non-canonical entry from the reports is skipped");
    expect(exit.rax == 0, "the skip answers with a null slot record");
    expect(exit.rip == no_slot, "the skip continues at the function's epilogue");

    exit = {poison, poison};
    expect(!tl::entry_step(0, no_slot, read, &m, &exit) && exit.rip == no_slot, "a null entry is skipped");
    expect(!tl::entry_step(0xffffffffffffffffull, no_slot, read, &m, &exit), "an all-ones entry is skipped");
    expect(!tl::entry_step(0x9d9ec88, no_slot, read, &m, &exit), "a canonical entry that does not read is skipped");

    Memory counted = m;
    counted.reads = 0;
    tl::entry_step(entry, no_slot, read, &counted, &exit);
    expect(counted.reads == 3, "a usable entry costs exactly three reads, one per link");
    Memory refused = m;
    refused.reads = 0;
    tl::entry_step(0x2a00002f33000020ull, no_slot, read, &refused, &exit);
    expect(refused.reads == 0, "a non-canonical entry is refused without reading");

    Memory badnode = m;
    badnode.words[entry + gs::NODE_AT] = 0x2a00002f33000020ull;
    expect(!tl::entry_step(entry, no_slot, read, &badnode, &exit), "a node that is not a pointer is skipped");

    Memory noinfo = m;
    noinfo.words.erase(node + gs::INFO_AT);
    expect(!tl::entry_step(entry, no_slot, read, &noinfo, &exit), "a node whose info does not read is skipped");

    Memory noid = m;
    noid.dwords.clear();
    expect(!tl::entry_step(entry, no_slot, read, &noid, &exit), "an info whose id does not read is skipped");

    tl::Skips s;
    int lines = 0, last = 0;
    for (int i = 0; i < 500; i++) {
        int n = s.note();
        if (s.logged(n))
            lines++;
        if (s.last(n))
            last++;
    }
    expect(lines == tl::LOG_FIRST, "five hundred skips produce exactly the capped number of lines");
    expect(last == 1, "the capped line is marked once");

    std::printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
