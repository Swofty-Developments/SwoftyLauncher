#include "core/runtime/pages.hpp"
#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"

#include <swofty/game.hpp>

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>

namespace frontend::main_menu {

namespace {

using namespace swofty::game;

}

bool Navigation::entry_point_writable(uintptr_t at)
{
    if (at && entry_point_checked.load() == at)
        return entry_point_rw.load();
    bool ok = at && runtime::pages::writable_in_region(at, 4);
    entry_point_rw = ok;
    entry_point_checked = at;
    return ok;
}

void Navigation::clear_entry_point(const char *why)
{
    uintptr_t at = entry_point_addr.load();
    if (!at && !entry_point_told.load()) {
        at = swofty::api->resolve("g_Game_F1WorldEntryPoint");
        entry_point_addr = at;
    }
    auto entry = reinterpret_cast<uint32_t *>(at);
    switch (entry_point_write(at, at && entry_point_writable(at))) {
    case EntryPointWrite::write:
        *entry = 0;
        return;
    case EntryPointWrite::unresolved:
        if (!entry_point_told.exchange(true))
            swofty::log("[main-menu] back: g_Game_F1WorldEntryPoint is not resolved on this build (%s); the F1 World entry point "
                        "keeps the game's value (%s)",
                        runtime::build_distribution().c_str(), why);
        return;
    case EntryPointWrite::unwritable:
        if (!entry_point_told.exchange(true))
            swofty::log("[main-menu] back: g_Game_F1WorldEntryPoint at base+0x%llx is not writable memory; the F1 World entry "
                        "point keeps the game's value (%s)",
                        (unsigned long long)(at - swofty::api->module_base()), why);
        return;
    }
}

void Navigation::leave_picker_to_home(const char *why)
{
    clear_entry_point(why);
    {
        std::lock_guard<std::mutex> g(swap_lock);
        swap_screens.clear();
        swap_reported = false;
    }
    swap_at = GetTickCount64();
    reset_transition_state(BACK_HOME_FROM_CONTAINER);
    expect_screen(Expect::home, why);
    nav.return_armed = false;
    swofty::log("[main-menu] back: the game's handler fired route f1_world from its own view model (group 0x59ba71d1); it is "
                "replaced by swofty_back_home in the same group, so the F1 World hub is never opened (picker Back: %s)",
                why);
    fire_in_group("f1_world_activity_container", BACK_HOME_FROM_CONTAINER, why);
}

void Navigation::saves_back_tabs()
{
    if (saves_back_at.load() && !saves_tabs_at.load())
        saves_tabs_at = GetTickCount64();
}

void Navigation::watch_saves_back(uint64_t now)
{
    uint64_t back_at = saves_back_at.load(), tabs_at = saves_tabs_at.load();
    if (!back_at)
        return;
    if (tabs_at && now - tabs_at >= SAVES_BACK_DELAY_MS) {
        saves_back_at = 0;
        saves_tabs_at = 0;
        swofty::log(
            "[main-menu] back: the activity container is up %llu ms after the saves list's Back; leaving it for the main menu",
            (unsigned long long)(now - back_at));
        leave_picker_to_home("saves list Back");
    } else if (!tabs_at && now - back_at > SAVES_BACK_TIMEOUT_MS) {
        saves_back_at = 0;
        swofty::log("[main-menu] back: no activity container came up within %llu ms of the saves list's Back; the player "
                    "continues by hand",
                    (unsigned long long)SAVES_BACK_TIMEOUT_MS);
    }
}

void Navigation::hub_back()
{
    if (uint64_t at = hub_at.load(); at && GetTickCount64() - at >= 60) {
        hub_at = 0;
        void *router = last_router.load();
        if (router && h_fire) {
            uint8_t *scratch = route_events.back;
            *reinterpret_cast<uint32_t *>(scratch + 0x250) = djb2("fe_f1_world");
            NeStr *route = nullptr;
            Ne_Make_StrFromCStr(&route, "back", 0, 0);
            {
                std::lock_guard<std::mutex> g(swap_lock);
                swap_screens.clear();
                swap_reported = false;
            }
            swap_at = GetTickCount64();
            clear_entry_point("hub Back");
            swofty::log("[main-menu] back: firing route 'back' in group fe_f1_world (0x%08x), the route the hub's own Back "
                        "button fires (this build session15 reached the main menu with it)",
                        djb2("fe_f1_world"));
            h_fire.call<void>(router, scratch, &route, 0);
            Ne_Make_StrFromCStr(&route, nullptr, 0, 0);
        } else {
            swofty::log("[main-menu] back: no router was captured; Back from the hub stays stock");
        }
    }
}

void Navigation::report_swap()
{
    if (uint64_t at = swap_at.load(); at && GetTickCount64() - at > 1000) {
        std::lock_guard<std::mutex> g(swap_lock);
        if (!swap_reported) {
            swap_reported = true;
            swofty::log("[main-menu] back: 1 s after the route swap the screens opened since were: %s; the F1 World hub "
                        "(fe_f1_world) %s; fe_home %s",
                        swap_screens.empty() ? "(none)" : swap_screens.c_str(),
                        swap_screens.find("fe_f1_world") != std::string::npos ? "WAS OPENED" : "was never opened",
                        menu.home_open.load() ? "is the last front-end screen opened"
                                              : "is NOT the last front-end screen opened");
        }
    }
}

}
