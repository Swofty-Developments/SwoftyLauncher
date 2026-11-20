#pragma once

#include <algorithm>
#include <cstddef>
#include <vector>

namespace runtime::loader {

// Where a core unit's registrations start in the SDK mixin list: how many nodes the list held when its rank mark was
// constructed, and the how-manyth mark that was.
struct RankMarkAt {
    size_t at = 0;
    size_t seq = 0;
    bool set = false;
};

// Orders a singly linked list of nodes by the rank of the mark each node follows (the set mark with the greatest at,
// then seq, at or before the node's position), keeping the list order within a rank and putting nodes no mark
// precedes first. Returns the new head.
template <class Node> Node *rank_list(Node *head, const RankMarkAt *marks, size_t count)
{
    std::vector<Node *> nodes;
    for (Node *n = head; n; n = n->next)
        nodes.push_back(n);
    std::vector<size_t> owner(nodes.size(), 0);
    for (size_t i = 0; i < nodes.size(); i++) {
        const RankMarkAt *best = nullptr;
        for (size_t r = 0; r < count; r++)
            if (marks[r].set && marks[r].at <= i &&
                (!best || marks[r].at > best->at || (marks[r].at == best->at && marks[r].seq > best->seq)))
                best = &marks[r];
        owner[i] = best ? size_t(best - marks) + 1 : 0;
    }
    std::vector<size_t> order(nodes.size());
    for (size_t i = 0; i < order.size(); i++)
        order[i] = i;
    std::stable_sort(order.begin(), order.end(), [&](size_t a, size_t b) { return owner[a] < owner[b]; });
    Node *first = nullptr, **tail = &first;
    for (size_t i : order) {
        *tail = nodes[i];
        tail = &nodes[i]->next;
    }
    *tail = nullptr;
    return first;
}

}
