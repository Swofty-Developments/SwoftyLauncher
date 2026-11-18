// Host test of the core rank order (core/runtime/loader/core_rank_logic.hpp): the SDK mixin list is ordered by the
// rank of the mark each node follows, each unit's own nodes keep their order, and the unit initialization order does
// not change the result.

#include "core/runtime/loader/core_rank_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace ld = runtime::loader;

static int failed;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    failed += !ok;
}

struct Node {
    std::string name;
    Node *next;
};

struct Built {
    std::vector<Node *> nodes;
    std::vector<ld::RankMarkAt> marks;
    Node *head = nullptr;
};

// Builds the list the way static initialization does: each unit, in the order given, sets its mark and then appends
// its nodes. A unit is its rank and its node names.
static Built build(const std::vector<std::pair<size_t, std::vector<std::string>>> &units, size_t ranks)
{
    Built b;
    b.marks.resize(ranks);
    size_t seq = 0;
    for (auto &[rank, names] : units) {
        if (rank < ranks)
            b.marks[rank] = {b.nodes.size(), ++seq, true};
        for (auto &n : names)
            b.nodes.push_back(new Node{n, nullptr});
    }
    for (size_t i = 0; i + 1 < b.nodes.size(); i++)
        b.nodes[i]->next = b.nodes[i + 1];
    b.head = b.nodes.empty() ? nullptr : b.nodes[0];
    return b;
}

static std::string ranked(Built b)
{
    std::string out;
    for (Node *n = ld::rank_list(b.head, b.marks.data(), b.marks.size()); n; n = n->next)
        out += (out.empty() ? "" : " ") + n->name;
    for (Node *n : b.nodes)
        delete n;
    return out;
}

int main()
{
    expect(ranked(build({{0, {"a1", "a2"}}, {1, {"b1"}}, {2, {"c1", "c2"}}}, 3)) == "a1 a2 b1 c1 c2",
           "a list built in rank order keeps its order");
    expect(ranked(build({{2, {"c1", "c2"}}, {1, {"b1"}}, {0, {"a1", "a2"}}}, 3)) == "a1 a2 b1 c1 c2",
           "a list built in the reverse of rank order is put in rank order");
    expect(ranked(build({{1, {"b1", "b2", "b3"}}, {0, {"a1"}}, {2, {"c1"}}}, 3)) == "a1 b1 b2 b3 c1",
           "a unit's own nodes keep the order it declares them in");
    expect(ranked(build({{2, {"c1"}}, {0, {}}, {1, {"b1"}}}, 3)) == "b1 c1",
           "a rank whose unit registers nothing takes no node from the next unit");
    expect(ranked(build({{1, {}}, {0, {"a1"}}, {2, {}}}, 3)) == "a1", "marks with nothing after them leave the list alone");
    expect(ranked(build({{9, {"x1"}}, {1, {"b1"}}, {0, {"a1"}}}, 2)) == "x1 a1 b1", "nodes no mark precedes come first");
    expect(ranked(build({}, 3)).empty(), "an empty list stays empty");

    printf("%s\n", failed ? "FAILED" : "all passed");
    return failed ? 1 : 0;
}
