#pragma once
#include <cstdint>
#include <cstring>

namespace runtime::netplay_privilege {

constexpr size_t task_state = 0x158;
constexpr size_t task_slot = 0x140;
constexpr size_t task_list = 0x150;
constexpr size_t slot_list = 0x18;
constexpr size_t slot_state = 0x28;
constexpr size_t list_sentinel = 0x20;
constexpr size_t list_first = 0x28;
constexpr size_t link_next = 0x8;
constexpr size_t link_payload = 0x10;
constexpr size_t payload_enum = 0x4;
constexpr size_t payload_fn = 0x10;
constexpr size_t payload_ctx = 0x18;
constexpr uint8_t waiting = 2;

struct Continuation {
    void (*fn)(void *ctx, bool ok);
    void *ctx;
};

using NodeCallback = void(void *task, uint32_t privilege, bool success, const Continuation *next);

inline void done(void *, bool) {}

template <class T>
inline T load(const uint8_t *base, size_t off)
{
    T v;
    std::memcpy(&v, base + off, sizeof v);
    return v;
}

inline int grant(uint8_t *task)
{
    if (load<uint8_t>(task, task_state) != waiting)
        return 0;
    auto slot = load<uint8_t *>(task, task_slot);
    auto list = load<uint8_t *>(task, task_list);
    if (!slot || !list || load<int32_t>(slot, slot_state) != 1 || load<const uint8_t *>(slot, slot_list) != list)
        return 0;
    static Continuation next{&done, nullptr};
    int answered = 0;
    const uint8_t *end = list + list_sentinel;
    for (const uint8_t *link = load<const uint8_t *>(list, list_first); link && link != end;
         link = load<const uint8_t *>(link, link_next)) {
        auto payload = load<const uint8_t *>(link, link_payload);
        auto fn = load<NodeCallback *>(payload, payload_fn);
        if (!fn)
            continue;
        fn(load<void *>(payload, payload_ctx), load<uint32_t>(payload, payload_enum), true, &next);
        ++answered;
    }
    int32_t idle = 0;
    std::memcpy(slot + slot_state, &idle, sizeof idle);
    return answered;
}

}
