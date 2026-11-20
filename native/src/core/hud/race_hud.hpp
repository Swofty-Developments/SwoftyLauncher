#pragma once
#include "core/hud/hud_leaves_logic.hpp"

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

// The race HUD's sets and registered leaves behind the SwoftyCoreHud export (swofty/hud.hpp, swofty/hud_leaves.hpp):
// writes into the game's hud_* view model leaves, queued from any thread and applied on the flow tick. Its toasts go
// through the popup message system (frontend/popup.cpp).
namespace hud::race_hud {

struct Set {
    std::string path, text, by;
    int type;
    int32_t number;
};

struct RaceHud {
    explicit RaceHud(std::mutex &queue_lock) : queue_lock(queue_lock) {}

    int set_enqueue(std::string path, int type, std::string text, int32_t number, const std::string &by);
    void read_switches();
    void tick();

    // The popup message system's queue lock (frontend::popup::messages.queue_lock), which the set queue shares.
    std::mutex &queue_lock;
    std::deque<Set> sets;
    std::atomic<size_t> sets_pending{0};
    std::atomic<uint32_t> sets_dropped{0};
    hud::leaves::Table leaf_table;
    // The first 16 applied sets and unresolved leaves are logged, and the first leaf registration.
    std::atomic<int> sets_told{0}, leaves_told{0};
    std::atomic<bool> leaves_logged{false};

private:
    struct EpochWatch {
        uintptr_t epoch = 0;
        std::vector<std::string> parents;
        std::vector<uintptr_t> tokens;
        size_t next = 0, known = 0;
    };

    void leaves_watch(uint64_t now);
    void set_test_tick();
    void dev_set_parse(const std::string &spec);
    void dev_set_tick();

    std::string set_test_text;
    uint64_t set_test_next = 0;
    std::vector<Set> dev_sets;
    EpochWatch epoch_watch;
    uint64_t epoch_next_check = 0, dev_set_next = 0;
};

extern RaceHud state;

}
