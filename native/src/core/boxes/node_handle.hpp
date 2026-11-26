#pragma once
#include <cstddef>
#include <cstdint>

namespace runtime::era {

constexpr size_t NODE_REFS = 0x24;
constexpr size_t NODE_HOLDERS = 0x28;
constexpr size_t HOLDER_PREV = 0x10;

struct Handle {
    uint8_t *node;
    void *next, *prev;
};
static_assert(offsetof(Handle, prev) == HOLDER_PREV);

inline void link(Handle &h, uint8_t *node)
{
    h = {node, *reinterpret_cast<void **>(node + NODE_HOLDERS), nullptr};
    if (h.next)
        *reinterpret_cast<void **>(static_cast<uint8_t *>(h.next) + HOLDER_PREV) = &h;
    *reinterpret_cast<void **>(node + NODE_HOLDERS) = &h;
    __atomic_fetch_add(reinterpret_cast<int16_t *>(node + NODE_REFS), int16_t(1), __ATOMIC_SEQ_CST);
}

// Unlinks a handle from its node's holder list and drops its reference, as the game's tile set clear does
// (prev.next = next, next.prev = prev, the node's head moves on when it was this handle). release is
// Vm_Release_Node. A handle the game already cleared (node null) is left alone.
template <class Release> bool unlink(Handle &h, Release release)
{
    uint8_t *node = h.node;
    if (!node)
        return false;
    auto *next = static_cast<uint8_t *>(h.next), *prev = static_cast<uint8_t *>(h.prev);
    if (prev)
        *reinterpret_cast<void **>(prev + 8) = next;
    if (next)
        *reinterpret_cast<void **>(next + HOLDER_PREV) = prev;
    h.next = h.prev = nullptr;
    auto **head = reinterpret_cast<void **>(node + NODE_HOLDERS);
    if (*head == &h)
        *head = next;
    release(node);
    h.node = nullptr;
    return true;
}

}
