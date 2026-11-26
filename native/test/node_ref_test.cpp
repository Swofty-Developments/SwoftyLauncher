// Host unit test for the reference binding recipe (frontend/node_ref_logic.hpp) on a fake node struct: the handle link at
// node+0x28, the +0x24 count, the leaf's own copy, the unlink and release of the temporary, and the refusals.
// native/test/run_tests.sh node_ref builds and runs it.
#include "core/frontend/node_ref_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace nr = frontend::node_ref;
namespace era = runtime::era;

static int fails;
static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Node {
    alignas(8) uint8_t mem[0x68] = {};
    void *&holders() { return *reinterpret_cast<void **>(mem + era::NODE_HOLDERS); }
    int16_t &refs() { return *reinterpret_cast<int16_t *>(mem + era::NODE_REFS); }
    uint8_t *ptr() { return mem; }
};

static size_t holders_of(Node &n)
{
    size_t c = 0;
    for (auto *h = static_cast<era::Handle *>(n.holders()); h; h = static_cast<era::Handle *>(h->next))
        c++;
    return c;
}

struct Fake {
    void *empty_slot = nullptr;
    void **empty() { return &empty_slot; }
    era::Handle leaf{};
    void *leaf_slot = nullptr;
    bool leaf_is_ref = true;
    bool saw_linked = false;
    int released = 0;
    int16_t refs_during = 0;
    void **set_ref(Node &target, void *handle)
    {
        auto *h = static_cast<era::Handle *>(handle);
        saw_linked = h->node == target.ptr() && target.holders() == h;
        refs_during = target.refs();
        if (!leaf_is_ref)
            return empty();
        era::link(leaf, h->node);
        leaf_slot = leaf.node;
        return &leaf_slot;
    }
};

int main()
{
    Node target;
    Fake f;
    void *slot = target.ptr();
    int r = nr::bind(
        &slot, f.empty(), [&](void *h) { return f.set_ref(target, h); },
        [&](uint8_t *n) {
            f.released++;
            --*reinterpret_cast<int16_t *>(n + era::NODE_REFS);
        });
    expect(r == nr::OK, "bind: a live target binds");
    expect(f.saw_linked && f.refs_during == 1, "bind: the temporary is linked at +0x28 and counted at +0x24 during the call");
    expect(f.released == 1, "bind: the temporary is released once");
    expect(target.refs() == 1 && holders_of(target) == 1 && target.holders() == &f.leaf,
           "bind: only the leaf's handle keeps the count and the holder list");
    expect(f.leaf.next == nullptr && f.leaf.node == target.ptr(), "bind: the leaf's handle is intact");

    Node other;
    other.refs() = 3;
    Fake g;
    void *slot2 = other.ptr();
    int released = 0;
    auto rel = [&](uint8_t *n) {
        released++;
        --*reinterpret_cast<int16_t *>(n + era::NODE_REFS);
    };
    nr::bind(&slot2, g.empty(), [&](void *h) { return g.set_ref(other, h); }, rel);
    Fake g2;
    nr::bind(&slot2, g2.empty(), [&](void *h) { return g2.set_ref(other, h); }, rel);
    expect(other.refs() == 5 && holders_of(other) == 2, "bind: two leaves on one target hold two counts and two links");
    expect(g.leaf.next == nullptr && g2.leaf.next == &g.leaf, "bind: the holder chain is leaf2 -> leaf1");

    Node t3;
    Fake h;
    h.leaf_is_ref = false;
    void *slot3 = t3.ptr();
    int rel3 = 0;
    r = nr::bind(
        &slot3, h.empty(), [&](void *x) { return h.set_ref(t3, x); },
        [&](uint8_t *n) {
            rel3++;
            --*reinterpret_cast<int16_t *>(n + era::NODE_REFS);
        });
    expect(r == nr::REFUSED, "refused: a non-reference leaf is refused");
    expect(rel3 == 1 && t3.refs() == 0 && t3.holders() == nullptr, "refused: the temporary leaves no count and no dangling link");

    Fake e;
    int calls = 0;
    void *empty_slot_alias = nullptr;
    r = nr::bind(
        e.empty(), e.empty(),
        [&](void *) {
            calls++;
            return e.empty();
        },
        [&](uint8_t *) { calls++; });
    expect(r == nr::EMPTY_TARGET && calls == 0, "empty: the empty handle slot as target is refused before any call");
    void *null_node = nullptr;
    r = nr::bind(
        &null_node, e.empty(),
        [&](void *) {
            calls++;
            return e.empty();
        },
        [&](uint8_t *) { calls++; });
    expect(r == nr::EMPTY_TARGET && calls == 0, "empty: a slot holding no node is refused before any call");
    r = nr::bind(
        nullptr, e.empty(),
        [&](void *) {
            calls++;
            return e.empty();
        },
        [&](uint8_t *) { calls++; });
    expect(r == nr::EMPTY_TARGET && calls == 0, "empty: a null slot is refused");
    (void)empty_slot_alias;
    expect(std::string(nr::describe(nr::REFUSED)).find("not a reference") != std::string::npos, "describe: names the refusal");
    printf("%s\n", fails ? "FAILED" : "all ok");
    return fails ? 1 : 0;
}
