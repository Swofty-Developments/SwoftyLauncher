// Host unit test for the bag scan (bag_scan_logic.hpp): synthetic bags built per docs/reference/save-format.md, the split of the
// root's containers block, the read stream's slots, and the put-back decision for a registered mode's container.
#include "core/career/bag_scan_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace bs = career::bag_scan;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

using Bytes = std::vector<uint8_t>;

static void u(Bytes &b, uint64_t v, int n)
{
    for (int i = 0; i < n; i++)
        b.push_back(uint8_t(v >> (8 * i)));
}

static void id(Bytes &b, const char (&s)[5]) { u(b, bs::fourcc(s), 4); }

static void key(Bytes &b, const std::string &k)
{
    b.push_back(uint8_t(k.size()));
    b.insert(b.end(), k.begin(), k.end());
}

static Bytes bag(const std::vector<Bytes> &entries)
{
    Bytes b;
    u(b, 1, 8);
    u(b, entries.size(), 8);
    for (auto &e : entries)
        b.insert(b.end(), e.begin(), e.end());
    return b;
}

static Bytes entry(const std::string &k, const char (&type)[5], const Bytes &value)
{
    Bytes b;
    key(b, k);
    id(b, type);
    b.insert(b.end(), value.begin(), value.end());
    return b;
}

static Bytes sample_bag()
{
    Bytes list;
    u(list, 1, 8);
    u(list, 2, 8);
    id(list, "ui32");
    u(list, 7, 4);
    id(list, "st32");
    list.push_back(3);
    list.insert(list.end(), {'a', 'b', 'c'});
    Bytes dstr;
    u(dstr, 5, 4);
    dstr.insert(dstr.end(), {'h', 'e', 'l', 'l', 'o'});
    Bytes dbrf1 = {1, 2, 1, 0, 0, 0, 2, 0, 0, 0}, dbrf0 = {0};
    Bytes nested = bag({entry("n", "si64", Bytes(8, 9))});
    return bag({entry("a", "ui08", {1}), entry("lst", "plst", list), entry("s", "dstr", dstr), entry("d1", "dbrf", dbrf1),
                entry("d0", "dbrf", dbrf0), entry("m", "mat4", Bytes(64, 1)), entry("sub", "pbag", nested),
                entry("b", "bffr", Bytes{2, 0, 0, 0, 9, 9}), entry("w", "s256", Bytes{1, 0, 'x'})});
}

static void scan()
{
    Bytes b = sample_bag();
    expect(bs::skip_bag(b.data(), b.size(), 0) == b.size(), "scan: a bag with every kind of value ends where it should");
    expect(bs::whole_bag(b.data(), b.size()), "scan: whole_bag accepts it");
    Bytes cut(b.begin(), b.end() - 1);
    expect(!bs::whole_bag(cut.data(), cut.size()), "scan: a bag cut short is refused");
    Bytes extra = b;
    extra.push_back(0);
    expect(!bs::whole_bag(extra.data(), extra.size()), "scan: trailing bytes are refused");
    Bytes v2 = b;
    v2[0] = 2;
    expect(!bs::whole_bag(v2.data(), v2.size()), "scan: a version above 1 is refused");
    Bytes unknown = bag({entry("a", "zzzz", {1})});
    expect(!bs::whole_bag(unknown.data(), unknown.size()), "scan: an unknown type id is refused");
    Bytes bigcount;
    u(bigcount, 1, 8);
    u(bigcount, 1ull << 40, 8);
    expect(!bs::whole_bag(bigcount.data(), bigcount.size()), "scan: a count the bytes cannot hold is refused");
    Bytes deep = bag({});
    for (int i = 0; i < 70; i++)
        deep = bag({entry("x", "pbag", deep)});
    expect(!bs::whole_bag(deep.data(), deep.size()), "scan: nesting past the limit is refused");
    expect(bs::fixed_size(bs::fourcc("rgba")) == 4 && bs::fixed_size(bs::PBAG) == 0 && bs::fixed_size(1234) == -1,
           "scan: type table");
}

static void split()
{
    Bytes a = sample_bag(), c = bag({entry("k", "bool", {1})});
    Bytes block;
    for (auto &p : {std::make_pair(std::string("driver_career"), a), std::make_pair(std::string("settings"), c)}) {
        key(block, p.first);
        id(block, "pbag");
        block.insert(block.end(), p.second.begin(), p.second.end());
    }
    std::vector<bs::Entry> out;
    expect(bs::split_entries(block.data(), block.size(), 2, out) && out.size() == 2, "split: two entries");
    expect(out[0].name == "driver_career" && Bytes(block.begin() + long(out[0].begin), block.begin() + long(out[0].end)) == a,
           "split: the first body is the bag bytes exactly");
    expect(out[1].name == "settings" && Bytes(block.begin() + long(out[1].begin), block.begin() + long(out[1].end)) == c,
           "split: the second body is the bag bytes exactly");
    out.clear();
    expect(!bs::split_entries(block.data(), block.size(), 1, out), "split: fewer entries than bytes is refused");
    out.clear();
    expect(!bs::split_entries(block.data(), block.size(), 3, out), "split: more entries than bytes is refused");
    Bytes wrong = block;
    wrong[1 + 13] = 'x';
    out.clear();
    expect(!bs::split_entries(wrong.data(), wrong.size(), 2, out), "split: an entry whose type is not pbag is refused");
    out.clear();
    expect(bs::split_entries(block.data(), 0, 0, out), "split: zero entries in no bytes");
}

static void stream()
{
    Bytes data = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    bs::ReadStream s;
    bs::init_read(s, data.data(), data.size());
    using Exact = int (*)(void *, void *, uint64_t);
    using Some = int (*)(void *, void *, uint64_t, uint64_t *);
    using One = int (*)(void *, void *);
    using Total = uint64_t (*)(void *);
    auto exact = reinterpret_cast<Exact>(s.vtable[0x30 / 8]);
    auto some = reinterpret_cast<Some>(s.vtable[0x28 / 8]);
    auto one = reinterpret_cast<One>(s.vtable[0x38 / 8]);
    uint8_t buf[16] = {};
    expect(exact(&s, buf, 4) == 0 && buf[0] == 1 && buf[3] == 4 && s.pos == 4, "stream: +0x30 reads exactly and advances");
    uint64_t got = 99;
    expect(some(&s, buf, 100, &got) == 0 && got == 6 && buf[0] == 5 && s.pos == 10,
           "stream: +0x28 reads what is left and reports it");
    expect(exact(&s, buf, 1) == bs::READ_ERROR && s.pos == 10, "stream: a read past the end is the game's error 0x54");
    bs::init_read(s, data.data(), data.size());
    expect(one(&s, buf) == 0 && buf[0] == 1 && s.pos == 1, "stream: +0x38 reads one byte");
    expect(reinterpret_cast<Total>(s.vtable[0xa8 / 8])(&s) == 10 && reinterpret_cast<Total>(s.vtable[0xb0 / 8])(&s) == 1,
           "stream: +0xa8 is the size and +0xb0 the position");
    expect(reinterpret_cast<int (*)(void *)>(s.vtable[0x40 / 8])(&s) == bs::READ_ERROR && s.bad_slot_calls == 1,
           "stream: a slot the reader should not use returns an error and is counted");
    expect(s.vtable == bs::read_vtable(), "stream: one vtable");
}

static void decision()
{
    using M = bs::Mode;
    expect(bs::registered_decision(false, 0, 2) == M::add_absent, "decision: a container the file lacks comes back");
    expect(bs::registered_decision(true, 0, 2) == M::replace_empty, "decision: a container with no careers is replaced by ours");
    expect(bs::registered_decision(true, 1, 3) == M::leave, "decision: the game's container with careers wins");
    expect(bs::registered_decision(false, 0, 0) == M::leave && bs::registered_decision(true, 0, 0) == M::leave,
           "decision: ours without careers is never put in");
}

int main()
{
    scan();
    split();
    stream();
    decision();
    printf("%s (%d failure(s))\n", fails ? "SOME FAILED" : "ALL PASSED", fails);
    return fails ? 1 : 0;
}
