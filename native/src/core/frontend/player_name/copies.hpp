#pragma once
#include "core/frontend/player_name_logic.hpp"
#include "core/frontend/ui_registry_walk.hpp"

#include <atomic>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>

namespace name_copies {

// The view model registry's node vtables in the KB image (the TT name gate's proof names them).
inline runtime::ui_registry::Layout registry_layout(uint64_t base)
{
    return {base + 0x5a70080, base + 0x5a6ff90, base + 0x565f200, base + 0x5660370};
}

bool registry_read(uint64_t at, void *out, size_t n);
void probe_registry(int pass);
void scan_copies(std::string stock);
int pin_names(const std::string &name, const std::string &stock);

class SourceSwap {
public:
    void plan_swap(bool live_ours, const std::string &stock, const std::string &name, const char *source, bool visible);

private:
    struct SwapJob {
        uint64_t serial = 0;
        std::string stock, name;
        const char *source = "";
        runtime::player_name::SwapWhy why = runtime::player_name::SwapWhy::none;
    };

    void swap_worker();

    std::mutex swap_lock;
    std::condition_variable swap_cv;
    SwapJob swap_job;
    bool swap_queued = false;
    std::atomic<bool> swap_busy{false};
    runtime::player_name::SwapPlan swap_plan;
    std::once_flag swap_thread_once;
};

extern SourceSwap source_swap;

}
