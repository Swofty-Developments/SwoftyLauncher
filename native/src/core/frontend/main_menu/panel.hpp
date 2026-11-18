#pragma once
#include "core/frontend/main_menu_logic.hpp"
#include "core/hud/hud_drawlist.hpp"

#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>

namespace frontend::main_menu {

// The video card's watch bind, pad Y or the key. XInputGetState and the key sweep can block for about a second each under
// Wine, so they are sampled on a thread of its own while the flow thread asks, and the flow thread only reads the result.
class BindSampler {
public:
    bool poll(uint64_t now);
    bool on_pad() const { return pad.load(); }

private:
    void run();
    bool pressed_edge(bool *on_pad);

    void *xinput_get = nullptr;
    bool xinput_tried = false;
    bool was = false;
    uint32_t last_packet = 0;
    uint64_t pad_at = 0, key_at = 0;
    bool shown_pad = false;
    uint64_t slow = 0;
    std::atomic<bool> pressed{false}, started{false}, pad{false};
    std::atomic<uint64_t> wanted_at{0};
};

class Panel {
public:
    void read_state(bool force = false);
    State snapshot();
    bool account(std::string &name);
    bool due();
    void want_strip(int visible);
    void home_tick();
    void sync_strip();
    void paint(hud::drawlist::Frame &f, uint64_t now);

private:
    struct Prepared {
        State s;
        int avatar_slot = -1, video_slot = -1;
        uint32_t video_w = 0, video_h = 0;
        std::string version, discord, watch;
    };

    void set_offline_text(long long online);
    void prepare();
    void poll_bind();

    std::atomic<bool> state_loaded{false}, panel_disabled{false}, panel_prepared{false}, overlay_drawn{false};
    std::mutex prepared_lock;
    std::shared_ptr<const Prepared> prepared;
    uint64_t prepared_at = 0;
    std::string prepared_key;
    std::atomic<int> strip_wanted{-1}, strip_set{-1};
    std::mutex state_lock;
    State state;
    uint64_t state_read_at = 0;
    long long offline_shown = -1;
    int offline_fails = 0;
    uint64_t slow_ticks = 0;
    int strip_tries = 0;
    uint64_t bind_polled_at = 0;
    BindSampler bind;
    bool told_first = false, told_blocked = false, told_noprep = false, told_drawn = false, told_verts = false;
};

}
