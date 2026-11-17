#pragma once
#include "core/frontend/player_name_logic.hpp"
#include "core/frontend/ui_registry_walk.hpp"
#include "core/runtime/build_proof.hpp"

#include <atomic>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>

namespace name_copies {

namespace bp = ::runtime::build_proof;

inline constexpr bp::Proof NAME_FUNCTIONS{"the presence name lookup, the name copy functions and the UI registry node classes",
                                          bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
inline constexpr bp::Gate NAME_BRIDGE{"the Time Trial player name bridge", &NAME_FUNCTIONS};

bool registry_read(uint64_t at, void *out, size_t n);
// The UI registry layout on the running exe; false when its node classes are not found there.
bool registry_layout(runtime::ui_registry::Layout &lay);
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
