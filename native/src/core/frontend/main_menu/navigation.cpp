#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/apply/moments.hpp"

#include <swofty/game.hpp>

#include <atomic>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>

namespace frontend::main_menu {

namespace {

using namespace swofty::game;

const char *route_name()
{
    const char *r = env("SWOFTY_MAIN_MENU_ROUTE");
    return *r ? r : "home_to_activity";
}

// The function's first 64 bytes are shared with a second lookup (0x1407c7280), so the pattern is taken from the first
// byte that differs (function + 0x31c: the lea/call displacements are wildcards, the short jump after them is not) and
// the anchor walks back to the entry.
const runtime::Sig S_MODE_BY_NAME{"Game_GameModes_FindByName",
                                  "44 24 5E C6 00 00 48 8D 05 ?? ?? ?? ?? 48 89 44 24 28 48 8D 54 24 28 E8 ?? ?? ?? ?? 48 C1 E0 "
                                  "03 48 03 46 58 48 3B 46 60 74 65 48 8B",
                                  0x7d0de0,
                                  runtime::Sig::addr,
                                  0,
                                  uintptr_t(0) - 0x31c};
const runtime::Sig S_GAME_MANAGER{"game.manager",
                                  "48 8B B8 98 01 00 00 48 85 FF 0F 84 ?? ?? ?? ?? 48 8D 4C 24 70 E8 ?? ?? ?? ?? 48 8B 05",
                                  0x604c2c8,
                                  runtime::Sig::riprel,
                                  26,
                                  29,
                                  7};

// The championship modes the Grand Prix season probe asks, as name addresses in the KB image.
constexpr uintptr_t GP_MODE_NAMES[] = {0x5734370, 0x573438c, 0x57343a8, 0x570a6b2, 0x570a6c9};

}

bool Navigation::nav_flag()
{
    int f = nav_flag_state.load();
    if (f < 0) {
        f = runtime::read_file(runtime::under_swofty("ui\\home_nav.flag"), 16).empty() ? 0 : 1;
        nav_flag_state = f;
        swofty::log("[main-menu] nav: the launcher's Time Trial, Grand Prix and Back routes %s",
                    f ? "are in this build" : "are not in this build; the old chain and the game's Back stay");
    }
    return f == 1;
}

// The stock Grand Prix entry (grand_prix_continue) shows its saved seasons when GrandPrixContinueViewModel's mode
// lookup (Game_GrandPrixContinueVm_FindModeEntity 0x141bccc00) finds a championship mode whose season registry
// (entity +0x1030, virtual +0x78) answers 1: F2_2026/F2_2025/F2_2024_Championship_Season, F1_Championship_Season,
// F1_2026_Championship_Season, in that order. The card does not decide on this lookup any more: in the field it found
// no entity for any name, F1_Championship_Season included, so a card deciding on it never opened the saves screen.
// It is asked once per session and logged (the modes pointer and every answer) so the field says which step fails.
void Navigation::gp_saves_probe()
{
    if (gp_probe_asked.exchange(true))
        return;
    uintptr_t find = runtime::find(S_MODE_BY_NAME, "home", "core"), manager = runtime::find(S_GAME_MANAGER, "home", "core");
    if (!find || !manager) {
        swofty::log("[main-menu] probe: the Grand Prix season lookup did not resolve (mode lookup %p, game manager %p)",
                    reinterpret_cast<void *>(find), reinterpret_cast<void *>(manager));
        return;
    }
    void *mgr = *reinterpret_cast<void **>(manager);
    if (!mgr || !runtime::readable(static_cast<uint8_t *>(mgr) + 0xc0, 8)) {
        swofty::log("[main-menu] probe: the game manager %p is not readable", mgr);
        return;
    }
    void *modes = *reinterpret_cast<void **>(static_cast<uint8_t *>(mgr) + 0xc0);
    uintptr_t base = swofty::api->module_base();
    swofty::log("[main-menu] probe: game manager %p, game modes %p, mode lookup base+0x%llx", mgr, modes,
                (unsigned long long)(find - base));
    if (!modes || !runtime::readable(modes, 8))
        return;
    for (uintptr_t rva : GP_MODE_NAMES) {
        const char *name = reinterpret_cast<const char *>(base + rva);
        if (!runtime::readable(name, 8) || std::strlen(name) > 40 || std::strncmp(name, "F", 1)) {
            swofty::log("[main-menu] probe: mode name at base+0x%llx is not a name", (unsigned long long)rva);
            continue;
        }
        auto entity = reinterpret_cast<uint8_t *>(reinterpret_cast<void *(*)(void *, const char *)>(find)(modes, name));
        if (!entity) {
            swofty::log("[main-menu] probe: Grand Prix mode %s: no entity", name);
            continue;
        }
        if (!runtime::readable(entity + 0x1030, 8)) {
            swofty::log("[main-menu] probe: Grand Prix mode %s: entity %p has no readable season registry", name, entity);
            continue;
        }
        void **vt = *reinterpret_cast<void ***>(entity + 0x1030);
        if (!vt || !runtime::readable(vt + 0xf, 8)) {
            swofty::log("[main-menu] probe: Grand Prix mode %s: entity %p, registry vtable %p unreadable", name, entity, vt);
            continue;
        }
        int answer = reinterpret_cast<int (*)(void *)>(vt[0x78 / 8])(entity + 0x1030);
        swofty::log("[main-menu] probe: Grand Prix mode %s: entity %p, season registry answers %d", name, entity, answer);
    }
}

bool Navigation::entry_by_route() { return nav_flag() && std::strcmp(env("SWOFTY_MAIN_MENU_ENTRY"), "chain") != 0; }

Back Navigation::back_mode() { return back_variant_of(env("SWOFTY_MAIN_MENU_BACK"), nav_flag()); }

std::string Navigation::get_flow()
{
    std::lock_guard<std::mutex> g(flow_lock);
    return flow_now;
}

void Navigation::set_flow(const std::string &f)
{
    std::lock_guard<std::mutex> g(flow_lock);
    flow_now = f;
}

void Navigation::expect_screen(Expect e, const char *why)
{
    nav.expect = int(e);
    nav.expect_at = GetTickCount64();
    swofty::log("[main-menu] nav: expecting %s (%s); flow now %s",
                e == Expect::home    ? "fe_home"
                : e == Expect::saves ? "grand_prix_continue"
                                     : "f1_world_activity_container",
                why, get_flow().c_str());
}

void Navigation::reset_transition_state(const char *why)
{
    if (off("SWOFTY_MAIN_MENU_STATEFIX"))
        return;
    bool ok = ui_vm_set_path("ui_menu_data.frontend_transition_state", 0, "main_menu");
    swofty::log("[main-menu] nav: #ui_menu_data.frontend_transition_state set to main_menu before %s: %s", why,
                ok ? "done" : "could not be set");
}

uint64_t Navigation::raise_delay_ms()
{
    const char *v = std::getenv("SWOFTY_MAIN_MENU_RAISE_MS");
    return v && *v ? uint64_t(std::atoll(v)) : 300;
}

void Navigation::cover_up(const char *why)
{
    if (std::strcmp(env("SWOFTY_MAIN_MENU_COVER"), "1") != 0)
        return;
    cover_since = GetTickCount64();
    cover_frames = 0;
    applied_at = 0;
    cover_on = true;
    swofty::log("[main-menu] nav: cover up (%s) at frame %llu", why, (unsigned long long)frame_no.load());
}

void Navigation::cover_down(const char *picker)
{
    if (!cover_on.exchange(false))
        return;
    swofty::log("[main-menu] nav: picker %s open; cover released after %llu ms / %llu frames", picker,
                (unsigned long long)(GetTickCount64() - cover_since.load()), (unsigned long long)cover_frames.load());
}

bool Navigation::covering() const { return cover_on.load(); }

void Navigation::paint_cover(hud::drawlist::Frame &f)
{
    frame_no++;
    if (cover_on.load()) {
        hud::drawlist::reset_state(f);
        hud::drawlist::rect(f, 0, 0, f.width, f.height, 0x000000ffu);
        cover_frames++;
    }
}

Action *Navigation::card_pressed(HomeItem *item, Action *out)
{
    if (uint64_t at = menu.home_opened_at.load(); at && GetTickCount64() - at < 700) {
        out->type = 0;
        out->str = nullptr;
        swofty::log("[main-menu] %s: press ignored %llu ms after fe_home opened (carried-over input)", id_of(item),
                    (unsigned long long)(GetTickCount64() - at));
        return out;
    }
    bool gp = !std::strcmp(id_of(item), ID_GRAND_PRIX);
    const char *route = route_name();
    if (entry_by_route()) {
        if (gp)
            gp_saves_probe();
        Mode m = gp ? Mode::grand_prix : Mode::time_trial;
        nav.mode = int(m);
        nav.return_armed = false;
        route = press_route(m);
        {
            std::lock_guard<std::mutex> g(chain_lock);
            chain = {};
            chain.step = Step::wait_container;
            chain.command = return_command(m);
            chain.started = chain.container_at = GetTickCount64();
            container_vm = nullptr;
        }
        expect_screen(Expect::container, route);
        swofty::log("[main-menu] chain: %s card pressed; route %s opens the activity container, then '%s' is raised on it (%s)",
                    gp ? "Grand Prix" : "Time Trial", route, return_command(m),
                    m == Mode::time_trial ? "the Time Trial picker"
                                          : "the container's own Grand Prix command: the saved seasons and New Grand Prix");
    } else {
        menu.via = 1;
        swofty::log("[main-menu] %s card pressed: route %s (the old chain)", id_of(item), route);
    }
    out->type = 1;
    out->str = nullptr;
    Ne_Make_StrFromCStr(&out->str, route, 0, 0);
    return out;
}

void Navigation::start_chain(bool time_trial)
{
    if (!std::strcmp(env("SWOFTY_MAIN_MENU_CHAIN"), "0")) {
        swofty::log("[main-menu] chain off (SWOFTY_MAIN_MENU_CHAIN=0); the card opens the activity container");
        return;
    }
    std::lock_guard<std::mutex> g(chain_lock);
    chain = {};
    chain.step = Step::wait_container;
    if (!time_trial)
        gp_saves_probe();
    Mode m = time_trial ? Mode::time_trial : Mode::grand_prix;
    nav.mode = int(m);
    chain.command = return_command(m);
    chain.started = GetTickCount64();
    container_vm = nullptr;
    swofty::log("[main-menu] chain: %s card pressed; route %s, then the activity container's %s command",
                time_trial ? "Time Trial" : "Grand Prix", route_name(), chain.command);
}

void Navigation::recover(Expect e)
{
    std::string flow = get_flow();
    switch (recovery_for(e, flow, menu.home_open.load())) {
    case Recovery::none:
        swofty::log("[main-menu] nav: the expected screen did not open but the main menu is up; nothing to recover");
        break;
    case Recovery::back_home_route:
        swofty::log("[main-menu] nav: the expected screen did not open within 4 s and the %s flow is current; migrating back to "
                    "the main menu so the player is never left without a screen",
                    flow.c_str());
        reset_transition_state("the recovery");
        if (fire_in_group("f1_world_activity_container", BACK_HOME_FROM_CONTAINER, "recovery"))
            expect_screen(Expect::home, "recovery");
        break;
    case Recovery::open_home:
        swofty::log("[main-menu] nav: the expected screen did not open within 4 s in the %s flow; reopening the main menu screen "
                    "so the player is never left without a screen",
                    flow.c_str());
        fire_in_group("fe_home", "open", "recovery");
        break;
    }
}

void Navigation::watch_cover()
{
    if (!cover_on.load())
        return;
    uint64_t now = GetTickCount64();
    if (uint64_t at = applied_at.load(); at && cover_frames.load() >= cover_applied_frames.load() + 3 && now - at >= 120) {
        cover_down("tab of the activity container (dynamic_era_select)");
        return;
    }
    if (now - cover_since.load() > 3000) {
        bool applied = applied_at.load() != 0;
        swofty::log("[main-menu] nav: cover safety timeout after 3 s (command %s)",
                    applied ? "was applied" : "was never applied");
        cover_down("(timeout)");
        if (!applied && !off("SWOFTY_MAIN_MENU_RECOVER"))
            recover(Expect::container);
    }
}

void Navigation::watch()
{
    watch_cover();
    uint64_t now = GetTickCount64();
    watch_saves_back(now);
    bool in_session = runtime::in_session_now();
    int e = nav.expect.load();
    if (e != int(Expect::none) && now - nav.expect_at.load() > 4000 && !in_session) {
        nav.expect = int(Expect::none);
        if (off("SWOFTY_MAIN_MENU_RECOVER"))
            swofty::log(
                "[main-menu] nav: the expected screen did not open within 4 s; recovery is off (SWOFTY_MAIN_MENU_RECOVER=0)");
        else
            recover(Expect(e));
    }
}

void Navigation::tick_chain()
{
    std::lock_guard<std::mutex> g(chain_lock);
    if (chain.step == Step::idle || chain.step == Step::done)
        return;
    uint64_t now = GetTickCount64();
    if (chain.step == Step::wait_container && !chain.container_at && now - chain.started > 3000) {
        void *router = last_router.load();
        chain.step = Step::done;
        if (router && h_fire) {
            uint8_t *scratch = route_events.reopen;
            *reinterpret_cast<uint32_t *>(scratch + 0x250) = djb2("fe_home");
            NeStr *route = nullptr;
            Ne_Make_StrFromCStr(&route, "open", 0, 0);
            swofty::log("[main-menu] chain: the activity container did not open within 3 s; reopening the main menu (route "
                        "'open' in group fe_home) so the player is never left without a screen");
            h_fire.call<void>(router, scratch, &route, 0);
            Ne_Make_StrFromCStr(&route, nullptr, 0, 0);
        } else {
            swofty::log("[main-menu] chain: the activity container did not open within 3 s and no router was captured to reopen "
                        "the main menu");
        }
        return;
    }
    if (now - chain.started > 20000) {
        swofty::log("[main-menu] chain: gave up at step %d after 20 s; the player continues by hand", int(chain.step));
        chain.step = Step::done;
        return;
    }
    if (chain.step == Step::wait_container && chain.container_at && container_vm.load() && now - chain.container_at > 300 &&
        now - container_vm_at.load() > raise_delay_ms()) {
        const char *command = chain.command;
        chain.step = Step::done;
        raise_command(container_vm.load(), command,
                      chain.handover ? "the hand-over: the container is active, its own first command came"
                                     : "fallback: after its view model was seen");
        chain.step = Step::done;
    }
}

void Navigation::session_enter()
{
    Mode pressed = Mode(nav.mode.load());
    if (pressed == Mode::none)
        return;
    std::string mode = swofty_core::apply::moments::session_mode_track().first;
    Mode driven = driven_mode(pressed, mode);
    if (driven == pressed)
        return;
    nav.mode = int(driven);
    const char *card = pressed == Mode::time_trial ? "Time Trial" : "Grand Prix";
    if (driven == Mode::none)
        swofty::log("[main-menu] nav: the session that loaded (game mode %s) is not the %s the card opened, nor the other card's "
                    "mode; the session-end hand-over raises no command",
                    mode.c_str(), card);
    else
        swofty::log("[main-menu] nav: the session that loaded is a %s (game mode %s), not the %s the card opened; the "
                    "session-end hand-over raises '%s'",
                    driven == Mode::time_trial ? "Time Trial" : "Grand Prix", mode.c_str(), card, return_command(driven));
}

void Navigation::screen(const char *name)
{
    if (menu.via.load() == 1 && !std::strcmp(name, "fe_f1_world") && back_mode() == Back::twostep) {
        hub_at = GetTickCount64();
        swofty::log("[main-menu] back: the stock Back opened the F1 World hub; its own Back (route 'back') follows at once");
    }
    if (uint64_t at = swap_at.load(); at && GetTickCount64() - at < 5000) {
        swofty::log("[main-menu] back: screen %s opened %llu ms after the route swap", name,
                    (unsigned long long)(GetTickCount64() - at));
        std::lock_guard<std::mutex> g(swap_lock);
        swap_screens += (swap_screens.empty() ? "" : ", ") + std::string(name);
    }
    {
        std::lock_guard<std::mutex> g(chain_lock);
        uint64_t now = GetTickCount64();
        if (chain.step != Step::idle && chain.step != Step::done)
            swofty::log("[main-menu] chain: screen %s (step %d)", name, int(chain.step));
        if (!std::strcmp(name, "f1_world_activity_container")) {
            open_frame = frame_no.load();
            if (chain.step == Step::wait_container) {
                chain.container_at = now;
                container_vm = nullptr;
            } else if (nav.return_armed.exchange(false)) {
                Mode m = Mode(nav.mode.load());
                chain = {};
                chain.step = Step::wait_container;
                chain.command = return_command(m);
                chain.started = chain.container_at = now;
                chain.handover = true;
                container_vm = nullptr;
                swofty::log("[main-menu] nav: session-end hand-over landed on the activity container (the F1 World hub was "
                            "skipped); raising '%s' on it once it is active",
                            chain.command);
            }
        }
    }
    if (!std::strcmp(name, "frontend_transition") && Mode(nav.mode.load()) != Mode::none && !off("SWOFTY_MAIN_MENU_RETURN") &&
        entry_by_route()) {
        nav.return_armed = true;
        nav.return_at = GetTickCount64();
        swofty::log("[main-menu] nav: session-end hand-over: the front end is back (frontend_transition) after a session that "
                    "the %s card started; the activity container the hand-over lands on gets that card's command; armed until it "
                    "opens or fe_home does",
                    return_command(Mode(nav.mode.load())));
    }
    {
        Expect e = Expect(nav.expect.load());
        bool container = !std::strcmp(name, "f1_world_activity_container"), saves = !std::strcmp(name, "grand_prix_continue"),
             home = !std::strcmp(name, "fe_home");
        if (e != Expect::none || nav.return_armed.load())
            swofty::log("[main-menu] nav: screen %s opened; flow now %s; expected %s", name, get_flow().c_str(),
                        e == Expect::container ? "the activity container"
                        : e == Expect::saves   ? "the saves list"
                        : e == Expect::home    ? "fe_home"
                                               : "(nothing)");
        if ((container && e == Expect::container) || (saves && e == Expect::saves) || (home && e == Expect::home)) {
            nav.expect = int(Expect::none);
            swofty::log("[main-menu] nav: %s opened %llu ms after it was asked for", name,
                        (unsigned long long)(GetTickCount64() - nav.expect_at.load()));
        }
        if (home) {
            set_flow(FRONTEND_FLOW);
            saves_back_at = 0;
            saves_tabs_at = 0;
            nav.return_armed = false;
            nav.mode = int(Mode::none);
            if (e != Expect::none && e != Expect::home)
                nav.expect = int(Expect::none);
        }
    }
}

}
