#include <windows.h>

#include "core/career/resume_list.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include <safetyhook.hpp>

#include <swofty/game.hpp>
#include <swofty/mem.hpp>

#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>

namespace frontend::main_menu {

namespace {

using namespace swofty::game;

std::string ne_text(NeStr *s)
{
    auto p = reinterpret_cast<uint8_t *>(s);
    if (!p || !runtime::readable(p, NESTR_CHARS))
        return {};
    uint32_t len = *reinterpret_cast<uint32_t *>(p + 6) & 0xfffff;
    if (len > 200 || !runtime::readable(p + NESTR_CHARS, len + 1))
        return {};
    return std::string(reinterpret_cast<const char *>(p) + NESTR_CHARS, len);
}

const runtime::Sig S_CONTAINER{
    "Game_F1WorldActivityContainer_HandleCommand",
    "41 57 41 56 41 55 41 54 56 57 53 48 81 EC B0 00 00 00 48 8B 02 48 85 C0 0F 84 ?? ?? ?? ?? 4C 89 C7 49 89 D0 48 89 CE "
    "8B 00 3D 37 3B FC FC 0F 8E ?? ?? ?? ?? 3D BD FC E1 41",
    0x1a77d60};

const runtime::Sig S_GOHOME{"Game_F1WorldActivityContainer_GoHome",
                            "56 57 48 83 EC 28 48 89 CE 88 91 45 09 00 00 80 FA 06 74 34 0F B6 C2 83 F8 05 0F 85 2F 01 00 00 80 "
                            "BE 42 09 00 00 00 0F 84 22 01 00 00",
                            0x1a79ee0};

const runtime::Sig S_ADDTABS{"Game_F1WorldActivityContainer_AddTabs",
                             "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC F8 00 00 00 C5 F8 29 B4 24 E0 00 00 00 49 89 CF B1 02 "
                             "E8 ?? ?? ?? ?? 48 8B 05 ?? ?? ?? ?? 80 78 38",
                             0x1a76130};

const runtime::Sig S_MIGRATE{"Ui_RouteCommand_Migrate",
                             "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC C8 00 00 00 4C 89 C7 48 89 D6 48 89 CB 4D 8B 78 50 45 "
                             "8B 70 40 48 8B 0D ?? ?? ?? ?? 48 8B 01 FF 50 48",
                             0x4186240};

const runtime::Sig S_FIRE{"Vm_Fire_Route",
                          "48 8B 4E 28 48 89 F2 49 89 F8 45 31 C9 E8 ?? ?? ?? ?? E9 E4 00 00 00",
                          0x42c0e20,
                          runtime::Sig::rel32,
                          13,
                          14,
                          5};

uint64_t container_detour(void *container, NeStr **message, void *third)
{
    return menu.navigation.container_command(container, message, third);
}

void addtabs_detour(void *vm) { menu.navigation.tabs_added(vm); }

uint64_t gohome_detour(void *vm, uint8_t state) { return menu.navigation.went_home(vm, state); }

NeStr **migrate_detour(void *cmd, NeStr **out, void *node) { return menu.navigation.migrated(cmd, out, node); }

void fire_detour(void *router, void *vm, NeStr **name, uint64_t a4) { menu.navigation.route_fired(router, vm, name, a4); }

}

uint64_t Navigation::container_command(void *container, NeStr **message, void *third)
{
    if (menu.patched.load() && message && *message && back_mode() == Back::route && !off("SWOFTY_MAIN_MENU_PICKERBACK")) {
        Mode m = Mode(nav.mode.load());
        if (m == Mode::time_trial && ne_text(*message) == "entrant_goto_select" && nav.expect.load() == int(Expect::none)) {
            swofty::log("[main-menu] nav: picker Back intercepted (container command entrant_goto_select, which would select the "
                        "default tab; the %s picker's Back leaves for the main menu at once)",
                        return_command(m));
            leave_picker_to_home("picker Back");
            return 0;
        }
    }
    if (container_vm.load() != container) {
        container_vm = container;
        container_vm_at = GetTickCount64();
        swofty::log("[main-menu] chain: activity container view model %p seen", container);
    }
    container_third = third;
    if (nav.expect.load() != int(Expect::none) || chain.step == Step::wait_container || nav.mode.load() != int(Mode::none))
        swofty::log("[main-menu] nav: container command '%s'", message && *message ? ne_text(*message).c_str() : "");
    return h_container.call<uint64_t>(container, message, third);
}

void Navigation::raise_command(void *vm, const char *command, const char *where)
{
    NeStr *name = nullptr;
    Ne_Make_StrFromCStr(&name, command, 0, 0);
    void *third = container_third.load() ? container_third.load() : route_events.third;
    swofty::log("[main-menu] nav: raising '%s' on the activity container %p (%s) at frame %llu (container opened at frame %llu)",
                command, vm, where, (unsigned long long)frame_no.load(), (unsigned long long)open_frame.load());
    uint64_t r = h_container ? h_container.call<uint64_t>(vm, &name, third) : 0;
    Ne_Make_StrFromCStr(&name, nullptr, 0, 0);
    applied_at = GetTickCount64();
    cover_applied_frames = cover_frames.load();
    swofty::log("[main-menu] nav: command %s applied at frame %llu (returned 0x%llx)", command,
                (unsigned long long)frame_no.load(), (unsigned long long)r);
}

void Navigation::apply_pending(void *vm, const char *where)
{
    const char *command = nullptr;
    {
        std::lock_guard<std::mutex> g(chain_lock);
        if (chain.step != Step::wait_container || !*chain.command)
            return;
        if (chain.handover) {
            swofty::log(
                "[main-menu] nav: the hand-over's '%s' waits until the activity container is active (its own first command) (%s)",
                chain.command, where);
            return;
        }
        command = chain.command;
        chain.step = Step::done;
    }
    raise_command(vm, command, where);
}

void Navigation::tabs_added(void *vm)
{
    h_addtabs.call<void>(vm);
    swofty::log("[main-menu] nav: activity container tabs built (AddTabs returned) at frame %llu",
                (unsigned long long)frame_no.load());
    saves_back_tabs();
    apply_pending(vm, "the moment its tabs exist");
}

uint64_t Navigation::went_home(void *vm, uint8_t state)
{
    uint64_t r = h_gohome.call<uint64_t>(vm, state);
    if (state == 5) {
        swofty::log("[main-menu] nav: activity container tabs built at frame %llu", (unsigned long long)frame_no.load());
        saves_back_tabs();
        apply_pending(vm, "the moment its tabs exist");
    }
    return r;
}

NeStr **Navigation::migrated(void *cmd, NeStr **out, void *node)
{
    NeStr **r = h_migrate.call<NeStr **>(cmd, out, node);
    swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
    auto base = static_cast<uint8_t *>(cmd);
    std::string flow, entry, result, before = get_flow();
    if (runtime::readable(base + 0xc8, 16)) {
        flow = ne_text(*reinterpret_cast<NeStr **>(base + 0xc8));
        entry = ne_text(*reinterpret_cast<NeStr **>(base + 0xd0));
    }
    if (r && runtime::readable(r, sizeof(void *)))
        result = ne_text(*r);
    bool failed = migrate_failed(result);
    if (!failed && !flow.empty())
        set_flow(flow);
    if (!failed && customisation_disarms(flow) && menu.customisation_armed.exchange(false))
        swofty::log("[main-menu] back: the %s was entered after the Customisation card press without its Back; the Customisation "
                    "screen's Back is the game's again",
                    flow.c_str());
    if (!failed && flow == F1_WORLD_FLOW) {
        bool pending = false;
        {
            std::lock_guard<std::mutex> g(chain_lock);
            if (chain.step != Step::wait_container && nav.return_armed.exchange(false)) {
                chain = {};
                chain.step = Step::wait_container;
                chain.command = return_command(Mode(nav.mode.load()));
                chain.started = chain.container_at = GetTickCount64();
                chain.handover = true;
                container_vm = nullptr;
                swofty::log("[main-menu] nav: session-end hand-over migrates to the activity container; '%s' is raised on it "
                            "once it is active",
                            chain.command);
            }
            pending = chain.step == Step::wait_container;
        }
        if (pending)
            cover_up("the container is about to open");
    }
    std::string press;
    if (!failed && flow == career::resume_list::SETUP_FLOW && career::resume_list::press_recent(press)) {
        void *frames[4] = {};
        USHORT n = RtlCaptureStackBackTrace(1, 4, frames, nullptr);
        auto base = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
        std::string at;
        for (USHORT i = 0; i < n; i++) {
            char b[24];
            snprintf(b, sizeof b, "%sbase+0x%llx", i ? " < " : "",
                     (unsigned long long)(reinterpret_cast<uintptr_t>(frames[i]) - base));
            at += b;
        }
        swofty::log("[career] resume screen: Migrate to %s within %llu s of a list row press; callers %s; at the press: %s",
                    flow.c_str(), (unsigned long long)(career::resume_list::SETUP_DIAG_MS / 1000), at.c_str(), press.c_str());
    }
    if (migrate_logged(failed, flow, before))
        swofty::log("[main-menu] nav: Migrate command: flow '%s' entry '%s' -> %s%s%s; the flow the game last accepted is now %s",
                    flow.c_str(), entry.empty() ? "(none)" : entry.c_str(), failed ? "FAILED: " : "accepted",
                    failed ? result.c_str() : "", "", get_flow().c_str());
    return r;
}

bool Navigation::fire_in_group(const char *group, const char *route, const char *why)
{
    void *router = last_router.load();
    if (!router || !h_fire) {
        swofty::log("[main-menu] nav: route '%s' in group %s (%s) not fired: no router captured yet", route, group, why);
        return false;
    }
    uint8_t *scratch = route_events.fire;
    *reinterpret_cast<uint32_t *>(scratch + 0x250) = djb2(group);
    NeStr *name = nullptr;
    Ne_Make_StrFromCStr(&name, route, 0, 0);
    swofty::log("[main-menu] nav: firing route '%s' in group %s (%s); flow now %s", route, group, why, get_flow().c_str());
    h_fire.call<void>(router, scratch, &name, 0);
    Ne_Make_StrFromCStr(&name, nullptr, 0, 0);
    return true;
}

void Navigation::route_fired(void *router, void *vm, NeStr **name, uint64_t a4)
{
    last_router = router;
    const char *text = name && *name ? reinterpret_cast<const char *>(*name) + NESTR_CHARS : "";
    uint32_t gid = 0;
    {
        swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
        if (!vm || !swofty::mem::read(reinterpret_cast<uintptr_t>(vm) + 0x250, gid))
            gid = 0;
    }
    if (menu.home_open.load() && gid == djb2("fe_home") && hides_panel(text)) {
        std::lock_guard<std::mutex> g(menu.view_lock);
        menu.home_view.left();
        menu.home_open = false;
        swofty::log("[main-menu] panel hidden: route %s fired from the home screen", text);
    }
    if (menu.patched.load()) {
        if (gid == djb2("f1_world_activity_container") || gid == djb2("fe_showroom") || gid == djb2("grand_prix_continue") ||
            gid == djb2(CUSTOMISATION_GROUP))
            swofty::log("[main-menu] nav: route '%s' fired in group 0x%08x (%s)", text, gid,
                        gid == djb2("fe_showroom")           ? "fe_showroom"
                        : gid == djb2("grand_prix_continue") ? "grand_prix_continue"
                        : gid == djb2(CUSTOMISATION_GROUP)   ? CUSTOMISATION_GROUP
                                                             : "f1_world_activity_container");
        if (saves_back_leaves_home(back_mode(), gid, text, Mode(nav.mode.load()))) {
            saves_tabs_at = 0;
            saves_back_at = GetTickCount64();
            swofty::log("[main-menu] back: the saves list's Back (route back) migrates to the activity container as the game's "
                        "own; the core leaves the container for the main menu once its tabs exist");
        }
        const char *to = back_replacement(back_mode(), gid, text);
        if (!to && (to = customisation_back(back_mode(), menu.customisation_armed.load(), gid, text))) {
            menu.customisation_armed = false;
            swofty::log("[main-menu] back: the Customisation screen was entered from the main menu card; its Back (route %s in "
                        "%s) returns to the main menu",
                        text, CUSTOMISATION_GROUP);
        }
        if (to) {
            clear_entry_point("Back");
            {
                std::lock_guard<std::mutex> g(swap_lock);
                swap_screens.clear();
                swap_reported = false;
            }
            swap_at = GetTickCount64();
            if (std::strcmp(to, HOME_BACK_ROUTE) != 0)
                reset_transition_state(to);
            expect_screen(Expect::home, "Back");
            nav.return_armed = false;
            swofty::log("[main-menu] back: the game's handler fired route %s from its own view model (group 0x%08x); it is "
                        "replaced by %s in the same group, so the F1 World hub is never opened",
                        text, gid, to);
            NeStr *replacement = nullptr;
            Ne_Make_StrFromCStr(&replacement, to, 0, 0);
            h_fire.call<void>(router, vm, &replacement, a4);
            Ne_Make_StrFromCStr(&replacement, nullptr, 0, 0);
            return;
        }
    }
    if (menu.via.load()) {
        swofty::log("[main-menu] route fired while back-to-menu is armed (%s): %s (route group id 0x%08x)",
                    menu.via.load() == 1 ? "card" : "showroom", text, gid);
        if (uint64_t at = swap_at.load(); at && GetTickCount64() - at < 5000)
            swofty::log("[main-menu] route %s fired %llu ms after the back step (vm %p, not replaced)", text,
                        (unsigned long long)(GetTickCount64() - at), vm);
    }
    h_fire.call<void>(router, vm, name, a4);
}

void Navigation::hook_fire()
{
    if (h_fire)
        return;
    auto at = runtime::find(S_FIRE, "home", "core");
    if (!at) {
        swofty::log("[main-menu] back: Vm_Fire_Route did not resolve; Back from the pickers and the Showroom stays stock");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(fire_detour));
    if (!h) {
        swofty::log("[main-menu] back: hooking Vm_Fire_Route failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_fire = std::move(*h);
    swofty::log("[main-menu] back: Vm_Fire_Route hooked at base+0x%llx", (unsigned long long)(at - swofty::api->module_base()));
}

void Navigation::hook_migrate()
{
    if (h_migrate || off("SWOFTY_MAIN_MENU_MIGRATELOG"))
        return;
    auto at = runtime::find(S_MIGRATE, "home", "core");
    if (!at) {
        swofty::log(
            "[main-menu] nav: the route Migrate command did not resolve; migrations are not logged and no flow is tracked");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(migrate_detour));
    if (!h) {
        swofty::log("[main-menu] nav: hooking the route Migrate command failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_migrate = std::move(*h);
    swofty::log("[main-menu] nav: route Migrate command hooked at base+0x%llx",
                (unsigned long long)(at - swofty::api->module_base()));
}

void Navigation::hook_addtabs()
{
    if (h_addtabs)
        return;
    auto at = runtime::find(S_ADDTABS, "home", "core");
    if (!at) {
        swofty::log("[main-menu] nav: the container's AddTabs did not resolve; the command is raised %llu ms after its view "
                    "model is seen",
                    (unsigned long long)raise_delay_ms());
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(addtabs_detour));
    if (!h) {
        swofty::log("[main-menu] nav: hooking the container's AddTabs failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_addtabs = std::move(*h);
    swofty::log("[main-menu] nav: container AddTabs hooked at base+0x%llx",
                (unsigned long long)(at - swofty::api->module_base()));
}

void Navigation::hook_gohome()
{
    if (h_gohome)
        return;
    auto at = runtime::find(S_GOHOME, "home", "core");
    if (!at) {
        swofty::log("[main-menu] nav: the container's tabs-ready callback did not resolve; the command is raised %llu ms after "
                    "its view model is seen",
                    (unsigned long long)raise_delay_ms());
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(gohome_detour));
    if (!h) {
        swofty::log("[main-menu] nav: hooking the container's tabs-ready callback failed: %s",
                    runtime::hook_error(h.error()).c_str());
        return;
    }
    h_gohome = std::move(*h);
    swofty::log("[main-menu] nav: container tabs-ready callback hooked at base+0x%llx",
                (unsigned long long)(at - swofty::api->module_base()));
}

void Navigation::hook_container()
{
    if (h_container)
        return;
    auto at = runtime::find(S_CONTAINER, "home", "core");
    if (!at) {
        swofty::log("[main-menu] chain: the activity container command handler did not resolve; the cards open the activity "
                    "container only");
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(container_detour));
    if (!h) {
        swofty::log("[main-menu] chain: hooking the activity container command handler failed: %s",
                    runtime::hook_error(h.error()).c_str());
        return;
    }
    h_container = std::move(*h);
    swofty::log("[main-menu] chain: activity container command handler hooked at base+0x%llx",
                (unsigned long long)(at - swofty::api->module_base()));
}

void Navigation::install_hooks()
{
    hook_container();
    hook_fire();
    hook_migrate();
    hook_gohome();
    hook_addtabs();
}

}
