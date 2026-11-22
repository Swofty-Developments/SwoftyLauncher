#pragma once
#include "core/frontend/main_menu_logic.hpp"
#include "core/hud/hud_drawlist.hpp"

#include <safetyhook.hpp>
#include <swofty/game.hpp>

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>

namespace frontend::main_menu {

class Navigation {
public:
    bool entry_by_route();
    void install_hooks();
    swofty::game::Action *card_pressed(swofty::game::HomeItem *item, swofty::game::Action *out);
    void start_chain(bool time_trial);
    void watch();
    void hub_back();
    void report_swap();
    void tick_chain();
    void session_enter();
    void screen(const char *name);
    bool covering() const;
    void paint_cover(hud::drawlist::Frame &f);

    uint64_t container_command(void *container, swofty::game::NeStr **message, void *third);
    void tabs_added(void *vm);
    uint64_t went_home(void *vm, uint8_t state);
    swofty::game::NeStr **migrated(void *cmd, swofty::game::NeStr **out, void *node);
    void route_fired(void *router, void *vm, swofty::game::NeStr **name, uint64_t a4);
    void hook_container();

private:
    enum class Step { idle, wait_container, done };
    struct Chain {
        Step step = Step::idle;
        const char *command = "";
        uint64_t started = 0, container_at = 0;
        bool handover = false;
    };
    struct NavState {
        std::atomic<int> mode{int(Mode::none)}, expect{int(Expect::none)};
        std::atomic<uint64_t> expect_at{0}, return_at{0};
        std::atomic<bool> was_in_session{false}, return_armed{false};
    };

    bool nav_flag();
    void gp_saves_probe();
    Back back_mode();
    std::string get_flow();
    void set_flow(const std::string &f);
    void expect_screen(Expect e, const char *why);
    void reset_transition_state(const char *why);
    uint64_t raise_delay_ms();
    bool entry_point_writable(uintptr_t at);
    void clear_entry_point(const char *why);
    void leave_picker_to_home(const char *why);
    void saves_back_tabs();
    void cover_up(const char *why);
    void cover_down(const char *picker);
    void raise_command(void *vm, const char *command, const char *where);
    void apply_pending(void *vm, const char *where);
    bool fire_in_group(const char *group, const char *route, const char *why);
    void hook_fire();
    void hook_migrate();
    void hook_addtabs();
    void hook_gohome();
    void recover(Expect e);
    void watch_cover();
    void watch_saves_back(uint64_t now);

    Chain chain;
    std::mutex chain_lock;
    std::mutex container_hook_lock;
    NavState nav;
    std::mutex flow_lock;
    std::string flow_now = FRONTEND_FLOW;
    safetyhook::InlineHook h_container;
    std::atomic<void *> container_vm{nullptr}, container_third{nullptr};
    std::atomic<uint64_t> container_vm_at{0};

    // Clears g_Game_F1WorldEntryPoint (how F1 World was entered) so the hub is not reopened after a Back. The address comes from
    // the running build's binding set, never from a fixed offset: a build whose set does not resolve it keeps the game's
    // own value and says so once.
    std::atomic<uintptr_t> entry_point_addr{0};
    std::atomic<bool> entry_point_told{false};

    // The entry point is a global in the game image, whose protection never changes: it is queried once per address.
    std::atomic<uintptr_t> entry_point_checked{0};
    std::atomic<bool> entry_point_rw{false};

    std::atomic<uint64_t> saves_back_at{0}, saves_tabs_at{0};
    std::atomic<bool> cover_on{false};
    std::atomic<uint64_t> cover_since{0}, cover_frames{0}, cover_applied_frames{0}, applied_at{0}, frame_no{0}, open_frame{0};
    safetyhook::InlineHook h_gohome, h_addtabs;
    safetyhook::InlineHook h_migrate;
    safetyhook::InlineHook h_fire;
    std::atomic<uint64_t> swap_at{0};
    std::atomic<void *> last_router{nullptr};
    std::atomic<uint64_t> hub_at{0};
    std::mutex swap_lock;
    std::string swap_screens;
    bool swap_reported = true;
    std::atomic<int> nav_flag_state{-1};
    std::atomic<bool> gp_probe_asked{false};
    // The event blocks the route fires and the container command pass the game, one per caller.
    struct RouteEvents {
        uint8_t back[0x400], reopen[0x400], fire[0x400], third[0x100];
    };
    RouteEvents route_events{};
};

}
