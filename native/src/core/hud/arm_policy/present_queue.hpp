#pragma once
#include <cstddef>
#include <cstdint>
#include <vector>

namespace hud::arm_policy {

// Whether a Present or Present1 closes the frame: not one made inside another on the same thread (DXVK's Present
// calls its Present1), which is the same frame.
inline bool dispatches_present(int presenting_depth) { return presenting_depth == 0; }

// What learn does with a swapchain whose present queue cannot be read: ask once to arm the device hooks, wait
// for a direct queue to execute command lists once they are armed, learn that queue, or refuse the swapchain.
enum class UnreadQueue { Arm, Wait, Learn, Refuse };

inline UnreadQueue unread_queue_step(bool exec_hooked, bool arm_asked, bool direct_queue_seen)
{
    if (exec_hooked)
        return direct_queue_seen ? UnreadQueue::Learn : UnreadQueue::Wait;
    return arm_asked ? UnreadQueue::Refuse : UnreadQueue::Arm;
}

constexpr int LIST_TYPE_DIRECT = 0;

// A command queue seen in ExecuteCommandLists: its type from one GetDesc, and the thread and tick of its
// last call while it is direct.
struct ExecSeen {
    void *queue;
    int type;
    uint32_t tid;
    uint64_t tick;
};

// The present queue of a swapchain the creation hooks missed: the direct queue that last executed command
// lists on the presenting thread, else the direct queue that executed most recently; nullptr when none has.
inline const ExecSeen *present_queue_from_execs(const std::vector<ExecSeen> &seen, uint32_t tid)
{
    const ExecSeen *same = nullptr, *newest = nullptr;
    for (auto &e : seen) {
        if (e.type != LIST_TYPE_DIRECT)
            continue;
        if (e.tid == tid && (!same || e.tick > same->tick))
            same = &e;
        if (!newest || e.tick > newest->tick)
            newest = &e;
    }
    return same ? same : newest;
}

// Whether a COM object's own vtable sends this slot to fn. A hook's trampoline runs the hooked
// implementation's body, which reads its this pointer as its own object type, so the core calls it only
// for an object of that type; a wrapper such as Streamline's queue proxy goes through its own vtable.
inline bool vtable_slot_is(const void *object, size_t slot, const void *fn)
{
    return object && fn && (*static_cast<const void *const *const *>(object))[slot] == fn;
}

}
