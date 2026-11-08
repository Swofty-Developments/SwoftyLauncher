#include "../src/core/netplay_privilege_logic.hpp"

#include <cstdio>
#include <vector>

namespace np = runtime::netplay_privilege;

static std::vector<uint32_t> seen;
static bool all_granted = true;

static void cb(void *task, uint32_t privilege, bool success, const np::Continuation *next)
{
    seen.push_back(privilege);
    all_granted = all_granted && success && task && next && next->fn;
    next->fn(next->ctx, true);
}

struct Node {
    alignas(8) uint8_t link[0x20];
    alignas(8) uint8_t payload[0x20];
};

int main()
{
    alignas(8) uint8_t task[0x200] = {};
    alignas(8) uint8_t slot[0x40] = {};
    alignas(8) uint8_t list[0x40] = {};
    Node a{}, b{};
    auto put = [](uint8_t *base, size_t off, auto v) { std::memcpy(base + off, &v, sizeof v); };
    uint8_t *sentinel = list + np::list_sentinel;
    put(list, np::list_first, a.link);
    put(a.link, np::link_next, b.link);
    put(b.link, np::link_next, sentinel);
    put(a.link, np::link_payload, a.payload);
    put(b.link, np::link_payload, b.payload);
    put(a.payload, np::payload_enum, uint32_t(7));
    put(b.payload, np::payload_enum, uint32_t(11));
    put(a.payload, np::payload_fn, &cb);
    put(b.payload, np::payload_fn, &cb);
    put(a.payload, np::payload_ctx, static_cast<void *>(task));
    put(b.payload, np::payload_ctx, static_cast<void *>(task));
    put(task, np::task_slot, slot);
    put(task, np::task_list, list);
    put(slot, np::slot_state, int32_t(1));
    put(slot, np::slot_list, static_cast<void *>(list));

    int fail = 0;
    auto check = [&](bool ok, const char *what) {
        if (!ok) {
            std::printf("FAIL: %s\n", what);
            ++fail;
        }
    };

    check(np::grant(task) == 0, "not waiting: nothing answered");
    task[np::task_state] = np::waiting;
    put(slot, np::slot_state, int32_t(0));
    check(np::grant(task) == 0, "slot idle: nothing answered");
    put(slot, np::slot_state, int32_t(1));
    uint8_t other[0x40] = {};
    put(slot, np::slot_list, static_cast<void *>(other));
    check(np::grant(task) == 0 && seen.empty(), "slot owned by another request: left alone");
    int32_t kept;
    std::memcpy(&kept, slot + np::slot_state, sizeof kept);
    check(kept == 1, "another request's slot state kept");
    put(slot, np::slot_list, static_cast<void *>(list));
    check(np::grant(task) == 2, "two nodes answered");
    check(seen.size() == 2 && seen[0] == 7 && seen[1] == 11, "privileges in list order");
    check(all_granted, "granted with a continuation");
    int32_t state;
    std::memcpy(&state, slot + np::slot_state, sizeof state);
    check(state == 0, "slot released");
    check(np::grant(task) == 0, "second call is a no-op");

    std::printf(fail ? "netplay_privilege_test: %d failure(s)\n" : "netplay_privilege_test: ok\n", fail);
    return fail != 0;
}
