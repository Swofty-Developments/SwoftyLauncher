#pragma once
#include "core/hud/arm_policy.hpp"
#include "core/render/d3d12_hooks.hpp"

#include <atomic>
#include <mutex>
#include <vector>

namespace render::d3d12 {

// The command queues ExecuteCommandLists runs on, read while a swapchain made before the creation hooks waits
// for its present queue.
class QueueWatch {
public:
    void count(ID3D12CommandQueue *q);
    bool recent_direct(DWORD tid, hud::arm_policy::ExecSeen &out);
    void want() { wanted_on.store(true, std::memory_order_relaxed); }
    bool wanted() const { return wanted_on.load(std::memory_order_relaxed); }

private:
    std::mutex lock;
    std::vector<hud::arm_policy::ExecSeen> seen;
    std::atomic<bool> wanted_on{false};
};

extern QueueWatch queue_watch;

bool exec_hooked();

}
