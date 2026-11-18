#pragma once
#include "core/frontend/view_state.hpp"
#include "core/live/live_car.hpp"
#include "core/render/postfx/pass.hpp"
#include "core/render/postfx/rule_logic.hpp"

#include "swofty_api.h"

#include <windows.h>

#include <atomic>
#include <string>
#include <vector>

namespace render::postfx {

// The shader config's rules: the signals they read (the view, the player's car, the session's events), the
// rule engine that turns them into each instance's options and on/off state, and the log of what fired.
class RuleDriver {
public:
    // Starts the rules of a new config over; instance passes come back on.
    void reset(const ActiveSpec &active, const Plan &plan, std::vector<RenderPass> &passes);
    // Reads the player's car when a rule or the live car trace (swofty/shaders/live.trace) wants it.
    void live_frame(ULONGLONG now, const std::wstring &swofty_root);
    // Steps the rules with this frame's signals and writes the instance passes' options and on/off state.
    void frame(const frontend::view::State &v, ULONGLONG now, const ActiveSpec &active, const Plan &plan,
               std::vector<RenderPass> &passes);
    // Feeds the session's events into the rules.
    void subscribe(const SwoftyEvents *ev);

private:
    struct RuleLog {
        bool on = false;
        ULONGLONG since = 0;
        unsigned count = 0;
    };
    struct Feed {
        RuleDriver *driver;
        int signal;
    };

    static void on_event(const SwoftyEvent *ev, void *user);
    void live_trace_tick(ULONGLONG now, const std::wstring &swofty_root);
    bool live_trace_on(ULONGLONG now) const;
    void live_trace_line(ULONGLONG now, const live_car::LiveCar &c);
    bool player_car(int64_t index);

    RuleEngine engine;
    std::vector<RuleLog> rule_log;
    ULONGLONG loaded_at = 0, minute_at = 0;
    LARGE_INTEGER clock{};
    std::atomic<unsigned> pending_events[SIGNAL_COUNT];
    std::atomic<bool> in_session{false};
    bool reads_car = false;
    live_car::LiveCar live_now;
    bool live_ok = false;
    ULONGLONG live_trace_checked = 0, live_trace_until = 0, live_trace_line_at = 0;
    int live_trace_surface[4] = {-2, -2, -2, -2};
    uintptr_t car_table = 0, car_count = 0;
    bool car_tried = false;
    Feed feeds[4] = {{this, S_SESSION_START}, {this, S_RACE_START}, {this, S_SESSION_LAP}, {this, S_SESSION_END}};
};

}
