#include "core/runtime/runtime.hpp"

#include "core/frontend/main_menu.hpp"
#include "core/frontend/main_menu/context.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/tt_blocked.hpp"

#include <swofty/game.hpp>

#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>

// The main menu in rows. FeTile_Build_List's home case adds the stock tiles in order: Braking Point, Career,
// F1 The Movie, F1 World, the challenge tile, a divider, Local Multiplayer, League Racing, Player Hub, Theatre.
// site_FeHome_AfterCareerCall (home_tiles.cpp) runs right after the Career tile and calls add_tiles, which adds
// Grand Prix and Time Trial as two copies of the F1 World item (their own id, title and description over the
// stock populate), Split Screen as the stock Local Multiplayer item with its own title (its route opens the
// game's Local Multiplayer list, the lone Splitscreen tile), the challenge tile when the game builds it, Theatre,
// Showroom and Customisation (two copies of the Theatre item, each when the launcher's route for it is in the
// build) and a divider; swofty_core then adds Addon Settings last. arm_patch overwrites the first instruction of the F1 World
// block (lea r15d,[r12+1]) with a jmp to the block's final jmp, so F1 World, the divider, League Racing, Player Hub
// and the stock Theatre are never added. The block is matched byte for byte first and the adders are the call
// targets read from it; when anything does not match nothing is patched, no tile is added and the stock menu stays.
//
// The status strip, the sign-in state and the latest video card are painted by the overlay frame
// (paint) from swofty/home_menu.json while the home screen is open. Images (the Discord avatar and the video
// thumbnail) are decoded by the overlay's image loader and drawn as overlay images.
//
// Navigation (notes: main-menu NAV.md). The Time Trial card fires swofty_time_trial, a launcher route in the fe_home
// group (ui/home_nav.flag says the build has it): the game's own home_to_activity with a launcher f1_world_flow entry
// that opens the activity container with the picker's command (swofty_enter_time_trial). The Grand Prix card fires the
// game's own home_to_activity and raises solo_gp on the container once its tabs exist, the command the container's own
// Grand Prix card raises: the game then raises grand_prix_resume and opens grand_prix_continue (the saved seasons and
// New Grand Prix) through the front-end flow's enter_gp_resume, as F1 World > Play > Grand Prix does. The Back of the
// container (f1_world / home), of the Showroom (back_1) and of that saves list (back) is swapped where the game fires
// it for swofty_back_home (the container's home route behind the standard wipe) or swofty_home (the hub's back_1).
// The Customisation card fires swofty_customisation (ui/home_customisation.flag says the build has it), a copy of
// fe_home's own customisation route: close the home screen and its slices, then Migrate customisation_flow
// enter_f1_life_customisation. After that press, the Customisation screen's Back (back, which leads nowhere when the
// screen was opened from the main menu, back_1 to the F1 World hub, or back_2) is swapped for swofty_back_home in its
// group, customisation_f1_life_entry; a Customisation screen reached
// any other way keeps the game's Back. Every Migrate the game runs is logged with its flow and entry
// (nav:), the last accepted flow is kept, and a screen that does not open in time is recovered. A session that
// started from a card hands over (frontend_transition.ui, swofty_enter_return) to the bare activity container, where
// the card's command is raised; session_enter swaps the card's mode for the mode of the session that loaded, as the
// container's tabs can start a Time Trial after a Grand Prix card press (or the other way round).
//
// SWOFTY_MAIN_MENU=0 turns the whole feature off. SWOFTY_MAIN_MENU_NAV=single|rows picks the navigation
// layout; only single (one spawner, tile order) exists so far, and rows is logged as not built.
// SWOFTY_MAIN_MENU_ENTRY=chain opens the pickers the old way (home_to_activity, then the command 900 ms after
// the container is up); SWOFTY_MAIN_MENU_BACK=stock|home|twostep picks the game's Back, its home route or the two-step
// hub Back instead of the launcher's routes. grand_prix_continue opened straight from the home screen
// (swofty_grand_prix_saves, a copy of the theatre route) crashes the game in Game_CareerContinue_ListSlots, so
// neither that route nor swofty_grand_prix is fired;
// SWOFTY_MAIN_MENU_RETURN=0 leaves the session-end hand-over on the bare container; SWOFTY_MAIN_MENU_RECOVER=0 turns
// the recovery of a screen that did not open off; SWOFTY_MAIN_MENU_STATEFIX=0 leaves #ui_menu_data.frontend_transition_state
// alone when leaving the container; SWOFTY_MAIN_MENU_MIGRATELOG=0 leaves the Migrate command unhooked.

namespace frontend::main_menu {

MainMenu menu;

namespace {

using namespace swofty::game;

}

const char *env(const char *name)
{
    const char *v = std::getenv(name);
    return v ? v : "";
}

bool off(const char *name) { return !std::strcmp(env(name), "0"); }

bool enabled() { return menu.tiles.resolve(); }

bool drawing() { return menu.navigation.covering() || menu.panel.due(); }

bool wants_overlay()
{
    if (!std::strcmp(env("SWOFTY_MAIN_MENU"), "0") || !std::strcmp(env("SWOFTY_MAIN_MENU_PANEL"), "0"))
        return false;
    return !runtime::read_file(runtime::under_swofty("home_menu.json"), size_t(1) << 20).empty();
}

void arm_container() { menu.navigation.hook_container(); }

bool arm_patch()
{
    if (!menu.tiles.resolve())
        return false;
    if (menu.patched.load())
        return true;
    if (!menu.tiles.skip_stock_block())
        return false;
    menu.patched = true;
    menu.navigation.install_hooks();
    swofty::log("[main-menu] stock F1 World, divider, League Racing, Player Hub and Theatre tiles skipped");
    return true;
}

uint8_t add_tiles(void *list, uint8_t career_index, const uint8_t *flags)
{
    return menu.tiles.add(static_cast<TileList *>(list), career_index, flags);
}

void style_settings(void *item)
{
    if (menu.tiles.rows_on())
        menu.tiles.style_item(static_cast<HomeItem *>(item));
}

void tick()
{
    if (menu.patched.load())
        menu.navigation.watch();
    menu.navigation.hub_back();
    menu.navigation.report_swap();
    if (menu.home_open.load())
        menu.panel.home_tick();
    menu.panel.sync_strip();
    menu.navigation.tick_chain();
}

void session_enter() { menu.navigation.session_enter(); }

void screen(const char *name)
{
    if (!name)
        return;
    menu.navigation.screen(name);
    Screen kind = classify_screen(name);
    if (kind != Screen::overlay) {
        {
            std::lock_guard<std::mutex> g(menu.view_lock);
            menu.home_view.screen(name);
            menu.home_open = menu.home_view.home;
        }
        bool home = !std::strcmp(name, "fe_home");
        if (home)
            menu.home_opened_at = GetTickCount64();
        if (home && menu.via.exchange(0))
            swofty::log("[main-menu] fe_home screen opened; back-to-menu disarmed (a screen-open event, not proof it is the "
                        "visible screen)");
        if (home && menu.customisation_armed.exchange(false))
            swofty::log("[main-menu] back: fe_home opened after the Customisation card press; the Customisation screen's Back is "
                        "the game's again");
        menu.panel.want_strip(home ? 0 : 1);
        if (home) {
            menu.panel.read_state(true);
            swofty::log("[main-menu] home screen open: state read, stock top-right strip %s",
                        menu.tiles.rows_on() ? "hidden" : "hidden");
        }
    }
}

void command(void *, uint32_t command_hash, uint32_t focused_hash)
{
    if (!menu.patched.load() || command_hash != djb2("select_pressed"))
        return;
    bool tt = focused_hash == djb2(ID_TIME_TRIAL);
    bool gp = focused_hash == djb2(ID_GRAND_PRIX);
    if (!tt && !gp)
        return;
    if (uint64_t at = menu.home_opened_at.load(); at && GetTickCount64() - at < 700)
        return;
    if (tt && tt_blocked::active())
        return;
    if (!tt && !menu.navigation.entry_by_route())
        menu.navigation.start_chain(false);
    if (!tt)
        return;
    menu.panel.read_state();
    State s = menu.panel.snapshot();
    if (!time_trial_warning_due(s, true, menu.tt_warned.load())) {
        if (!menu.navigation.entry_by_route())
            menu.navigation.start_chain(true);
        return;
    }
    menu.tt_warned = true;
    int r = runtime::popup_show(TT_TITLE, TT_BODY, "swofty_core (main menu)");
    swofty::log("[main-menu] Time Trial while not signed in to Discord: warning %s; the pad chain is skipped this once",
                r == 0 ? "queued" : "refused");
}

void paint(hud::drawlist::Frame &f, uint64_t now) { menu.panel.paint(f, now); }

}

namespace runtime {

bool main_menu_account(std::string &name) { return frontend::main_menu::menu.panel.account(name); }

}
