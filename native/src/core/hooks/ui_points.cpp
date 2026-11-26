#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <string>

#include "core/frontend/chosen_tiles.hpp"
#include "core/frontend/tile_records.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/mfd_pages_point.hpp"
#include "core/hooks/track_select_point.hpp"
#include "core/hooks/ui_names.hpp"
#include "core/hooks/ui_points.hpp"
#include "core/registry/older.hpp"
#include "core/registry/store.hpp"

namespace swofty_core::hooks {
namespace {

struct Links {
    ui_points::SiteLink era, home, career_content, car_category, screen, route;
};

std::atomic<Links *> g_links{nullptr};

bool active(const GameLink &l) { return l.installed(); }

std::string route_screen(const char *route)
{
    auto &st = runtime::older_store();
    return ui_names::screen_of_route(
        route, registry::older::in_load_order(st, "ui.route"), registry::older::in_load_order(st, "ui.screen"),
        [&st](const std::string &id) { return frontend::tile_records::text_of(st.get("ui.route", id).value["opens"]); });
}

std::string built_screen(const char *root, const char *registered)
{
    if (!registered || !*registered)
        return root;
    return ui_names::built_screen(root, registered, registry::older::in_load_order(runtime::older_store(), "ui.screen"));
}

}

namespace ui_points {

bool wants(const char *point)
{
    Table &t = core();
    return t.has_handlers(*t.direct_point(point));
}

void era_tiles(void *list, const SwoftyTileList *tiles)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->era))
        fire_list<ev::EraTiles>(core(), list, tiles);
}

namespace {

template <class E> void fire_chosen(Table &t, void *list, void *chosen)
{
    auto *c = static_cast<frontend::chosen_tiles::Chosen *>(chosen);
    if (!c) {
        fire_list<E>(t, list);
        return;
    }
    frontend::chosen_tiles::ChosenTable table(*c);
    fire_list<E>(t, list, table.table());
}

}

void home_tiles(void *list, void *chosen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->home))
        fire_chosen<ev::HomeTiles>(core(), list, chosen);
}

void career_content_tiles(void *chosen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->career_content))
        fire_chosen<ev::CareerContentTiles>(core(), chosen, chosen);
}

void car_category_tiles(void *chosen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->car_category))
        fire_chosen<ev::CarCategoryTiles>(core(), chosen, chosen);
}

void screen_built(const char *root, const char *registered)
{
    const char *screen = root;
    Links *l = g_links.load(std::memory_order_acquire);
    if (!l || !active(l->screen) || !screen || !*screen)
        return;
    void *view = reinterpret_cast<void *>(frontend::ui_vm_node_token(root));
    std::string name = built_screen(root, registered);
    fire_screen(core(), name.c_str(), view);
}

bool route_open(const char *route)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (!l || !active(l->route) || !route || !*route)
        return false;
    std::string screen = route_screen(route);
    return !fire_route(core(), route, screen.c_str());
}

}

void declare_ui_points(Table &t)
{
    auto log = [](const std::string &s) { swofty::log("%s", s.c_str()); };
    auto era_live = [] { return runtime::era_picker_live(); };
    auto *l = new Links{
        {ui_points::ERA_POINT, "site_FeTile_EraTilesBuilt", era_live, log},
        {ui_points::HOME_POINT, "site_FeHome_AfterCareerCall", {}, log},
        {ui_points::CAREER_CONTENT_POINT, "career content select (content_tiles.cpp add_record_tiles)", {}, log},
        {ui_points::CAR_CATEGORY_POINT, "car category tab (car_category.cpp add_record_tiles)", {}, log},
        {ui_points::SCREEN_POINT, "site_Ui_Job_ScreenDone", [] { return runtime::ui_screen_site_live(); }, log},
        {ui_points::ROUTE_POINT, "Vm_Run_Route", [] { return runtime::career_start_live(); }, log},
    };
    t.declare_game(ui_points::ERA_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::EraTiles::version, &l->era);
    t.declare_game(ui_points::HOME_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::HomeTiles::version, &l->home);
    t.declare_game(ui_points::CAREER_CONTENT_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::CareerContentTiles::version,
                   &l->career_content);
    t.declare_game(ui_points::CAR_CATEGORY_POINT, SWOFTY_POINT_MODIFY, swofty::ui::events::CarCategoryTiles::version,
                   &l->car_category);
    t.declare_game(ui_points::SCREEN_POINT, SWOFTY_POINT_NOTIFY, swofty::ui::events::ScreenBuilt::version, &l->screen);
    t.declare_game(ui_points::ROUTE_POINT, SWOFTY_POINT_CANCEL, swofty::ui::events::RouteOpen::version, &l->route);
    g_links.store(l, std::memory_order_release);
    declare_track_select_point(t);
    declare_mfd_pages_point(t);
}

}
