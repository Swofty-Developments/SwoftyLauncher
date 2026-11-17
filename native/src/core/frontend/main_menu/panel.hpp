#pragma once
#include "core/frontend/main_menu_logic.hpp"
#include "core/hud/hud_drawlist.hpp"

#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>

namespace runtime::main_menu {

class Panel {
public:
    void read_state(bool force = false);
    State snapshot();
    bool account(std::string &name);
    bool due();
    void want_strip(int visible);
    void home_tick();
    void sync_strip();
    void paint(hud::Frame &f, uint64_t now);

private:
    struct Prepared {
        State s;
        int avatar_slot = -1, video_slot = -1;
        uint32_t video_w = 0, video_h = 0;
        std::string version, discord, watch;
    };

    void prepare();
    void poll_bind();

    std::atomic<bool> state_loaded{false}, panel_disabled{false}, panel_prepared{false}, bind_pad{false}, overlay_drawn{false};
    std::mutex prepared_lock;
    std::shared_ptr<const Prepared> prepared;
    uint64_t prepared_at = 0;
    std::string prepared_key;
    std::atomic<int> strip_wanted{-1}, strip_set{-1};
    std::mutex state_lock;
    State state;
    uint64_t state_read_at = 0;
};

}
