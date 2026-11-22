#include "wine_test.hpp"
#include <swofty/career.hpp>
#include <swofty/events.hpp>

namespace events = swofty::events;

namespace {

void on_tick(double, void *) { probe()[38] = probe()[38] + 1; }

void on_session(const SwoftyEvent *, void *) {}

bool at_init()
{
    uint32_t token = 0;
    events::Result r = events::subscribe(SWOFTY_EVENT_SESSION_START, on_session, nullptr, &token);
    swofty::log("[t_events_early] at init: subscribe session.start %s (token %u)", events::name(r), token);
    return true;
}

}

SWOFTY_ON_LOAD
{
    bool available = events::available(), career = swofty::career::available();
    uint32_t token = 0;
    events::Result r = events::on_frame(on_tick, nullptr, &token);
    probe()[39] = (available ? 1 : 0) | (r == events::Result::ok ? 2 : 0) | (career ? 4 : 0);
    swofty::log("[t_events_early] at load: events available %d, on_frame %s (token %u), career available %d", available,
                events::name(r), token, career);
}

SWOFTY_ADDON(at_init)
