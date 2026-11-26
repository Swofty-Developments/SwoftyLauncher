#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <string>

#include "core/frontend/tile_records.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/mfd_pages_point.hpp"
#include "core/hooks/track_select_point.hpp"
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
    auto rec = runtime::older_store().get("ui.route", route);
    if (rec.status != swofty_core::registry::Status::ok)
        return {};
    return frontend::tile_records::text_of(rec.value["opens"]);
}

}

namespace ui_points {

bool wants(const char *point)
{
    Table &t = core();
    return t.has_handlers(*t.direct_point(point));
}

void era_tiles(void *list)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->era))
        fire_list<ev::EraTiles>(core(), list);
}

void home_tiles(void *list)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->home))
        fire_list<ev::HomeTiles>(core(), list);
}

void career_content_tiles(void *chosen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->career_content))
        fire_list<ev::CareerContentTiles>(core(), chosen);
}

void car_category_tiles(void *chosen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (l && active(l->car_category))
        fire_list<ev::CarCategoryTiles>(core(), chosen);
}

void screen_built(const char *screen)
{
    Links *l = g_links.load(std::memory_order_acquire);
    if (!l || !active(l->screen) || !screen || !*screen)
        return;
    void *view = reinterpret_cast<void *>(frontend::ui_vm_node_token(screen));
    fire_screen(core(), screen, view);
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
