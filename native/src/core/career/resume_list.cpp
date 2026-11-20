#include "core/career/resume_list.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>
#include <swofty/ui.hpp>

#include <atomic>
#include <cstring>
#include <mutex>

#include "core/career/content_state.hpp"
#include "core/career/resume_press_logic.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

// The Resume Career list screen (on unless SWOFTY_RESUME_LIST=0). The launcher builds the screen swofty_resume_list at Play, with literal
// text, and writes `resume_list` into settings.json (resume_list_logic.hpp). The Career menu's Resume Career press (route
// solo_career_continue_driver or _my_team) is rewritten to the route swofty_resume_list (content_hooks.cpp). A row press
// raises `swofty_resume:<index>` to this view model, which maps the row to its save slot and keeps it as the target; the same
// press routes the screen to the stock continue screen of the row's mode, and when that screen finishes its transition in
// the resume gate (resume_gate_guard.cpp) rewrites card 0 to the target and loads it, after a Load Anyway / Back question
// for a row that is not ready. A row that cannot load (its own team, car or driver row is missing) has no route in the
// launcher's page: its press shows why on the list screen and selects nothing.

namespace career::resume_list {

namespace {

std::atomic<bool> on{false};
std::mutex mu;
bool table_read = false, target_on = false;
std::vector<Entry> table;
Target target_;
uint64_t target_at = 0, new_career_at = 0, press_at = 0;
std::string press_state;

const std::vector<Entry> &entries_locked()
{
    if (!table_read) {
        table_read = true;
        table = parse(runtime::settings_json_text());
        swofty::log("[career] resume screen: settings.json lists %zu row(s) for the list screen", table.size());
    }
    return table;
}

class ResumePage : public swofty::ui::ViewModel {
public:
    void on_activated() override
    {
        std::lock_guard<std::mutex> g(mu);
        new_career_at = 0;
        swofty::log("[career] resume screen: %s is open with %zu row(s)", screen().c_str(), entries_locked().size());
    }

    bool on_command(const swofty::ui::Command &c) override
    {
        size_t index = 0;
        if (!c.name || !command_index(c.name, index))
            return false;
        swofty::log("[career] resume screen: command '%s' raised by a row press", c.name);
        pressed(index);
        return true;
    }
};

SWOFTY_VIEW_MODEL("swofty_resume_", ResumePage)

}

bool wanted()
{
    static const bool v = [] {
        char e[16] = {};
        return !(GetEnvironmentVariableA("SWOFTY_RESUME_LIST", e, sizeof e) && !strcmp(e, "0"));
    }();
    return v;
}

bool armed() { return on.load(); }

void set_armed(bool v) { on = v; }

size_t rows()
{
    std::lock_guard<std::mutex> g(mu);
    return entries_locked().size();
}

bool pressed(size_t index)
{
    std::lock_guard<std::mutex> g(mu);
    const Entry *e = find(entries_locked(), index);
    if (!e) {
        swofty::log("[career] resume screen: row %zu is not in settings.json's resume_list; nothing was selected", index);
        return false;
    }
    if (press(*e) == Action::Refuse) {
        swofty::log("[career] resume screen: row %zu (%s, key %s) cannot load (%s); nothing is selected and the list says why",
                    index, e->my_team ? "My Team" : "Driver Career", e->key.c_str(), e->status.c_str());
        runtime::popup_show(REFUSE_TITLE, refuse_text(*e), "swofty_core");
        return false;
    }
    Selection sel;
    if (!select(runtime::career_listed_slots(), *e, sel)) {
        swofty::log("[career] resume screen: row %zu (%s, key %s): no such career in the save slots the core listed; the stock "
                    "continue screen opens without a selection",
                    index, e->my_team ? "My Team" : "Driver Career", e->key.c_str());
        return false;
    }
    target_on = true;
    target_at = GetTickCount64();
    std::string group =
        group_choice(e->group, career::content::group_of_mode(e->game_mode), runtime::career_saves_content_for_slot(e->key));
    target_ = {sel,
               e->my_team,
               press(*e) == Action::Ask,
               index,
               dialog_text(*e),
               group,
               suffix_for(group, career::content::suffix_of_group(group)),
               e->game_mode};
    swofty::log("[career] resume screen: row %zu (%s, key %s) is slot id %llu guid %016llx, %s%s%s; the %s continue screen "
                "loads it next",
                index, e->my_team ? "My Team" : "Driver Career", e->key.c_str(), (unsigned long long)sel.id,
                (unsigned long long)sel.guid, e->ready ? "ready" : "not ready (", e->ready ? "" : e->status.c_str(),
                e->ready ? "" : ") and asks first", e->my_team ? "My Team" : "Driver Career");
    return true;
}

void route_ran(const std::string &route)
{
    if (!is_new_career_route(route))
        return;
    std::lock_guard<std::mutex> g(mu);
    new_career_at = GetTickCount64();
    swofty::log("[career] resume screen: New Career was chosen on the list screen; the next Resume Career tile press opens the "
                "stock continue screen");
}

bool new_career_requested()
{
    std::lock_guard<std::mutex> g(mu);
    if (!new_career_live(new_career_at, GetTickCount64()))
        return false;
    new_career_at = 0;
    return true;
}

void press_noted(const std::string &state)
{
    std::lock_guard<std::mutex> g(mu);
    press_at = GetTickCount64();
    press_state = state;
}

bool press_recent(std::string &state)
{
    std::lock_guard<std::mutex> g(mu);
    if (!setup_after_press(SETUP_FLOW, press_at, GetTickCount64()))
        return false;
    state = press_state;
    return true;
}

bool target_set()
{
    std::lock_guard<std::mutex> g(mu);
    return target_on && !expired(target_at, GetTickCount64());
}

bool take_target(bool my_team, Target &t)
{
    std::lock_guard<std::mutex> g(mu);
    bool live = target_on && !expired(target_at, GetTickCount64());
    bool match = live && target_.my_team == my_team;
    if (target_on && !live)
        swofty::log("[career] resume screen: the selection for row %zu expired before a continue screen opened; dropped",
                    target_.row);
    t = target_;
    target_on = false;
    return match;
}

}
