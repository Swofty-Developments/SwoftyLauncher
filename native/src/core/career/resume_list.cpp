#include "core/career/resume_list.hpp"

#include <windows.h>

#include <atomic>
#include <cstring>
#include <mutex>

namespace career::resume_list {

namespace {

std::atomic<bool> on{false};
std::mutex mu;
bool target_on = false, list_on = false, target_team = false;
Selection target_sel;
std::vector<Row> rows_;

}

bool wanted()
{
    static const bool v = [] {
        char e[16] = {};
        return GetEnvironmentVariableA("SWOFTY_RESUME_LIST", e, sizeof e) && !strcmp(e, "1");
    }();
    return v;
}

bool armed() { return on.load(); }

void set_armed(bool v) { on = v; }

bool target_set()
{
    std::lock_guard<std::mutex> g(mu);
    return target_on;
}

void set_target(const Selection &sel, bool my_team)
{
    std::lock_guard<std::mutex> g(mu);
    target_on = true;
    target_sel = sel;
    target_team = my_team;
}

bool take_target(bool my_team, Selection &sel)
{
    std::lock_guard<std::mutex> g(mu);
    bool match = target_on && target_team == my_team;
    sel = target_sel;
    target_on = false;
    return match;
}

void begin_list(std::vector<Row> rows)
{
    std::lock_guard<std::mutex> g(mu);
    rows_ = std::move(rows);
    list_on = true;
}

bool take_list()
{
    std::lock_guard<std::mutex> g(mu);
    bool v = list_on;
    list_on = false;
    return v;
}

std::vector<Row> shown()
{
    std::lock_guard<std::mutex> g(mu);
    return rows_;
}

}
